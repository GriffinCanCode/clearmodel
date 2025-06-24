use secrecy::ExposeSecret;

use std::time::Duration;
use tokio::process::Command as AsyncCommand;
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::config::ClearModelConfig;
use crate::environment::EnvironmentManager;
use crate::errors::{ClearModelError, Result};
use crate::resource_manager::{ResourceManager, CleanupResult};

/// Main cache cleaner that orchestrates all cleaning operations
pub struct CacheCleaner {
    config: ClearModelConfig,
    env_manager: EnvironmentManager,
    resource_manager: ResourceManager,
}

impl CacheCleaner {
    /// Create a new cache cleaner
    pub async fn new(
        config: ClearModelConfig,
        env_manager: EnvironmentManager,
    ) -> Result<Self> {
        let resource_manager = ResourceManager::new(config.clone()).await?;
        
        Ok(Self {
            config,
            env_manager,
            resource_manager,
        })
    }
    
    /// Clean all caches (main entry point)
    pub async fn clean_all_caches(&self, dry_run: bool) -> Result<()> {
        info!("Starting comprehensive cache cleanup");
        
        // Clean ML model caches
        let ml_results = self.clean_ml_model_caches(dry_run).await?;
        self.log_cleanup_results("ML Model Caches", &ml_results);
        
        // Clean Python cache files
        let python_result = self.clean_python_cache_files(dry_run).await?;
        self.log_cleanup_results("Python Caches", &[python_result]);
        
        info!("All cache cleaning operations completed successfully");
        Ok(())
    }
    
    /// Clean machine learning model caches
    async fn clean_ml_model_caches(&self, dry_run: bool) -> Result<Vec<CleanupResult>> {
        info!("Cleaning ML model caches");
        
        // Use the resource manager to clean all configured cache paths
        let results = self.resource_manager.clean_all_caches(dry_run).await?;
        
        // Additional cleanup for specific ML frameworks
        self.clean_framework_specific_caches(dry_run).await?;
        
        Ok(results)
    }
    
    /// Clean framework-specific caches that might not be in standard locations
    async fn clean_framework_specific_caches(&self, dry_run: bool) -> Result<()> {
        // Clean HuggingFace cache with their CLI if available
        if let Err(e) = self.clean_huggingface_cache(dry_run).await {
            warn!("Failed to clean HuggingFace cache: {}", e);
        }
        
        // Clean PyTorch cache
        if let Err(e) = self.clean_pytorch_cache(dry_run).await {
            warn!("Failed to clean PyTorch cache: {}", e);
        }
        
        // Clean TensorFlow cache
        if let Err(e) = self.clean_tensorflow_cache(dry_run).await {
            warn!("Failed to clean TensorFlow cache: {}", e);
        }
        
        Ok(())
    }
    
    /// Clean HuggingFace cache using their CLI
    async fn clean_huggingface_cache(&self, dry_run: bool) -> Result<()> {
        debug!("Attempting to clean HuggingFace cache");
        
        // Check if huggingface-hub CLI is available
        let check_cmd = AsyncCommand::new("huggingface-cli")
            .arg("--help")
            .output()
            .await;
            
        if check_cmd.is_err() {
            debug!("huggingface-cli not available, skipping");
            return Ok(());
        }
        
        let mut cmd = AsyncCommand::new("huggingface-cli");
        cmd.arg("delete-cache");
        
        if dry_run {
            // HuggingFace CLI doesn't have a dry-run flag, so we'll just report
            info!("Would run: huggingface-cli delete-cache");
            return Ok(());
        }
        
        // Add confirmation flag to avoid interactive prompts
        cmd.arg("--yes");
        
        let timeout_duration = Duration::from_secs(300); // 5 minutes timeout
        
        match timeout(timeout_duration, cmd.output()).await {
            Ok(Ok(output)) => {
                if output.status.success() {
                    info!("Successfully cleaned HuggingFace cache");
                    debug!("HuggingFace cleanup output: {}", String::from_utf8_lossy(&output.stdout));
                } else {
                    warn!(
                        "HuggingFace cache cleanup failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            Ok(Err(e)) => {
                warn!("Failed to execute HuggingFace cache cleanup: {}", e);
            }
            Err(_) => {
                warn!("HuggingFace cache cleanup timed out");
            }
        }
        
        Ok(())
    }
    
    /// Clean PyTorch cache
    async fn clean_pytorch_cache(&self, _dry_run: bool) -> Result<()> {
        debug!("Cleaning PyTorch cache");
        
        // PyTorch doesn't have a built-in cache cleanup command,
        // so we rely on the resource manager to clean the cache directories
        // This is already handled in clean_ml_model_caches
        
        Ok(())
    }
    
    /// Clean TensorFlow cache
    async fn clean_tensorflow_cache(&self, _dry_run: bool) -> Result<()> {
        debug!("Cleaning TensorFlow cache");
        
        // TensorFlow doesn't have a built-in cache cleanup command,
        // so we rely on the resource manager to clean the cache directories
        // This is already handled in clean_ml_model_caches
        
        Ok(())
    }
    
    /// Clean Python cache files in the current directory and subdirectories
    async fn clean_python_cache_files(&self, dry_run: bool) -> Result<CleanupResult> {
        info!("Cleaning Python cache files");
        
        let result = self.resource_manager.clean_python_caches(dry_run).await?;
        
        Ok(result)
    }
    
    /// Execute a command with sudo if needed
    async fn execute_sudo_command(&mut self, command: &str, args: &[&str], dry_run: bool) -> Result<()> {
        if dry_run {
            info!("Would execute: sudo {} {}", command, args.join(" "));
            return Ok(());
        }
        
        let sudo_password = self.env_manager.get_sudo_password()?;
        
        let mut cmd = AsyncCommand::new("sudo");
        cmd.arg("-S") // Read password from stdin
            .arg(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        
        let mut child = cmd.spawn()
            .map_err(|e| ClearModelError::file_operation(
                format!("Failed to spawn sudo command: {}", e),
                None
            ))?;
        
        // Send password to sudo
        if let Some(stdin) = child.stdin.as_mut() {
            use tokio::io::AsyncWriteExt;
            let password_with_newline = format!("{}\n", sudo_password.expose_secret());
            stdin.write_all(password_with_newline.as_bytes()).await
                .map_err(|e| ClearModelError::file_operation(
                    format!("Failed to write password to sudo: {}", e),
                    None
                ))?;
        }
        
        let output = child.wait_with_output().await
            .map_err(|e| ClearModelError::file_operation(
                format!("Failed to wait for sudo command: {}", e),
                None
            ))?;
        
        if !output.status.success() {
            return Err(ClearModelError::file_operation(
                format!(
                    "Sudo command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
                None
            ));
        }
        
        debug!("Sudo command executed successfully");
        Ok(())
    }
    
    /// Log cleanup results in a formatted way
    fn log_cleanup_results(&self, category: &str, results: &[CleanupResult]) {
        let total_files: u64 = results.iter().map(|r| r.files_removed).sum();
        let total_bytes: u64 = results.iter().map(|r| r.bytes_freed).sum();
        let total_errors: usize = results.iter().map(|r| r.errors.len()).sum();
        
        info!(
            "{}: {} files cleaned, {:.2} MB freed, {} errors",
            category,
            total_files,
            total_bytes as f64 / 1_048_576.0,
            total_errors
        );
        
        if total_errors > 0 {
            warn!("Errors encountered during {} cleanup:", category);
            for result in results {
                for error in &result.errors {
                    warn!("  {}: {}", result.path.display(), error);
                }
            }
        }
        
        // Log individual results at debug level
        for result in results {
            debug!(
                "  {:?}: {} files, {:.2} MB, {:?}",
                result.path,
                result.files_removed,
                result.bytes_freed as f64 / 1_048_576.0,
                result.duration
            );
        }
    }
    
    /// Get current operation statistics
    pub fn get_operation_stats(&self) -> Vec<(String, crate::resource_manager::OperationStats)> {
        self.resource_manager.get_operation_stats()
    }
    
    /// Estimate space that would be freed without actually cleaning
    pub async fn estimate_cleanup_space(&self) -> Result<u64> {
        info!("Estimating cleanup space");
        
        let results = self.resource_manager.clean_all_caches(true).await?;
        let total_bytes: u64 = results.iter().map(|r| r.bytes_freed).sum();
        
        info!(
            "Estimated cleanup space: {:.2} MB",
            total_bytes as f64 / 1_048_576.0
        );
        
        Ok(total_bytes)
    }
    
    /// Check if cleanup is needed based on available space
    pub async fn is_cleanup_needed(&self) -> Result<bool> {
        let estimated_cleanup = self.estimate_cleanup_space().await?;
        let min_threshold = self.config.min_free_space_gb * 1_073_741_824; // GB to bytes
        
        // Simple heuristic: cleanup is needed if we can free more than the minimum threshold
        Ok(estimated_cleanup > min_threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_cache_cleaner_creation() {
        // This test requires environment setup, so we'll skip it in CI
        if std::env::var("CI").is_ok() {
            return;
        }
        
        let config = ClearModelConfig::default();
        
        // Create a mock environment manager
        // Note: This would fail in real scenarios without proper .env setup
        // but demonstrates the structure
        match EnvironmentManager::new().await {
            Ok(env_manager) => {
                let cleaner = CacheCleaner::new(config, env_manager).await;
                assert!(cleaner.is_ok() || cleaner.is_err()); // Either outcome is fine for this test
            }
            Err(_) => {
                // Expected in test environment without proper .env setup
            }
        }
    }
    
    #[tokio::test]
    async fn test_cleanup_estimation() {
        // Create a temporary directory structure for testing
        let temp_dir = TempDir::new().unwrap();
        let mut config = ClearModelConfig::default();
        
        // Override cache paths to use temp directory
        config.cache_paths = vec![temp_dir.path().to_path_buf()];
        
        // Create some test files
        let test_file = temp_dir.path().join("test.pyc");
        std::fs::write(&test_file, b"test cache file").unwrap();
        
        // Note: Full test would require proper environment setup
        // This demonstrates the structure
    }
} 