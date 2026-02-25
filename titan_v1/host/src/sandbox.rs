use std::path::{Path, PathBuf};
use std::fs;
use std::io::{self, Read, Write};
use sha2::{Sha256, Digest};
use tempfile::TempDir;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("Access denied: {0}")]
    AccessDenied(String),
    #[error("Path not found: {0}")]
    PathNotFound(String),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    #[error("Permission denied for operation: {0}")]
    PermissionDenied(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Permission {
    Read,
    Write,
    Execute,
    Delete,
}

pub struct Sandbox {
    allowed_paths: Vec<PathBuf>,
    denied_paths: Vec<PathBuf>,
    trash_dir: TempDir,
}

impl Sandbox {
    pub fn new() -> Result<Self, SandboxError> {
        let trash_dir = TempDir::new()?;
        
        let mut sandbox = Sandbox {
            allowed_paths: Vec::new(),
            denied_paths: Vec::new(),
            trash_dir,
        };

        // Setup default denied paths (system directories)
        sandbox.deny_path("/System");
        sandbox.deny_path("/Library/System");
        sandbox.deny_path("/usr/bin");
        sandbox.deny_path("/usr/sbin");
        sandbox.deny_path("/bin");
        sandbox.deny_path("/sbin");
        sandbox.deny_path("/etc");
        sandbox.deny_path("/var/root");
        sandbox.deny_path("/root");
        sandbox.deny_path("/private/etc");
        sandbox.deny_path("/private/var/root");

        Ok(sandbox)
    }

    pub fn allow_path<P: AsRef<Path>>(&mut self, path: P) {
        if let Ok(canonical) = path.as_ref().canonicalize() {
            self.allowed_paths.push(canonical);
        } else {
            self.allowed_paths.push(path.as_ref().to_path_buf());
        }
    }

    pub fn deny_path<P: AsRef<Path>>(&mut self, path: P) {
        if let Ok(canonical) = path.as_ref().canonicalize() {
            self.denied_paths.push(canonical);
        } else {
            self.denied_paths.push(path.as_ref().to_path_buf());
        }
    }

    pub fn validate_path<P: AsRef<Path>>(&self, path: P, permission: Permission) -> Result<PathBuf, SandboxError> {
        let path = path.as_ref();
        
        // Canonicalize the path
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                // If path doesn't exist yet, canonicalize the parent
                if let Some(parent) = path.parent() {
                    if parent.exists() {
                        parent.canonicalize()
                            .map_err(|_| SandboxError::InvalidPath(path.display().to_string()))?
                            .join(path.file_name().unwrap())
                    } else {
                        return Err(SandboxError::PathNotFound(path.display().to_string()));
                    }
                } else {
                    return Err(SandboxError::InvalidPath(path.display().to_string()));
                }
            }
        };

        // Check denied paths first
        for denied in &self.denied_paths {
            if canonical.starts_with(denied) {
                return Err(SandboxError::AccessDenied(
                    format!("Path {} is in denied area {}", canonical.display(), denied.display())
                ));
            }
        }

        // Check allowed paths (if any are configured)
        if !self.allowed_paths.is_empty() {
            let mut allowed = false;
            for allowed_path in &self.allowed_paths {
                if canonical.starts_with(allowed_path) {
                    allowed = true;
                    break;
                }
            }
            if !allowed {
                return Err(SandboxError::AccessDenied(
                    format!("Path {} is not in allowed areas", canonical.display())
                ));
            }
        }

        // Additional checks for destructive operations
        if permission == Permission::Delete {
            // Extra protection: never allow deleting the root of allowed paths
            for allowed_path in &self.allowed_paths {
                if canonical == *allowed_path {
                    return Err(SandboxError::PermissionDenied(
                        "Cannot delete root of allowed workspace".to_string()
                    ));
                }
            }
        }

        Ok(canonical)
    }

    pub fn safe_read<P: AsRef<Path>>(&self, path: P) -> Result<(Vec<u8>, String), SandboxError> {
        let validated_path = self.validate_path(&path, Permission::Read)?;
        
        let mut file = fs::File::open(&validated_path)?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        
        let hash = compute_hash(&contents);
        
        tracing::info!(
            "Read file: {} (size: {} bytes, hash: {})",
            validated_path.display(),
            contents.len(),
            hash
        );

        Ok((contents, hash))
    }

    pub fn safe_write<P: AsRef<Path>>(&self, path: P, contents: &[u8]) -> Result<String, SandboxError> {
        let validated_path = self.validate_path(&path, Permission::Write)?;
        
        // Read old content if exists for audit trail
        let old_hash = if validated_path.exists() {
            match fs::read(&validated_path) {
                Ok(old_contents) => Some(compute_hash(&old_contents)),
                Err(_) => None,
            }
        } else {
            None
        };

        let mut file = fs::File::create(&validated_path)?;
        file.write_all(contents)?;
        
        let new_hash = compute_hash(contents);
        
        tracing::info!(
            "Wrote file: {} (size: {} bytes, old_hash: {:?}, new_hash: {})",
            validated_path.display(),
            contents.len(),
            old_hash,
            new_hash
        );

        Ok(new_hash)
    }

    pub fn safe_delete<P: AsRef<Path>>(&self, path: P) -> Result<String, SandboxError> {
        let validated_path = self.validate_path(&path, Permission::Delete)?;
        
        if !validated_path.exists() {
            return Err(SandboxError::PathNotFound(validated_path.display().to_string()));
        }

        // Compute hash before deletion for audit trail
        let hash = if validated_path.is_file() {
            let contents = fs::read(&validated_path)?;
            compute_hash(&contents)
        } else {
            "directory".to_string()
        };

        // Move to trash instead of permanent deletion
        let file_name = validated_path
            .file_name()
            .ok_or_else(|| SandboxError::InvalidPath("No filename".to_string()))?;
        
        let trash_path = self.trash_dir.path().join(format!(
            "{}_{}",
            chrono::Utc::now().timestamp(),
            file_name.to_string_lossy()
        ));

        fs::rename(&validated_path, &trash_path)?;
        
        tracing::info!(
            "Deleted (moved to trash): {} -> {} (hash: {})",
            validated_path.display(),
            trash_path.display(),
            hash
        );

        Ok(hash)
    }

    pub fn safe_execute<P: AsRef<Path>>(&self, executable: P, args: &[String]) -> Result<String, SandboxError> {
        let validated_path = self.validate_path(&executable, Permission::Execute)?;
        
        if !validated_path.exists() {
            return Err(SandboxError::PathNotFound(validated_path.display().to_string()));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&validated_path)?;
            let permissions = metadata.permissions();
            if permissions.mode() & 0o111 == 0 {
                return Err(SandboxError::PermissionDenied(
                    format!("File {} is not executable", validated_path.display())
                ));
            }
        }

        tracing::info!(
            "Executing: {} with args: {:?}",
            validated_path.display(),
            args
        );

        Ok(validated_path.display().to_string())
    }

    pub fn create_directory<P: AsRef<Path>>(&self, path: P) -> Result<(), SandboxError> {
        let validated_path = self.validate_path(&path, Permission::Write)?;
        
        fs::create_dir_all(&validated_path)?;
        
        tracing::info!("Created directory: {}", validated_path.display());
        
        Ok(())
    }

    pub fn list_directory<P: AsRef<Path>>(&self, path: P) -> Result<Vec<PathBuf>, SandboxError> {
        let validated_path = self.validate_path(&path, Permission::Read)?;
        
        if !validated_path.is_dir() {
            return Err(SandboxError::InvalidPath(
                format!("{} is not a directory", validated_path.display())
            ));
        }

        let mut entries = Vec::new();
        for entry in fs::read_dir(&validated_path)? {
            let entry = entry?;
            entries.push(entry.path());
        }

        tracing::info!(
            "Listed directory: {} ({} entries)",
            validated_path.display(),
            entries.len()
        );

        Ok(entries)
    }

    pub fn get_trash_dir(&self) -> &Path {
        self.trash_dir.path()
    }
}

fn compute_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_sandbox_creation() {
        let sandbox = Sandbox::new().unwrap();
        assert!(!sandbox.denied_paths.is_empty());
    }

    #[test]
    fn test_denied_paths() {
        let sandbox = Sandbox::new().unwrap();
        let result = sandbox.validate_path("/etc/passwd", Permission::Read);
        assert!(result.is_err());
    }

    #[test]
    fn test_safe_operations() {
        let mut sandbox = Sandbox::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        sandbox.allow_path(temp_dir.path());

        let test_file = temp_dir.path().join("test.txt");
        let test_data = b"Hello, Sandbox!";

        // Write
        let write_hash = sandbox.safe_write(&test_file, test_data).unwrap();
        assert!(!write_hash.is_empty());

        // Read
        let (read_data, read_hash) = sandbox.safe_read(&test_file).unwrap();
        assert_eq!(read_data, test_data);
        assert_eq!(read_hash, write_hash);

        // Delete
        let delete_hash = sandbox.safe_delete(&test_file).unwrap();
        assert_eq!(delete_hash, write_hash);
        assert!(!test_file.exists());
    }

    #[test]
    fn test_compute_hash() {
        let data = b"test data";
        let hash = compute_hash(data);
        assert_eq!(hash.len(), 64); // SHA256 produces 64 hex characters
    }
}
