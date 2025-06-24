use config::{Config, Environment, File};
use home::home_dir;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use crate::errors::{ClearModelError, Result};

/// Configuration for the clearmodel application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearModelConfig {
    /// Cache directories to clean
    pub cache_paths: Vec<PathBuf>,
    
    /// Maximum age of cache files in days
    pub max_cache_age_days: u32,
    
    /// Maximum number of parallel operations
    pub max_parallel_operations: usize,
    
    /// Whether to follow symbolic links
    pub follow_symlinks: bool,
    
    /// File extensions to target for Python cache cleanup
    pub python_cache_extensions: Vec<String>,
    
    /// Directories to skip during cleanup
    pub skip_directories: Vec<String>,
    
    /// Minimum free space threshold (in GB) before cleanup
    pub min_free_space_gb: u64,
    
    /// Whether to perform dry run by default
    pub default_dry_run: bool,
    
    /// Logging configuration
    pub log_level: String,
    
    /// Security settings
    pub security: SecurityConfig,
}

/// Security-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Whether to validate cache paths
    pub validate_cache_paths: bool,
    
    /// Whether to check for path traversal attempts
    pub check_path_traversal: bool,
    
    /// Maximum path depth to traverse
    pub max_path_depth: usize,
    
    /// Whether to require confirmation for large deletions
    pub require_confirmation_threshold_gb: Option<u64>,
}

impl Default for ClearModelConfig {
    fn default() -> Self {
        Self {
            cache_paths: Self::default_cache_paths(),
            max_cache_age_days: 7,
            max_parallel_operations: 10,
            follow_symlinks: false,
            python_cache_extensions: vec![
                ".pyc".to_string(),
                ".pyo".to_string(),
                ".pyd".to_string(),
            ],
            skip_directories: vec![
                ".git".to_string(),
                ".svn".to_string(),
                "node_modules".to_string(),
                ".venv".to_string(),
                "venv".to_string(),
                "__pycache__".to_string(),
            ],
            min_free_space_gb: 1,
            default_dry_run: false,
            log_level: "info".to_string(),
            security: SecurityConfig::default(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            validate_cache_paths: true,
            check_path_traversal: true,
            max_path_depth: 20,
            require_confirmation_threshold_gb: Some(10),
        }
    }
}

impl ClearModelConfig {
    /// Load configuration from file or create default
    pub async fn load(config_path: Option<&str>) -> Result<Self> {
        let mut config_builder = Config::builder();
        
        // Start with defaults
        config_builder = config_builder.add_source(Config::try_from(&ClearModelConfig::default())?);
        
        // Try to load from various configuration file locations
        let config_paths = if let Some(path) = config_path {
            vec![PathBuf::from(path)]
        } else {
            Self::default_config_paths()
        };
        
        for path in config_paths {
            if path.exists() {
                info!("Loading configuration from: {:?}", path);
                config_builder = config_builder.add_source(
                    File::from(path.clone())
                        .required(false)
                        .format(Self::detect_config_format(&path))
                );
                break;
            }
        }
        
        // Override with environment variables
        config_builder = config_builder.add_source(
            Environment::with_prefix("CLEARMODEL")
                .prefix_separator("_")
                .separator("__")
        );
        
        let config = config_builder.build()
            .map_err(|e| ClearModelError::configuration(
                format!("Failed to build configuration: {}", e)
            ))?;
            
        let clearmodel_config: ClearModelConfig = config.try_deserialize()
            .map_err(|e| ClearModelError::configuration(
                format!("Failed to deserialize configuration: {}", e)
            ))?;
        
        debug!("Loaded configuration: {:#?}", clearmodel_config);
        clearmodel_config.validate()?;
        
        Ok(clearmodel_config)
    }
    
    /// Validate the configuration
    fn validate(&self) -> Result<()> {
        if self.cache_paths.is_empty() {
            return Err(ClearModelError::configuration(
                "No cache paths configured".to_string()
            ));
        }
        
        if self.max_parallel_operations == 0 {
            return Err(ClearModelError::configuration(
                "max_parallel_operations must be greater than 0".to_string()
            ));
        }
        
        if self.security.max_path_depth == 0 {
            return Err(ClearModelError::configuration(
                "max_path_depth must be greater than 0".to_string()
            ));
        }
        
        // Validate cache paths exist or can be created
        for path in &self.cache_paths {
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    return Err(ClearModelError::configuration(
                        format!("Parent directory does not exist: {:?}", parent)
                    ));
                }
            }
        }
        
        Ok(())
    }
    
    /// Get default cache paths based on the operating system
    fn default_cache_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        
        if let Some(home) = home_dir() {
            // Common ML cache directories
            let cache_dirs = [
                ".cache/huggingface",
                ".cache/torch",
                ".cache/tensorflow",
                ".cache/keras",
                ".cache/transformers",
                ".cache/anthropic",
                ".cache/openai",
                ".cache/pytorch",
                ".cache/models",
                ".keras",
                ".transformers",
            ];
            
            for dir in &cache_dirs {
                paths.push(home.join(dir));
            }
            
            // Platform-specific paths
            if cfg!(target_os = "macos") {
                let macos_cache_dirs = [
                    "Library/Caches/torch",
                    "Library/Caches/tensorflow",
                    "Library/Caches/models",
                ];
                
                for dir in &macos_cache_dirs {
                    paths.push(home.join(dir));
                }
            }
        }
        
        paths
    }
    
    /// Get default configuration file paths
    fn default_config_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        
        // Current directory
        paths.push(PathBuf::from("clearmodel.toml"));
        paths.push(PathBuf::from("clearmodel.yaml"));
        paths.push(PathBuf::from("clearmodel.json"));
        
        // Home directory
        if let Some(home) = home_dir() {
            paths.push(home.join(".clearmodel.toml"));
            paths.push(home.join(".clearmodel.yaml"));
            paths.push(home.join(".clearmodel.json"));
            
            // XDG config directory
            let config_dir = home.join(".config").join("clearmodel");
            paths.push(config_dir.join("config.toml"));
            paths.push(config_dir.join("config.yaml"));
            paths.push(config_dir.join("config.json"));
        }
        
        paths
    }
    
    /// Detect configuration file format based on extension
    fn detect_config_format(path: &Path) -> config::FileFormat {
        match path.extension().and_then(|s| s.to_str()) {
            Some("toml") => config::FileFormat::Toml,
            Some("yaml") | Some("yml") => config::FileFormat::Yaml,
            Some("json") => config::FileFormat::Json,
            _ => config::FileFormat::Toml, // Default to TOML
        }
    }
    
    /// Save configuration to file
    pub async fn save(&self, path: &Path) -> Result<()> {
        let format = Self::detect_config_format(path);
        
        let content = match format {
            config::FileFormat::Toml => {
                toml::to_string_pretty(self)
                    .map_err(|e| ClearModelError::configuration(
                        format!("Failed to serialize to TOML: {}", e)
                    ))?
            }
            config::FileFormat::Yaml => {
                serde_yaml::to_string(self)
                    .map_err(|e| ClearModelError::configuration(
                        format!("Failed to serialize to YAML: {}", e)
                    ))?
            }
            config::FileFormat::Json => {
                serde_json::to_string_pretty(self)
                    .map_err(|e| ClearModelError::configuration(
                        format!("Failed to serialize to JSON: {}", e)
                    ))?
            }
            _ => {
                return Err(ClearModelError::configuration(
                    "Unsupported configuration format".to_string()
                ));
            }
        };
        
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| ClearModelError::file_operation(
                    format!("Failed to create config directory: {}", e),
                    Some(parent.to_path_buf())
                ))?;
        }
        
        tokio::fs::write(path, content).await
            .map_err(|e| ClearModelError::file_operation(
                format!("Failed to write config file: {}", e),
                Some(path.to_path_buf())
            ))?;
            
        info!("Configuration saved to: {:?}", path);
        Ok(())
    }
    
    /// Get cache paths that actually exist
    pub fn existing_cache_paths(&self) -> Vec<&PathBuf> {
        self.cache_paths
            .iter()
            .filter(|path| path.exists())
            .collect()
    }
    
    /// Get cache paths with their sizes
    pub async fn cache_paths_with_sizes(&self) -> Result<Vec<(PathBuf, u64)>> {
        let mut results = Vec::new();
        
        for path in &self.cache_paths {
            if path.exists() {
                let size = Self::calculate_directory_size(path).await?;
                results.push((path.clone(), size));
            }
        }
        
        Ok(results)
    }
    
    /// Calculate the total size of a directory
    fn calculate_directory_size(path: &Path) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + '_>> {
        Box::pin(async move {
            let mut total_size = 0u64;
            
            let mut entries = tokio::fs::read_dir(path).await
                .map_err(|e| ClearModelError::file_operation(
                    format!("Failed to read directory: {}", e),
                    Some(path.to_path_buf())
                ))?;
                
            while let Some(entry) = entries.next_entry().await
                .map_err(|e| ClearModelError::file_operation(
                    format!("Failed to read directory entry: {}", e),
                    Some(path.to_path_buf())
                ))? {
                
                let metadata = entry.metadata().await
                    .map_err(|e| ClearModelError::file_operation(
                        format!("Failed to get metadata: {}", e),
                        Some(entry.path())
                    ))?;
                    
                if metadata.is_file() {
                    total_size += metadata.len();
                } else if metadata.is_dir() {
                    total_size += Self::calculate_directory_size(&entry.path()).await?;
                }
            }
            
            Ok(total_size)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_default_config() {
        let config = ClearModelConfig::default();
        assert!(config.validate().is_ok());
        assert!(!config.cache_paths.is_empty());
        assert!(config.max_parallel_operations > 0);
    }
    
    #[tokio::test]
    async fn test_config_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");
        
        let original_config = ClearModelConfig::default();
        original_config.save(&config_path).await.unwrap();
        
        let loaded_config = ClearModelConfig::load(Some(config_path.to_str().unwrap())).await.unwrap();
        assert_eq!(original_config.max_cache_age_days, loaded_config.max_cache_age_days);
    }
} 