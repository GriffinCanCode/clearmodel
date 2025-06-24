use dashmap::DashMap;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use sysinfo::System;
use tokio::sync::Semaphore;

use tracing::{debug, info, warn, error};

use crate::config::ClearModelConfig;
use crate::errors::{ClearModelError, Result};
use crate::security::SecurityManager;

/// Resource manager for handling cache operations with proper resource management
pub struct ResourceManager {
    config: Arc<ClearModelConfig>,
    semaphore: Arc<Semaphore>,
    system_info: Arc<tokio::sync::Mutex<System>>,
    operation_stats: Arc<DashMap<String, OperationStats>>,
}

/// Statistics for tracking operations
#[derive(Debug, Clone)]
pub struct OperationStats {
    pub files_processed: u64,
    pub bytes_cleaned: u64,
    pub errors_encountered: u64,
    pub start_time: SystemTime,
    pub last_update: SystemTime,
}

impl Default for OperationStats {
    fn default() -> Self {
        let now = SystemTime::now();
        Self {
            files_processed: 0,
            bytes_cleaned: 0,
            errors_encountered: 0,
            start_time: now,
            last_update: now,
        }
    }
}

/// Result of a cache cleaning operation
#[derive(Debug, Clone)]
pub struct CleanupResult {
    pub path: PathBuf,
    pub files_removed: u64,
    pub bytes_freed: u64,
    pub errors: Vec<String>,
    pub duration: Duration,
}

impl ResourceManager {
    /// Create a new resource manager
    pub async fn new(config: ClearModelConfig) -> Result<Self> {
        let max_concurrent = config.max_parallel_operations;
        
        Ok(Self {
            config: Arc::new(config),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            system_info: Arc::new(tokio::sync::Mutex::new(System::new_all())),
            operation_stats: Arc::new(DashMap::new()),
        })
    }
    
    /// Clean all configured cache directories
    pub async fn clean_all_caches(&self, dry_run: bool) -> Result<Vec<CleanupResult>> {
        info!("Starting cache cleanup (dry_run: {})", dry_run);
        
        // Check system resources before starting
        self.check_system_resources().await?;
        
        let cache_paths = self.config.existing_cache_paths();
        if cache_paths.is_empty() {
            warn!("No existing cache directories found");
            return Ok(Vec::new());
        }
        
        info!("Found {} cache directories to clean", cache_paths.len());
        
        // Process cache directories concurrently
        let mut tasks = Vec::new();
        
        for path in cache_paths {
            let path = path.clone();
            let config = Arc::clone(&self.config);
            let semaphore = Arc::clone(&self.semaphore);
            let stats = Arc::clone(&self.operation_stats);
            
            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                Self::clean_cache_directory(&path, &config, &stats, dry_run).await
            });
            
            tasks.push(task);
        }
        
        // Wait for all tasks to complete
        let mut results = Vec::new();
        for task in tasks {
            match task.await {
                Ok(Ok(result)) => results.push(result),
                Ok(Err(e)) => {
                    error!("Cache cleaning task failed: {}", e);
                    // Continue with other tasks
                }
                Err(e) => {
                    error!("Task join error: {}", e);
                    // Continue with other tasks
                }
            }
        }
        
        // Log summary
        let total_files: u64 = results.iter().map(|r| r.files_removed).sum();
        let total_bytes: u64 = results.iter().map(|r| r.bytes_freed).sum();
        
        info!(
            "Cache cleanup completed: {} files, {:.2} MB freed",
            total_files,
            total_bytes as f64 / 1_048_576.0
        );
        
        Ok(results)
    }
    
    /// Clean a specific cache directory
    async fn clean_cache_directory(
        path: &Path,
        config: &ClearModelConfig,
        stats: &DashMap<String, OperationStats>,
        dry_run: bool,
    ) -> Result<CleanupResult> {
        let start_time = SystemTime::now();
        let path_key = path.to_string_lossy().to_string();
        
        // Initialize stats for this operation
        stats.insert(path_key.clone(), OperationStats::default());
        
        info!("Cleaning cache directory: {:?}", path);
        
        // Validate path security
        if config.security.validate_cache_paths {
            SecurityManager::validate_cache_path(path)?;
        }
        
        // Check if path is safe for deletion
        SecurityManager::validate_deletion_safety(path)?;
        
        let mut result = CleanupResult {
            path: path.to_path_buf(),
            files_removed: 0,
            bytes_freed: 0,
            errors: Vec::new(),
            duration: Duration::from_secs(0),
        };
        
        // Process directory contents
        match Self::process_directory_contents(path, config, stats, &path_key, dry_run).await {
            Ok((files, bytes)) => {
                result.files_removed = files;
                result.bytes_freed = bytes;
            }
            Err(e) => {
                result.errors.push(format!("Failed to process directory: {}", e));
            }
        }
        
        result.duration = start_time.elapsed().unwrap_or(Duration::from_secs(0));
        
        info!(
            "Completed cleaning {:?}: {} files, {:.2} MB, took {:?}",
            path,
            result.files_removed,
            result.bytes_freed as f64 / 1_048_576.0,
            result.duration
        );
        
        Ok(result)
    }
    
    /// Process directory contents recursively
    async fn process_directory_contents(
        path: &Path,
        config: &ClearModelConfig,
        stats: &DashMap<String, OperationStats>,
        stats_key: &str,
        dry_run: bool,
    ) -> Result<(u64, u64)> {
        let mut total_files = 0u64;
        let mut total_bytes = 0u64;
        
        // Use walkdir for safe directory traversal
        let walker = walkdir::WalkDir::new(path)
            .max_depth(config.security.max_path_depth)
            .follow_links(config.follow_symlinks)
            .into_iter()
            .filter_entry(|e| {
                // Skip directories that should be ignored
                if let Some(name) = e.file_name().to_str() {
                    !config.skip_directories.contains(&name.to_string())
                } else {
                    true
                }
            });
        
        // Collect entries to process
        let mut entries_to_process = Vec::new();
        
        for entry in walker {
            match entry {
                Ok(entry) => {
                    if entry.file_type().is_file() {
                        entries_to_process.push(entry.path().to_path_buf());
                    }
                }
                Err(e) => {
                    warn!("Error walking directory: {}", e);
                    continue;
                }
            }
        }
        
        // Process files in parallel batches
        let batch_size = 100;
        let batches: Vec<_> = entries_to_process.chunks(batch_size).collect();
        
        for batch in batches {
            let batch_results: Vec<_> = batch
                .par_iter()
                .map(|file_path| {
                    Self::process_single_file(file_path, config, dry_run)
                })
                .collect();
            
            // Aggregate results
            for result in batch_results {
                match result {
                    Ok((files, bytes)) => {
                        total_files += files;
                        total_bytes += bytes;
                    }
                    Err(e) => {
                        debug!("Error processing file: {}", e);
                        // Update error count in stats
                        if let Some(mut stat) = stats.get_mut(stats_key) {
                            stat.errors_encountered += 1;
                        }
                    }
                }
            }
            
            // Update stats
            if let Some(mut stat) = stats.get_mut(stats_key) {
                stat.files_processed += batch.len() as u64;
                stat.bytes_cleaned += total_bytes;
                stat.last_update = SystemTime::now();
            }
            
            // Yield control to allow other tasks to run
            tokio::task::yield_now().await;
        }
        
        Ok((total_files, total_bytes))
    }
    
    /// Process a single file
    fn process_single_file(
        file_path: &Path,
        config: &ClearModelConfig,
        dry_run: bool,
    ) -> Result<(u64, u64)> {
        // Check if file should be cleaned based on age and type
        if !Self::should_clean_file(file_path, config)? {
            return Ok((0, 0));
        }
        
        // Get file size before deletion
        let metadata = std::fs::metadata(file_path)
            .map_err(|e| ClearModelError::file_operation(
                format!("Failed to get file metadata: {}", e),
                Some(file_path.to_path_buf())
            ))?;
        
        let file_size = metadata.len();
        
        if dry_run {
            debug!("Would delete: {:?} ({} bytes)", file_path, file_size);
            return Ok((1, file_size));
        }
        
        // Actually delete the file
        match std::fs::remove_file(file_path) {
            Ok(_) => {
                debug!("Deleted: {:?} ({} bytes)", file_path, file_size);
                Ok((1, file_size))
            }
            Err(e) => {
                Err(ClearModelError::file_operation(
                    format!("Failed to delete file: {}", e),
                    Some(file_path.to_path_buf())
                ))
            }
        }
    }
    
    /// Determine if a file should be cleaned
    fn should_clean_file(file_path: &Path, config: &ClearModelConfig) -> Result<bool> {
        // Check file extension for Python cache files
        if let Some(extension) = file_path.extension().and_then(|s| s.to_str()) {
            let ext_with_dot = format!(".{}", extension);
            if config.python_cache_extensions.contains(&ext_with_dot) {
                return Ok(true);
            }
        }
        
        // Check if file is in __pycache__ directory
        if let Some(parent) = file_path.parent() {
            if parent.file_name().and_then(|s| s.to_str()) == Some("__pycache__") {
                return Ok(true);
            }
        }
        
        // Check file age
        let metadata = std::fs::metadata(file_path)
            .map_err(|e| ClearModelError::file_operation(
                format!("Failed to get file metadata: {}", e),
                Some(file_path.to_path_buf())
            ))?;
        
        if let Ok(modified) = metadata.modified() {
            let age = SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::from_secs(0));
            
            let max_age = Duration::from_secs(config.max_cache_age_days as u64 * 24 * 3600);
            
            if age > max_age {
                return Ok(true);
            }
        }
        
        Ok(false)
    }
    
    /// Check system resources before starting operations
    async fn check_system_resources(&self) -> Result<()> {
        let mut system = self.system_info.lock().await;
        system.refresh_all();
        
        // Check memory usage
        let total_memory = system.total_memory();
        let used_memory = system.used_memory();
        let memory_usage_percent = (used_memory as f64 / total_memory as f64) * 100.0;
        
        if memory_usage_percent > 90.0 {
            warn!("High memory usage: {:.1}%", memory_usage_percent);
        }
        
        debug!(
            "System resources: {:.1}% memory usage",
            memory_usage_percent
        );
        
        // Note: Disk space checking simplified due to API compatibility
        info!("System resource check completed");
        
        Ok(())
    }
    
    /// Get current operation statistics
    pub fn get_operation_stats(&self) -> Vec<(String, OperationStats)> {
        self.operation_stats
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }
    
    /// Clean up Python cache files specifically
    pub async fn clean_python_caches(&self, dry_run: bool) -> Result<CleanupResult> {
        info!("Cleaning Python cache files");
        
        let current_dir = std::env::current_dir()
            .map_err(|e| ClearModelError::file_operation(
                format!("Failed to get current directory: {}", e),
                None
            ))?;
        
        let stats = Arc::clone(&self.operation_stats);
        let config = Arc::clone(&self.config);
        
        Self::clean_cache_directory(&current_dir, &config, &stats, dry_run).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_resource_manager_creation() {
        let config = ClearModelConfig::default();
        let manager = ResourceManager::new(config).await.unwrap();
        assert!(manager.operation_stats.is_empty());
    }
    
    #[tokio::test]
    async fn test_should_clean_file() {
        let temp_dir = TempDir::new().unwrap();
        let config = ClearModelConfig::default();
        
        // Create a .pyc file
        let pyc_file = temp_dir.path().join("test.pyc");
        fs::write(&pyc_file, b"test").unwrap();
        
        assert!(ResourceManager::should_clean_file(&pyc_file, &config).unwrap());
        
        // Create a regular file
        let regular_file = temp_dir.path().join("test.txt");
        fs::write(&regular_file, b"test").unwrap();
        
        // Should not clean regular files unless they're old
        assert!(!ResourceManager::should_clean_file(&regular_file, &config).unwrap());
    }
} 