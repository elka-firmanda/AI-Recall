use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::models::{Memory, MemoryId};

/// Git-style version storage
pub struct VersionStore {
    base_path: PathBuf,
}

/// Version blob (content-addressable storage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionBlob {
    pub hash: String,
    pub content: Vec<u8>,
    pub content_type: String,
    pub size: usize,
}

/// Version reference (points to current blob)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionRef {
    pub memory_id: String,
    pub current_hash: String,
    pub updated_at: DateTime<Utc>,
    pub version_count: usize,
}

/// Version log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionLogEntry {
    pub timestamp: DateTime<Utc>,
    pub hash: String,
    pub parent_hash: Option<String>,
    pub operation: String,
    pub message: String,
    pub author: String,
}

/// Complete version history for a memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistory {
    pub memory_id: String,
    pub current_version: String,
    pub versions: Vec<VersionLogEntry>,
}

/// Diff between two versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDiff {
    pub from_hash: String,
    pub to_hash: String,
    pub content_changed: bool,
    pub content_diff: Option<String>,
    pub metadata_changes: Vec<FieldChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: String,
    pub old_value: String,
    pub new_value: String,
}

impl VersionStore {
    pub fn new(base_path: PathBuf) -> Self {
        let store = Self {
            base_path: base_path.join(".meta").join("versions"),
        };
        store
            .initialize()
            .expect("Failed to initialize version store");
        store
    }

    fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.base_path.join("objects"))?;
        fs::create_dir_all(&self.base_path.join("refs"))?;
        fs::create_dir_all(&self.base_path.join("logs"))?;
        Ok(())
    }

    /// Calculate SHA-256 hash of content
    pub fn calculate_hash(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        format!("{:x}", hasher.finalize())[..16].to_string()
    }

    /// Store a blob and return its hash
    pub fn store_blob(&self, content: &[u8]) -> Result<String> {
        let hash = Self::calculate_hash(content);
        let dir = &hash[..2];
        let filename = &hash[2..];

        let object_path = self.base_path.join("objects").join(dir);
        fs::create_dir_all(&object_path)?;

        let file_path = object_path.join(filename);
        if !file_path.exists() {
            fs::write(&file_path, content)?;
            debug!("Stored blob {}", hash);
        }

        Ok(hash)
    }

    /// Retrieve a blob by hash
    pub fn get_blob(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        if hash.len() < 3 {
            return Ok(None);
        }

        let dir = &hash[..2];
        let filename = &hash[2..];
        let file_path = self.base_path.join("objects").join(dir).join(filename);

        if file_path.exists() {
            Ok(Some(fs::read(&file_path)?))
        } else {
            Ok(None)
        }
    }

    /// Check if a blob exists
    pub fn blob_exists(&self, hash: &str) -> bool {
        if hash.len() < 3 {
            return false;
        }
        let dir = &hash[..2];
        let filename = &hash[2..];
        self.base_path
            .join("objects")
            .join(dir)
            .join(filename)
            .exists()
    }

    /// Create a new version for a memory
    pub fn create_version(&self, memory: &Memory, message: &str, author: &str) -> Result<String> {
        let content = memory
            .content
            .as_ref()
            .map(|c| c.as_bytes())
            .unwrap_or_default();

        // Store content blob
        let content_hash = self.store_blob(content)?;

        // Get previous version for parent hash
        let parent_hash = self.get_current_version(&memory.id)?;

        // Create log entry
        let entry = VersionLogEntry {
            timestamp: Utc::now(),
            hash: content_hash.clone(),
            parent_hash: parent_hash.clone(),
            operation: "commit".to_string(),
            message: message.to_string(),
            author: author.to_string(),
        };

        // Update ref
        let version_count = parent_hash
            .as_ref()
            .map(|_| self.get_version_count(&memory.id).unwrap_or(0) + 1)
            .unwrap_or(1);

        let version_ref = VersionRef {
            memory_id: memory.id.clone(),
            current_hash: content_hash.clone(),
            updated_at: Utc::now(),
            version_count,
        };

        // Save ref
        self.save_ref(&memory.id, &version_ref)?;

        // Append to log
        self.append_to_log(&memory.id, &entry)?;

        info!(
            "Created version {} for memory {} (parent: {:?})",
            content_hash, memory.id, parent_hash
        );

        Ok(content_hash)
    }

    /// Get the current version hash for a memory
    pub fn get_current_version(&self, memory_id: &str) -> Result<Option<String>> {
        let ref_path = self.get_ref_path(memory_id);
        if !ref_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&ref_path)?;
        let version_ref: VersionRef =
            serde_yaml::from_str(&content).with_context(|| "Failed to parse version ref")?;

        Ok(Some(version_ref.current_hash))
    }

    /// Get version count for a memory
    fn get_version_count(&self, memory_id: &str) -> Result<usize> {
        let ref_path = self.get_ref_path(memory_id);
        if !ref_path.exists() {
            return Ok(0);
        }

        let content = fs::read_to_string(&ref_path)?;
        let version_ref: VersionRef = serde_yaml::from_str(&content)?;
        Ok(version_ref.version_count)
    }

    /// Get version history for a memory
    pub fn get_history(&self, memory_id: &str, limit: usize) -> Result<VersionHistory> {
        let current = self.get_current_version(memory_id)?;
        let log = self.read_log(memory_id, limit)?;

        Ok(VersionHistory {
            memory_id: memory_id.to_string(),
            current_version: current.unwrap_or_default(),
            versions: log,
        })
    }

    /// Diff two versions
    pub fn diff_versions(&self, from_hash: &str, to_hash: &str) -> Result<VersionDiff> {
        let from_content = self.get_blob(from_hash)?.unwrap_or_default();
        let to_content = self.get_blob(to_hash)?.unwrap_or_default();

        let from_str = String::from_utf8_lossy(&from_content);
        let to_str = String::from_utf8_lossy(&to_content);

        let content_changed = from_content != to_content;
        let content_diff = if content_changed {
            Some(generate_diff(&from_str, &to_str))
        } else {
            None
        };

        Ok(VersionDiff {
            from_hash: from_hash.to_string(),
            to_hash: to_hash.to_string(),
            content_changed,
            content_diff,
            metadata_changes: vec![], // Would need to store metadata separately
        })
    }

    /// Revert to a specific version
    pub fn revert_to_version(
        &self,
        memory_id: &str,
        target_hash: &str,
        message: &str,
        author: &str,
    ) -> Result<String> {
        // Verify target version exists
        if !self.blob_exists(target_hash) {
            anyhow::bail!("Target version {} does not exist", target_hash);
        }

        // Get the content
        let content = self
            .get_blob(target_hash)?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve blob {}", target_hash))?;
        let content_str = String::from_utf8(content)?;

        // Create a new version pointing to the old content (like git revert)
        let parent_hash = self.get_current_version(memory_id)?;

        let entry = VersionLogEntry {
            timestamp: Utc::now(),
            hash: target_hash.to_string(),
            parent_hash: parent_hash.clone(),
            operation: "revert".to_string(),
            message: format!("Revert to {}: {}", target_hash, message),
            author: author.to_string(),
        };

        // Update ref
        let version_count = self.get_version_count(memory_id)? + 1;
        let version_ref = VersionRef {
            memory_id: memory_id.to_string(),
            current_hash: target_hash.to_string(),
            updated_at: Utc::now(),
            version_count,
        };

        self.save_ref(memory_id, &version_ref)?;
        self.append_to_log(memory_id, &entry)?;

        info!("Reverted memory {} to version {}", memory_id, target_hash);

        Ok(target_hash.to_string())
    }

    /// Save a version reference
    fn save_ref(&self, memory_id: &str, version_ref: &VersionRef) -> Result<()> {
        let ref_path = self.get_ref_path(memory_id);
        fs::create_dir_all(ref_path.parent().unwrap())?;

        let content = serde_yaml::to_string(version_ref)?;
        fs::write(&ref_path, content)?;

        Ok(())
    }

    /// Append entry to version log
    fn append_to_log(&self, memory_id: &str, entry: &VersionLogEntry) -> Result<()> {
        let log_path = self.get_log_path(memory_id);
        fs::create_dir_all(log_path.parent().unwrap())?;

        let entry_str = format!(
            "{} | {} | {} | {} | {}\n",
            entry.timestamp.to_rfc3339(),
            entry.hash,
            entry.operation,
            entry.author,
            entry.message.replace('\n', " ")
        );

        // Append to log file
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        file.write_all(entry_str.as_bytes())?;

        Ok(())
    }

    /// Read version log
    fn read_log(&self, memory_id: &str, limit: usize) -> Result<Vec<VersionLogEntry>> {
        let log_path = self.get_log_path(memory_id);

        if !log_path.exists() {
            return Ok(vec![]);
        }

        let content = fs::read_to_string(&log_path)?;
        let mut entries = Vec::new();

        for line in content.lines().rev().take(limit) {
            // Parse log line: timestamp | hash | operation | author | message
            let parts: Vec<&str> = line.splitn(5, " | ").collect();
            if parts.len() == 5 {
                let timestamp = DateTime::parse_from_rfc3339(parts[0])
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc));

                if let Some(ts) = timestamp {
                    entries.push(VersionLogEntry {
                        timestamp: ts,
                        hash: parts[1].to_string(),
                        parent_hash: None, // Would need to track this separately
                        operation: parts[2].to_string(),
                        author: parts[3].to_string(),
                        message: parts[4].to_string(),
                    });
                }
            }
        }

        // Reverse to get chronological order
        entries.reverse();
        Ok(entries)
    }

    /// Get the path to a memory's ref file
    fn get_ref_path(&self, memory_id: &str) -> PathBuf {
        // Sanitize memory_id for filesystem
        let safe_id = memory_id.replace(['/', '\\', ':'], "_");
        self.base_path
            .join("refs")
            .join(format!("{}.yaml", safe_id))
    }

    /// Get the path to a memory's log file
    fn get_log_path(&self, memory_id: &str) -> PathBuf {
        let safe_id = memory_id.replace(['/', '\\', ':'], "_");
        self.base_path.join("logs").join(format!("{}.log", safe_id))
    }

    /// Get storage statistics
    pub fn get_stats(&self) -> Result<VersionStats> {
        let mut total_objects = 0;
        let mut total_size = 0u64;

        // Count objects
        let objects_dir = self.base_path.join("objects");
        if objects_dir.exists() {
            for entry in walkdir::WalkDir::new(&objects_dir) {
                if let Ok(entry) = entry {
                    if entry.file_type().is_file() {
                        total_objects += 1;
                        if let Ok(metadata) = entry.metadata() {
                            total_size += metadata.len();
                        }
                    }
                }
            }
        }

        // Count refs (versions)
        let refs_dir = self.base_path.join("refs");
        let total_versions = if refs_dir.exists() {
            fs::read_dir(&refs_dir)?.count()
        } else {
            0
        };

        Ok(VersionStats {
            total_objects,
            total_size_bytes: total_size,
            total_versions,
        })
    }
}

/// Version store statistics
#[derive(Debug, Clone)]
pub struct VersionStats {
    pub total_objects: usize,
    pub total_size_bytes: u64,
    pub total_versions: usize,
}

/// Generate a simple text diff
fn generate_diff(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut diff = String::new();
    let max_lines = old_lines.len().max(new_lines.len());

    for i in 0..max_lines {
        let old_line = old_lines.get(i).unwrap_or(&"");
        let new_line = new_lines.get(i).unwrap_or(&"");

        if old_line != new_line {
            if !old_line.is_empty() {
                diff.push_str(&format!("- {}\n", old_line));
            }
            if !new_line.is_empty() {
                diff.push_str(&format!("+ {}\n", new_line));
            }
        }
    }

    diff
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (VersionStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = VersionStore::new(temp_dir.path().to_path_buf());
        (store, temp_dir)
    }

    #[test]
    fn test_calculate_hash() {
        let hash1 = VersionStore::calculate_hash(b"hello");
        let hash2 = VersionStore::calculate_hash(b"hello");
        let hash3 = VersionStore::calculate_hash(b"world");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 16);
    }

    #[test]
    fn test_store_and_retrieve_blob() {
        let (store, _temp) = create_test_store();

        let content = b"test content";
        let hash = store.store_blob(content).unwrap();

        let retrieved = store.get_blob(&hash).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), content);
    }

    #[test]
    fn test_blob_exists() {
        let (store, _temp) = create_test_store();

        let hash = store.store_blob(b"test").unwrap();
        assert!(store.blob_exists(&hash));
        assert!(!store.blob_exists("nonexistent"));
    }

    #[test]
    fn test_generate_diff() {
        let old = "line1\nline2\nline3";
        let new = "line1\nmodified\nline3";

        let diff = generate_diff(old, new);
        assert!(diff.contains("- line2"));
        assert!(diff.contains("+ modified"));
    }
}
