use thiserror::Error;
use std::path::PathBuf;

/// Custom error types for clearmodel operations
#[derive(Error, Debug)]
pub enum ClearModelError {
    #[error("Configuration error: {message}")]
    Configuration { message: String },
    
    #[error("Environment variable error: {message}")]
    Environment { message: String },
    
    #[error("Path traversal security violation: attempted to access {path}")]
    PathTraversal { path: PathBuf },
    
    #[error("File operation error: {message} (path: {path:?})")]
    FileOperation { message: String, path: Option<PathBuf> },
    
    #[error("Permission denied: {message}")]
    Permission { message: String },
    
    #[error("Resource manager error: {message}")]
    ResourceManager { message: String },
    
    #[error("Cache operation error: {message}")]
    Cache { message: String },
    
    #[error("Security validation failed: {message}")]
    Security { message: String },
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Configuration parsing error: {0}")]
    ConfigParsing(#[from] config::ConfigError),
}

impl ClearModelError {
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }
    
    pub fn environment(message: impl Into<String>) -> Self {
        Self::Environment {
            message: message.into(),
        }
    }
    
    pub fn path_traversal(path: impl Into<PathBuf>) -> Self {
        Self::PathTraversal {
            path: path.into(),
        }
    }
    
    pub fn file_operation(message: impl Into<String>, path: Option<PathBuf>) -> Self {
        Self::FileOperation {
            message: message.into(),
            path,
        }
    }
    
    pub fn permission(message: impl Into<String>) -> Self {
        Self::Permission {
            message: message.into(),
        }
    }
    
    pub fn resource_manager(message: impl Into<String>) -> Self {
        Self::ResourceManager {
            message: message.into(),
        }
    }
    
    pub fn cache(message: impl Into<String>) -> Self {
        Self::Cache {
            message: message.into(),
        }
    }
    
    pub fn security(message: impl Into<String>) -> Self {
        Self::Security {
            message: message.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, ClearModelError>; 