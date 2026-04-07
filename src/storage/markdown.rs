use anyhow::{Context, Result};
use chrono::Utc;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use walkdir::WalkDir;

use crate::config::StorageConfig;
use crate::models::{Memory, MemoryFrontmatter, MemoryId, MemoryType};

/// Storage backend for markdown files
pub struct MarkdownStorage {
    config: StorageConfig,
}

impl MarkdownStorage {
    pub fn new(config: StorageConfig) -> Self {
        Self { config }
    }

    /// Initialize storage directories
    pub fn initialize(&self) -> Result<()> {
        let data_dir = &self.config.data_dir;

        // Create main directories
        let dirs = [
            data_dir.join("wiki"),
            data_dir.join("wiki/semantic"),
            data_dir.join("wiki/profile"),
            data_dir.join("wiki/procedural"),
            data_dir.join("wiki/working"),
            data_dir.join("wiki/episodic"),
            data_dir.join("raw"),
            data_dir.join("raw/articles"),
            data_dir.join("raw/papers"),
            data_dir.join("raw/notes"),
            data_dir.join(".meta"),
            data_dir.join(".meta/versions"),
            data_dir.join(".meta/versions/objects"),
            data_dir.join(".meta/versions/refs"),
            data_dir.join(".tombstones"),
        ];

        for dir in &dirs {
            fs::create_dir_all(dir)
                .with_context(|| format!("Failed to create directory: {:?}", dir))?;
        }

        // Create initial index.md if it doesn't exist
        let index_path = data_dir.join(".meta/index.md");
        if !index_path.exists() {
            self.create_initial_index(&index_path)?;
        }

        info!("Storage initialized at {:?}", data_dir);
        Ok(())
    }

    fn create_initial_index(&self, path: &Path) -> Result<()> {
        let content = format!(
            "# AI Recall Index\n\nThis is the index of all memories in your knowledge base.\n\n## Last Updated: {}\n\n## Memories by Type\n\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S")
        );
        fs::write(path, content)?;
        Ok(())
    }

    /// Write a memory to storage
    pub fn write_memory(&self, memory: &Memory) -> Result<PathBuf> {
        let dir = self.config.data_dir.join(memory.memory_type.directory());
        let filename = Self::sanitize_filename(&memory.title);
        let file_path = dir.join(format!("{}.md", filename));

        fs::create_dir_all(&dir)?;

        let frontmatter = MemoryFrontmatter::from(memory);
        let yaml = serde_yaml::to_string(&frontmatter)?;

        let content = format!(
            "---\n{}---\n\n{}\n",
            yaml,
            memory.content.as_deref().unwrap_or("")
        );

        fs::write(&file_path, content)
            .with_context(|| format!("Failed to write memory to {:?}", file_path))?;

        debug!("Wrote memory to {:?}", file_path);
        Ok(file_path)
    }

    /// Read a memory by ID
    pub fn read_memory(&self, id: &MemoryId) -> Result<Option<Memory>> {
        // Find memory by searching all wiki directories
        let wiki_dir = self.config.data_dir.join("wiki");

        for entry in WalkDir::new(&wiki_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() && entry.path().extension() == Some("md".as_ref()) {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if let Ok(memory) = Self::parse_memory(&content, entry.path()) {
                        if memory.id == *id {
                            return Ok(Some(memory));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Read a memory by file path
    pub fn read_memory_by_path(&self, relative_path: &str) -> Result<Memory> {
        let full_path = self.config.data_dir.join(relative_path);
        let content = fs::read_to_string(&full_path)
            .with_context(|| format!("Failed to read memory from {:?}", full_path))?;

        Self::parse_memory(&content, &full_path)
    }

    /// Parse markdown content into Memory struct
    fn parse_memory(content: &str, path: &Path) -> Result<Memory> {
        // Extract frontmatter between --- markers
        // (?s) enables dotall mode so . matches newlines
        let re = Regex::new(r"(?s)^---\s*\n(.*?)\n---\s*\n(.*)$").unwrap();

        if let Some(captures) = re.captures(content) {
            let frontmatter_str = captures.get(1).unwrap().as_str();
            let body = captures.get(2).map(|m| m.as_str().trim()).unwrap_or("");

            let frontmatter: MemoryFrontmatter = serde_yaml::from_str(frontmatter_str)
                .with_context(|| "Failed to parse frontmatter")?;

            Ok(Memory {
                id: frontmatter.id,
                title: frontmatter.title,
                memory_type: frontmatter.memory_type,
                content: Some(body.to_string()),
                file_path: path.to_string_lossy().to_string(),
                created_at: frontmatter.created_at,
                updated_at: frontmatter.updated_at,
                tags: frontmatter.tags,
                confidence: frontmatter.confidence,
                source_refs: frontmatter.source_refs,
                version_hash: frontmatter.version_hash,
                related_memories: vec![],
                embedding_model: frontmatter.embedding_model,
                embedding_dimension: frontmatter.embedding_dimension,
            })
        } else {
            anyhow::bail!("No frontmatter found in markdown file")
        }
    }

    /// Delete a memory (soft delete with tombstone)
    pub fn delete_memory(&self, id: &MemoryId, permanent: bool) -> Result<bool> {
        if let Some(memory) = self.read_memory(id)? {
            let path = PathBuf::from(&memory.file_path);

            if permanent {
                fs::remove_file(&path)?;
                info!("Permanently deleted memory {} at {:?}", id, path);
            } else {
                // Soft delete: move to tombstones
                let tombstone_dir = self.config.data_dir.join(".tombstones");
                fs::create_dir_all(&tombstone_dir)?;

                let tombstone_path =
                    tombstone_dir.join(format!("{}_{}.md", id, Utc::now().timestamp()));
                fs::rename(&path, &tombstone_path)?;

                info!("Soft deleted memory {} to {:?}", id, tombstone_path);
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// List all memories
    pub fn list_memories(&self, memory_type: Option<MemoryType>) -> Result<Vec<Memory>> {
        let mut memories = Vec::new();

        let base_dirs = if let Some(t) = memory_type {
            vec![self.config.data_dir.join(t.directory())]
        } else {
            vec![
                self.config.data_dir.join("wiki/semantic"),
                self.config.data_dir.join("wiki/profile"),
                self.config.data_dir.join("wiki/procedural"),
                self.config.data_dir.join("wiki/working"),
                self.config.data_dir.join("wiki/episodic"),
            ]
        };

        for dir in base_dirs {
            if !dir.exists() {
                continue;
            }

            for entry in WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() && entry.path().extension() == Some("md".as_ref()) {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        if let Ok(memory) = Self::parse_memory(&content, entry.path()) {
                            memories.push(memory);
                        } else {
                            warn!("Failed to parse memory at {:?}", entry.path());
                        }
                    }
                }
            }
        }

        Ok(memories)
    }

    /// Search memories by title/content (simple text search)
    pub fn search_text(&self, query: &str) -> Result<Vec<(Memory, f32)>> {
        let memories = self.list_memories(None)?;
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for memory in memories {
            let mut score = 0.0;

            // Title match
            if memory.title.to_lowercase().contains(&query_lower) {
                score += 0.5;
            }

            // Content match
            if let Some(ref content) = memory.content {
                let content_lower = content.to_lowercase();
                if content_lower.contains(&query_lower) {
                    score += 0.3;

                    // Multiple occurrences
                    let count = content_lower.matches(&query_lower).count();
                    score += (count as f32 * 0.05).min(0.2);
                }
            }

            // Tag match
            for tag in &memory.tags {
                if tag.to_lowercase().contains(&query_lower) {
                    score += 0.2;
                }
            }

            if score > 0.0 {
                results.push((memory, score.min(1.0)));
            }
        }

        // Sort by score descending
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        Ok(results)
    }

    /// Extract wiki links from content
    pub fn extract_wiki_links(&self, content: &str) -> Vec<String> {
        let re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
        re.captures_iter(content)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect()
    }

    /// Update index.md with current memories
    pub fn update_index(&self, memories: &[Memory]) -> Result<()> {
        let index_path = self.config.data_dir.join(".meta/index.md");

        let mut content = format!(
            "# AI Recall Index\n\nLast Updated: {}\n\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S")
        );

        // Group by type
        let mut by_type: HashMap<String, Vec<&Memory>> = HashMap::new();
        for memory in memories {
            by_type
                .entry(memory.memory_type.as_str().to_string())
                .or_default()
                .push(memory);
        }

        for (type_name, type_memories) in by_type {
            content.push_str(&format!(
                "\n## {} ({} memories)\n\n",
                type_name,
                type_memories.len()
            ));

            for memory in type_memories {
                let full_path = PathBuf::from(&memory.file_path);
                let relative_path = full_path
                    .strip_prefix(&self.config.data_dir)
                    .unwrap_or(full_path.as_path())
                    .to_string_lossy();

                content.push_str(&format!(
                    "- [{}]({}) - {}\n",
                    memory.title,
                    relative_path,
                    memory.updated_at.format("%Y-%m-%d")
                ));
            }
        }

        fs::write(&index_path, content)?;
        debug!("Updated index at {:?}", index_path);
        Ok(())
    }

    /// Sanitize filename for filesystem
    fn sanitize_filename(title: &str) -> String {
        title
            .to_lowercase()
            .replace(" ", "-")
            .replace(|c: char| !c.is_alphanumeric() && c != '-', "")
            .trim_matches('-')
            .to_string()
    }

    /// Get storage statistics
    pub fn get_stats(&self) -> Result<StorageStats> {
        let memories = self.list_memories(None)?;

        let mut by_type: HashMap<String, usize> = HashMap::new();
        for memory in &memories {
            *by_type
                .entry(memory.memory_type.as_str().to_string())
                .or_default() += 1;
        }

        // Calculate total size
        let mut total_size = 0u64;
        for memory in &memories {
            if let Ok(metadata) = fs::metadata(&memory.file_path) {
                total_size += metadata.len();
            }
        }

        Ok(StorageStats {
            total_memories: memories.len(),
            by_type,
            total_size_bytes: total_size,
        })
    }
}

#[derive(Debug)]
pub struct StorageStats {
    pub total_memories: usize,
    pub by_type: HashMap<String, usize>,
    pub total_size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_storage() -> (MarkdownStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            data_dir: temp_dir.path().to_path_buf(),
            max_file_size_mb: 10,
        };
        let storage = MarkdownStorage::new(config);
        storage.initialize().unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(
            MarkdownStorage::sanitize_filename("Rust Ownership Rules"),
            "rust-ownership-rules"
        );
        assert_eq!(
            MarkdownStorage::sanitize_filename("Special!@#Characters"),
            "specialcharacters"
        );
    }

    #[test]
    fn test_extract_wiki_links() {
        let storage = create_test_storage().0;
        let content = "See [[Rust Ownership]] and [[Borrowing Rules]] for more info.";
        let links = storage.extract_wiki_links(content);
        assert_eq!(links, vec!["Rust Ownership", "Borrowing Rules"]);
    }

    #[test]
    fn test_write_and_read_memory() {
        let (storage, _temp) = create_test_storage();

        let memory = Memory {
            id: "test-id".to_string(),
            title: "Test Memory".to_string(),
            memory_type: MemoryType::Semantic,
            content: Some("Test content".to_string()),
            file_path: String::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            tags: vec!["test".to_string()],
            confidence: Some(0.9),
            source_refs: vec![],
            version_hash: "abc123".to_string(),
            related_memories: vec![],
            embedding_model: None,
            embedding_dimension: None,
        };

        let path = storage.write_memory(&memory).unwrap();
        assert!(path.exists());
    }
}
