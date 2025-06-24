use secrecy::Secret;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use crate::errors::{ClearModelError, Result};

/// Environment variable registry with validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVarConfig {
    pub required: bool,
    pub description: String,
    pub default: String,
}

/// Secure environment manager with automatic .env loading and validation
pub struct EnvironmentManager {
    env_registry: HashMap<String, EnvVarConfig>,
    sudo_password: Option<Secret<String>>,
}

impl EnvironmentManager {
    /// Create a new environment manager
    pub async fn new() -> Result<Self> {
        let mut manager = Self {
            env_registry: Self::create_env_registry(),
            sudo_password: None,
        };
        
        manager.load_environment().await?;
        Ok(manager)
    }
    
    /// Load environment variables from .env file and validate
    async fn load_environment(&mut self) -> Result<()> {
        // Try to load .env file from internal directory
        let env_path = self.find_env_file()?;
        
        if env_path.exists() {
            // Try to load the .env file, but be tolerant of parsing errors
            match dotenvy::from_path(&env_path) {
                Ok(_) => {
                    info!("Loaded environment from: {:?}", env_path);
                }
                Err(e) => {
                    // If parsing fails, warn but continue - we'll create our own config
                    debug!("Failed to parse .env file with dotenvy: {}", e);
                    info!("Skipping problematic .env file, will create clearmodel-specific config");
                }
            }
        } else {
            // Create default .env file
            self.create_default_env_file(&env_path).await?;
            return Err(ClearModelError::environment(
                format!("Created new .env file at {:?}. Please configure it and run again.", env_path)
            ));
        }
        
        // Validate required environment variables
        self.validate_environment()?;
        
        // Load sensitive data securely
        self.load_secure_data()?;
        
        Ok(())
    }
    
    /// Find the .env file location
    fn find_env_file(&self) -> Result<PathBuf> {
        // Look for clearmodel-specific .env files first to avoid conflicts
        let current_dir = env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."));
            
        // Try clearmodel-specific files first
        let clearmodel_specific_paths = [
            current_dir.join("clearmodel.env"),
            current_dir.join(".clearmodel.env"),
        ];
        
        for path in &clearmodel_specific_paths {
            if path.exists() {
                return Ok(path.clone());
            }
        }
        
        // Check home directory for clearmodel-specific configs
        if let Some(home) = home::home_dir() {
            let home_paths = [
                home.join(".clearmodel.env"),
                home.join(".config/clearmodel/.env"),
                home.join(".config/clearmodel/clearmodel.env"),
            ];
            
            for path in &home_paths {
                if path.exists() {
                    return Ok(path.clone());
                }
            }
        }
        
        // Only check for generic .env if we're in a directory that looks like it belongs to clearmodel
        let current_dir_name = current_dir.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
            
        if current_dir_name == "clearmodel" {
            let generic_env = current_dir.join(".env");
            if generic_env.exists() {
                return Ok(generic_env);
            }
        }
        
        // Default to clearmodel-specific .env in current directory
        Ok(current_dir.join("clearmodel.env"))
    }
    
    /// Create a default .env file with documented variables
    async fn create_default_env_file(&self, env_path: &Path) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = env_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| ClearModelError::file_operation(
                    format!("Failed to create directory: {}", e),
                    Some(parent.to_path_buf())
                ))?;
        }
        
        let mut content = String::from("# Environment Configuration\n\n");
        
        for (var_name, config) in &self.env_registry {
            content.push_str(&format!("# {}\n", config.description));
            if config.required {
                content.push_str("# Required: Yes\n");
            }
            content.push_str(&format!("{}={}\n\n", var_name, config.default));
        }
        
        tokio::fs::write(env_path, content).await
            .map_err(|e| ClearModelError::file_operation(
                format!("Failed to write .env file: {}", e),
                Some(env_path.to_path_buf())
            ))?;
            
        info!("Created default .env file at: {:?}", env_path);
        Ok(())
    }
    
    /// Validate required environment variables
    fn validate_environment(&self) -> Result<()> {
        let mut missing_vars = Vec::new();
        
        for (var_name, config) in &self.env_registry {
            if config.required && env::var(var_name).is_err() {
                missing_vars.push(var_name.clone());
            }
        }
        
        if !missing_vars.is_empty() {
            let mut error_msg = String::from("Missing required environment variables:\n");
            for var in &missing_vars {
                if let Some(config) = self.env_registry.get(var) {
                    error_msg.push_str(&format!("- {} ({})\n", var, config.description));
                }
            }
            return Err(ClearModelError::environment(error_msg));
        }
        
        Ok(())
    }
    
    /// Load sensitive data with proper security measures
    fn load_secure_data(&mut self) -> Result<()> {
        // Load sudo password securely - first try environment variable
        if let Ok(password) = env::var("SUDO_PASSWORD") {
            if !password.is_empty() {
                self.sudo_password = Some(Secret::new(password));
                debug!("Sudo password loaded from environment");
                return Ok(());
            }
        }
        
        // If not in environment, we'll prompt for it when needed
        debug!("Sudo password not found in environment - will prompt when needed");
        
        Ok(())
    }
    
    /// Create the environment variable registry
    fn create_env_registry() -> HashMap<String, EnvVarConfig> {
        let mut registry = HashMap::new();
        
        // Note: SUDO_PASSWORD is no longer required in env - will be prompted for
        registry.insert("SUDO_PASSWORD".to_string(), EnvVarConfig {
            required: false,
            description: "Password for sudo operations (will be prompted if not provided)".to_string(),
            default: "".to_string(),
        });
        
        registry.insert("DEBUG".to_string(), EnvVarConfig {
            required: false,
            description: "Enable debug mode".to_string(),
            default: "false".to_string(),
        });
        
        registry.insert("LOG_LEVEL".to_string(), EnvVarConfig {
            required: false,
            description: "Logging level configuration".to_string(),
            default: "INFO".to_string(),
        });
        
        registry.insert("MAX_PARALLEL_OPERATIONS".to_string(), EnvVarConfig {
            required: false,
            description: "Maximum number of parallel cache operations".to_string(),
            default: "10".to_string(),
        });
        
        registry.insert("CACHE_RETENTION_DAYS".to_string(), EnvVarConfig {
            required: false,
            description: "Number of days to retain cache files".to_string(),
            default: "7".to_string(),
        });
        
        registry
    }
    
    /// Get sudo password securely - prompts if not available
    pub fn get_sudo_password(&mut self) -> Result<&Secret<String>> {
        if self.sudo_password.is_none() {
            self.prompt_for_sudo_password()?;
        }
        
        self.sudo_password.as_ref()
            .ok_or_else(|| ClearModelError::environment(
                "Failed to obtain sudo password".to_string()
            ))
    }
    
    /// Prompt for sudo password securely
    fn prompt_for_sudo_password(&mut self) -> Result<()> {
        print!("Enter sudo password: ");
        io::stdout().flush()
            .map_err(|e| ClearModelError::environment(
                format!("Failed to flush stdout: {}", e)
            ))?;
            
        let password = rpassword::read_password()
            .map_err(|e| ClearModelError::environment(
                format!("Failed to read password: {}", e)
            ))?;
            
        if password.is_empty() {
            return Err(ClearModelError::environment(
                "Empty password provided".to_string()
            ));
        }
        
        self.sudo_password = Some(Secret::new(password));
        debug!("Sudo password obtained from user input");
        
        Ok(())
    }
    
    /// Get an environment variable with default fallback
    pub fn get_env_var(&self, key: &str) -> Option<String> {
        env::var(key).ok().or_else(|| {
            self.env_registry.get(key)
                .filter(|config| !config.default.is_empty())
                .map(|config| config.default.clone())
        })
    }
    
    /// Get an environment variable as integer
    pub fn get_env_var_as_int(&self, key: &str, default: i32) -> i32 {
        self.get_env_var(key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }
    
    /// Get an environment variable as boolean
    pub fn get_env_var_as_bool(&self, key: &str, default: bool) -> bool {
        self.get_env_var(key)
            .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes" | "on"))
            .unwrap_or(default)
    }
    
    /// Get the environment variable registry
    pub fn get_registry(&self) -> &HashMap<String, EnvVarConfig> {
        &self.env_registry
    }
}

impl Drop for EnvironmentManager {
    fn drop(&mut self) {
        // Securely clear sensitive data
        if let Some(password) = self.sudo_password.take() {
            // The Secret type handles secure clearing automatically
            drop(password);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_env_registry_creation() {
        let registry = EnvironmentManager::create_env_registry();
        assert!(registry.contains_key("SUDO_PASSWORD"));
        assert!(registry.get("SUDO_PASSWORD").unwrap().required);
    }
    
    #[tokio::test]
    async fn test_env_var_parsing() {
        env::set_var("TEST_INT", "42");
        env::set_var("TEST_BOOL", "true");
        
        let manager = EnvironmentManager {
            env_registry: HashMap::new(),
            sudo_password: None,
        };
        
        assert_eq!(manager.get_env_var_as_int("TEST_INT", 0), 42);
        assert_eq!(manager.get_env_var_as_bool("TEST_BOOL", false), true);
        assert_eq!(manager.get_env_var_as_bool("NONEXISTENT", true), true);
        
        env::remove_var("TEST_INT");
        env::remove_var("TEST_BOOL");
    }
} 