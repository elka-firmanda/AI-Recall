use std::collections::HashMap;

use anyhow::Result;
use serde_json::json;

// Re-export library items
use ai_recall::{
    analysis::contradictions::{ContradictionConfig, ContradictionType},
    auth::{AuthState, generate_token},
    config::{AppConfig, EmbeddingConfig, QdrantConfig, ServerConfig, StorageConfig},
    graph::WikiLinkExtractor,
    models::{AddMemoryRequest, Memory, MemoryType},
    storage::markdown::MarkdownStorage,
    storage::feedback::{FeedbackStore, FeedbackRating},
    storage::versioning::VersionStore,
};
// Helper module for test utilities
mod common {
    use std::path::PathBuf;
    use tempfile::TempDir;

    use super::{AppConfig, StorageConfig, ServerConfig, QdrantConfig, EmbeddingConfig};

    /// Create a test configuration with temporary directories
    pub fn test_config() -> (AppConfig, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        
        let config = AppConfig {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0, // Let OS assign random port
                auth_token: Some("test_token_123".to_string()),
                log_level: "debug".to_string(),
            },
            storage: StorageConfig {
                data_dir: data_dir.clone(),
                max_file_size_mb: 10,
            },
            qdrant: QdrantConfig {
                url: "http://localhost:6334".to_string(),
                api_key: None,
                collection_name: "test_memories".to_string(),
                graph_collection_name: "test_graph".to_string(),
                vector_size: 1536,
                distance: "Cosine".to_string(),
            },
            embeddings: EmbeddingConfig {
                provider: "mock".to_string(),
                api_key: "test_key".to_string(),
                model: "text-embedding-3-small".to_string(),
                dimension: 1536,
                batch_size: 10,
                timeout_secs: 30,
                base_url: None,
            },
            memory_defaults: Default::default(),
        };
        
        (config, temp_dir)
    }

    pub fn test_data_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data")
    }
}

#[cfg(test)]
mod config_tests {
    use super::common::test_config;
    use super::{AppConfig, StorageConfig, ServerConfig};

    #[test]
    fn test_config_creation() {
        let (config, _temp) = test_config();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 0);
        assert!(config.server.auth_token.is_some());
    }

    #[test]
    fn test_data_dir_paths() {
        let (config, _temp) = test_config();
        let wiki_dir = config.wiki_dir();
        assert!(wiki_dir.to_string_lossy().contains("wiki"));
    }
}

#[cfg(test)]
mod storage_tests {
    use anyhow::Result;
    use chrono::Utc;
    use super::common::test_config;
    use super::{MarkdownStorage, Memory, MemoryType};

    #[test]
    fn test_markdown_storage_initialization() -> Result<()> {
        let (config, _temp) = test_config();
        let storage = MarkdownStorage::new(config.storage.clone());
        storage.initialize()?;
        
        // Check that directories were created
        assert!(config.wiki_dir().exists());
        Ok(())
    }

    #[test]
    fn test_memory_write_and_read() -> anyhow::Result<()> {
        let (config, _temp) = test_config();
        let storage = MarkdownStorage::new(config.storage.clone());
        storage.initialize()?;

        let memory = Memory {
            id: "mem_test_001".to_string(),
            title: "Test Memory".to_string(),
            memory_type: MemoryType::Semantic,
            content: Some("This is test content".to_string()),
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

        let path = storage.write_memory(&memory)?;
        assert!(path.exists(), "Written file should exist at {:?}", path);

        let read_memory = storage.read_memory(&memory.id)?;
        assert!(read_memory.is_some(), "Should be able to read back the memory by ID");
        assert_eq!(read_memory.unwrap().title, "Test Memory");

        Ok(())
    }

    #[test]
    fn test_memory_list() -> anyhow::Result<()> {
        let (config, _temp) = test_config();
        let storage = MarkdownStorage::new(config.storage.clone());
        storage.initialize()?;

        // Create a few test memories
        for i in 0..3 {
            let memory = Memory {
                id: format!("mem_test_{:03}", i),
                title: format!("Test Memory {}", i),
                memory_type: MemoryType::Semantic,
                content: Some(format!("Content {}", i)),
                file_path: String::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                tags: vec![],
                confidence: Some(0.8),
                source_refs: vec![],
                version_hash: format!("hash{}", i),
                related_memories: vec![],
                embedding_model: None,
                embedding_dimension: None,
            };
            storage.write_memory(&memory)?;
        }

        let memories = storage.list_memories(None)?;
        assert_eq!(memories.len(), 3);

        Ok(())
    }
}

#[cfg(test)]
mod graph_tests {
    use super::WikiLinkExtractor;

    #[test]
    fn test_wiki_link_extraction() {
        let extractor = WikiLinkExtractor::new();
        let content = "This references [[Another Page]] and [[Some Other|Display Text]]";
        
        let links = extractor.extract(content);
        assert_eq!(links.len(), 2);
        assert!(links.iter().any(|l| l.target == "Another Page"));
    }

    #[test]
    fn test_wiki_link_count() {
        let extractor = WikiLinkExtractor::new();
        let content = "Links: [[A]], [[B]], [[C]]";
        
        let count = extractor.count_links(content);
        assert_eq!(count, 3);
    }

    #[test]
    fn test_no_wiki_links() {
        let extractor = WikiLinkExtractor::new();
        let content = "Plain text with no wiki links";
        
        let links = extractor.extract(content);
        assert!(links.is_empty());
    }
}

#[cfg(test)]
mod auth_tests {
    use super::{AuthState, generate_token};

    #[test]
    fn test_token_generation() {
        let token1 = generate_token();
        let token2 = generate_token();
        
        assert_ne!(token1, token2);
        assert!(token1.starts_with("arec_"));
        assert!(token1.len() > 40);
    }

    #[test]
    fn test_token_verification() {
        let token = "test_token_123".to_string();
        let state = AuthState::new(token.clone());
        
        assert!(state.verify_token(&token));
        assert!(!state.verify_token("wrong_token"));
        assert!(!state.verify_token(""));
    }
}

#[cfg(test)]
mod models_tests {
    use serde_json::json;
    use super::{MemoryType, Memory, AddMemoryRequest};

    #[test]
    fn test_memory_type_serialization() {
        let semantic = MemoryType::Semantic;
        let json = serde_json::to_string(&semantic).unwrap();
        assert_eq!(json, "\"semantic\"");
    }

    #[test]
    fn test_memory_type_from_str() {
        let parsed: MemoryType = "semantic".parse().unwrap();
        assert_eq!(parsed, MemoryType::Semantic);
        
        let parsed: MemoryType = "profile".parse().unwrap();
        assert_eq!(parsed, MemoryType::Profile);
    }

    #[test]
    fn test_add_memory_request_defaults() {
        let json = json!({
            "title": "Test",
            "content": "Content",
            "type": "semantic"
        });
        
        let request: AddMemoryRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request.confidence, None);
        assert!(request.auto_link);
    }
}

#[cfg(test)]
mod feedback_tests {
    use super::common::test_config;
    use super::{FeedbackStore, FeedbackRating};

    #[test]
    fn test_feedback_rating_score() {
        assert_eq!(FeedbackRating::Useful.score(), 1.0);
        assert_eq!(FeedbackRating::Wrong.score(), -1.0);
        assert!(FeedbackRating::Irrelevant.score() < 0.0);
        assert!(FeedbackRating::Outdated.score() < 0.0);
    }

    #[test]
    fn test_feedback_rating_parsing() {
        let rating: FeedbackRating = "useful".parse().unwrap();
        assert_eq!(rating, FeedbackRating::Useful);
        
        let rating: FeedbackRating = "outdated".parse().unwrap();
        assert_eq!(rating, FeedbackRating::Outdated);
    }

    #[test]
    fn test_feedback_store_operations() -> anyhow::Result<()> {
        let (config, _temp) = test_config();
        let store = FeedbackStore::new(config.data_dir().clone());
        store.initialize()?;

        store.record_feedback("mem_test", FeedbackRating::Useful, None, Some("Great!".to_string()), "test")?;

        let stats = store.get_stats("mem_test")?;
        assert_eq!(stats.total_feedback, 1);
        assert!(stats.relevance_score > 0.0);

        Ok(())
    }
}

#[cfg(test)]
mod versioning_tests {
    use chrono::Utc;
    use super::common::test_config;
    use super::{VersionStore, Memory, MemoryType};

    #[test]
    fn test_version_store_operations() -> anyhow::Result<()> {
        let (config, _temp) = test_config();
        let store = VersionStore::new(config.data_dir().join(".meta").join("versions"));

        let memory = Memory {
            id: "mem_test".to_string(),
            title: "Test".to_string(),
            memory_type: MemoryType::Semantic,
            content: Some("Content".to_string()),
            file_path: "wiki/semantic/test.md".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            tags: vec![],
            confidence: Some(0.8),
            source_refs: vec![],
            version_hash: "hash1".to_string(),
            related_memories: vec![],
            embedding_model: None,
            embedding_dimension: None,
        };

        let hash = store.create_version(&memory, "Initial version", "test")?;
        assert!(!hash.is_empty());

        let current = store.get_current_version(&memory.id)?;
        assert!(current.is_some());

        Ok(())
    }

    #[test]
    fn test_version_hash_calculation() {
        let content1 = b"Hello World";
        let content2 = b"Hello World";
        let content3 = b"Different content";

        let hash1 = ai_recall::storage::versioning::VersionStore::calculate_hash(content1);
        let hash2 = ai_recall::storage::versioning::VersionStore::calculate_hash(content2);
        let hash3 = ai_recall::storage::versioning::VersionStore::calculate_hash(content3);

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
}

#[cfg(test)]
mod contradiction_tests {
    use super::{ContradictionConfig, ContradictionType};

    #[test]
    fn test_contradiction_config_default() {
        let config = ContradictionConfig::default();
        assert!(config.detect_duplicates);
        assert!(config.detect_fact_conflicts);
    }

    #[test]
    fn test_contradiction_type_as_str() {
        assert_eq!(ContradictionType::FactConflict.as_str(), "fact_conflict");
        assert_eq!(ContradictionType::NearDuplicate.as_str(), "near_duplicate");
    }
}

#[cfg(test)]
mod integration_tests {
    //! These tests require external services (Qdrant, OpenAI)
    //! Run with: cargo test --features integration -- --ignored

    #[test]
    #[ignore]
    fn test_full_memory_workflow() {
        // This test would:
        // 1. Initialize storage
        // 2. Create a memory
        // 3. Search for it
        // 4. Update it
        // 5. Delete it
        // Requires: Qdrant running, OpenAI API key
    }

    #[test]
    #[ignore]
    fn test_vector_search() {
        // This test would verify vector search works
        // Requires: Qdrant running, embeddings API
    }

    #[test]
    #[ignore]
    fn test_mcp_stdio_server() {
        // This test would verify MCP stdio transport
        // Requires: running the binary and sending JSON-RPC
    }
}
