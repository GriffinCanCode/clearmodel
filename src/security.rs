use camino::{Utf8Path, Utf8PathBuf};
use path_clean::PathClean;
use sanitize_filename::sanitize;
use std::path::{Component, Path, PathBuf};
use tracing::{debug, warn};

use crate::errors::{ClearModelError, Result};

/// Security utilities for safe path operations and traversal protection
pub struct SecurityManager;

impl SecurityManager {
    /// Validate and sanitize a path to prevent path traversal attacks
    /// 
    /// This function implements multiple layers of security:
    /// 1. Normalizes the path to resolve .. and . components
    /// 2. Validates that the path doesn't escape the allowed base directory
    /// 3. Sanitizes filename components
    /// 4. Ensures UTF-8 compliance for cross-platform compatibility
    pub fn validate_and_sanitize_path(
        path: &Path,
        allowed_base: &Path,
    ) -> Result<PathBuf> {
        debug!("Validating path: {:?} against base: {:?}", path, allowed_base);
        
        // Convert to absolute paths for proper comparison
        let abs_path = path.canonicalize()
            .or_else(|_| {
                // If canonicalize fails (path doesn't exist), try manual resolution
                Self::resolve_path_manually(path)
            })
            .map_err(|e| ClearModelError::file_operation(
                format!("Failed to resolve path: {}", e),
                Some(path.to_path_buf())
            ))?;
            
        let abs_base = allowed_base.canonicalize()
            .map_err(|e| ClearModelError::file_operation(
                format!("Failed to resolve base path: {}", e),
                Some(allowed_base.to_path_buf())
            ))?;
        
        // Check if the resolved path is within the allowed base directory
        if !abs_path.starts_with(&abs_base) {
            warn!("Path traversal attempt detected: {:?} is outside {:?}", abs_path, abs_base);
            return Err(ClearModelError::path_traversal(abs_path));
        }
        
        // Additional validation: check for suspicious patterns
        Self::validate_path_components(&abs_path)?;
        
        debug!("Path validation successful: {:?}", abs_path);
        Ok(abs_path)
    }
    
    /// Manually resolve a path by cleaning and normalizing it
    fn resolve_path_manually(path: &Path) -> std::io::Result<PathBuf> {
        let cleaned = path.clean();
        
        // If it's relative, make it absolute relative to current dir
        if cleaned.is_relative() {
            let current_dir = std::env::current_dir()?;
            Ok(current_dir.join(cleaned))
        } else {
            Ok(cleaned)
        }
    }
    
    /// Validate individual path components for security
    fn validate_path_components(path: &Path) -> Result<()> {
        for component in path.components() {
            match component {
                Component::Normal(name) => {
                    let name_str = name.to_string_lossy();
                    
                    // Check for hidden files/directories (optional - may be legitimate)
                    if name_str.starts_with('.') {
                        debug!("Hidden file/directory detected: {}", name_str);
                    }
                    
                    // Check for suspicious patterns
                    if name_str.contains("..") || name_str.contains("./") {
                        return Err(ClearModelError::security(
                            format!("Suspicious path component: {}", name_str)
                        ));
                    }
                    
                    // Validate filename characters
                    let sanitized = sanitize(&name_str);
                    if sanitized != name_str {
                        warn!("Path component contains suspicious characters: {}", name_str);
                    }
                }
                Component::ParentDir => {
                    // This should have been resolved by canonicalize/clean
                    return Err(ClearModelError::security(
                        "Unresolved parent directory component found".to_string()
                    ));
                }
                Component::CurDir => {
                    // This should have been resolved by canonicalize/clean
                    return Err(ClearModelError::security(
                        "Unresolved current directory component found".to_string()
                    ));
                }
                _ => {} // Root, Prefix are generally safe
            }
        }
        Ok(())
    }
    
    /// Create a secure UTF-8 path with validation
    pub fn create_secure_utf8_path(path: &str, base: &Utf8Path) -> Result<Utf8PathBuf> {
        // First sanitize the input string
        let sanitized = sanitize(path);
        
        // Create a path from the sanitized string
        let candidate_path = Utf8Path::new(&sanitized);
        
        // Ensure it's relative (security measure)
        if candidate_path.is_absolute() {
            return Err(ClearModelError::security(
                "Absolute paths not allowed in this context".to_string()
            ));
        }
        
        // Join with base and validate
        let full_path = base.join(candidate_path);
        
        // Convert to standard Path for validation
        let std_path = Path::new(full_path.as_str());
        let base_std = Path::new(base.as_str());
        
        // Validate using our standard security checks
        let validated = Self::validate_and_sanitize_path(std_path, base_std)?;
        
        // Convert back to UTF-8 path
        Utf8PathBuf::try_from(validated)
            .map_err(|_| ClearModelError::security(
                "Path contains non-UTF-8 characters".to_string()
            ))
    }
    
    /// Check if a path is safe for deletion operations
    pub fn validate_deletion_safety(path: &Path) -> Result<()> {
        // Prevent deletion of critical system paths
        let dangerous_paths = [
            "/",
            "/bin",
            "/boot",
            "/dev",
            "/etc",
            "/lib",
            "/proc",
            "/root",
            "/sbin",
            "/sys",
            "/usr",
            "/var/log",
            "/var/lib",
        ];
        
        let path_str = path.to_string_lossy();
        for dangerous in &dangerous_paths {
            if path_str.starts_with(dangerous) && path_str.len() <= dangerous.len() + 1 {
                return Err(ClearModelError::security(
                    format!("Attempted to delete critical system path: {}", path_str)
                ));
            }
        }
        
        // Additional checks for macOS system paths
        if cfg!(target_os = "macos") {
            let macos_dangerous = [
                "/System",
                "/Library/System",
                "/Applications/Utilities",
            ];
            
            for dangerous in &macos_dangerous {
                if path_str.starts_with(dangerous) {
                    return Err(ClearModelError::security(
                        format!("Attempted to delete critical macOS system path: {}", path_str)
                    ));
                }
            }
        }
        
        Ok(())
    }
    
    /// Validate that a path is within expected cache directories
    pub fn validate_cache_path(path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy().to_lowercase();
        
        // Check if path contains cache-related keywords
        let cache_indicators = [
            "cache", "tmp", "temp", ".cache", "huggingface", 
            "torch", "tensorflow", "keras", "transformers",
            "anthropic", "openai", "pytorch", "models"
        ];
        
        let is_cache_path = cache_indicators.iter()
            .any(|indicator| path_str.contains(indicator));
            
        if !is_cache_path {
            warn!("Path doesn't appear to be a cache directory: {:?}", path);
            // Don't fail, but warn - user might have custom cache locations
        }
        
        // Ensure we're not trying to clean user data directories
        let user_data_indicators = [
            "documents", "desktop", "downloads", "pictures", 
            "music", "videos", "dropbox", "googledrive"
        ];
        
        let is_user_data = user_data_indicators.iter()
            .any(|indicator| path_str.contains(indicator));
            
        if is_user_data {
            return Err(ClearModelError::security(
                format!("Refusing to clean user data directory: {:?}", path)
            ));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    
    #[test]
    fn test_path_traversal_prevention() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();
        
        // Test cases that should fail
        let malicious_paths = [
            "../../../etc/passwd",
            "cache/../../../home/user",
            "/etc/passwd",
            "normal/../../etc/shadow",
        ];
        
        for malicious in &malicious_paths {
            let path = base.join(malicious);
            let result = SecurityManager::validate_and_sanitize_path(&path, base);
            assert!(result.is_err(), "Should reject malicious path: {}", malicious);
        }
    }
    
    #[test]
    fn test_valid_paths() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();
        
        // Create a test subdirectory
        let cache_dir = base.join("cache");
        fs::create_dir_all(&cache_dir).unwrap();
        
        let valid_paths = [
            "cache",
            "cache/models",
            "cache/huggingface/transformers",
        ];
        
        for valid in &valid_paths {
            let path = base.join(valid);
            fs::create_dir_all(&path).unwrap();
            let result = SecurityManager::validate_and_sanitize_path(&path, base);
            assert!(result.is_ok(), "Should accept valid path: {}", valid);
        }
    }
} 