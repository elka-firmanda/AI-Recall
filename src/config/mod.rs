use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Server configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    /// HTTP server bind address
    #[serde(default = "default_host")]
    pub host: String,
    /// HTTP server port
    #[serde(default = "default_port")]
    pub port: u16,
    /// Auth token (auto-generated if empty)
    pub auth_token: Option<String>,
    /// Log level
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            auth_token: None,
            log_level: default_log_level(),
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_log_level() -> String {
    "info".to_string()
}

/// Storage configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    /// Data directory path
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// Maximum file size in MB
    #[serde(default = "default_max_file_size")]
    pub max_file_size_mb: usize,
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("./data")
}

fn default_max_file_size() -> usize {
    10
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            max_file_size_mb: default_max_file_size(),
        }
    }
}

/// Qdrant vector database configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QdrantConfig {
    /// Qdrant server URL
    #[serde(default = "default_qdrant_url")]
    pub url: String,
    /// API key (if using Qdrant Cloud)
    pub api_key: Option<String>,
    /// Collection name for memories
    #[serde(default = "default_collection_name")]
    pub collection_name: String,
    /// Collection name for graph edges
    #[serde(default = "default_graph_collection_name")]
    pub graph_collection_name: String,
    /// Vector dimension
    #[serde(default = "default_vector_size")]
    pub vector_size: usize,
    /// Distance metric
    #[serde(default = "default_distance")]
    pub distance: String,
}

fn default_qdrant_url() -> String {
    "http://localhost:6334".to_string()
}

fn default_collection_name() -> String {
    "memories".to_string()
}

fn default_graph_collection_name() -> String {
    "memory_graph".to_string()
}

fn default_vector_size() -> usize {
    1536
}

fn default_distance() -> String {
    "Cosine".to_string()
}

impl Default for QdrantConfig {
    fn default() -> Self {
        Self {
            url: default_qdrant_url(),
            api_key: None,
            collection_name: default_collection_name(),
            graph_collection_name: default_graph_collection_name(),
            vector_size: default_vector_size(),
            distance: default_distance(),
        }
    }
}

/// Embedding API configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingConfig {
    /// Provider name
    #[serde(default = "default_embedding_provider")]
    pub provider: String,
    /// API key
    pub api_key: String,
    /// Model name
    #[serde(default = "default_embedding_model")]
    pub model: String,
    /// Vector dimension
    #[serde(default = "default_vector_size")]
    pub dimension: usize,
    /// Batch size for requests
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Base URL for custom endpoints
    pub base_url: Option<String>,
}

fn default_embedding_provider() -> String {
    "openai".to_string()
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".to_string()
}

fn default_batch_size() -> usize {
    100
}

fn default_timeout() -> u64 {
    30
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: default_embedding_provider(),
            api_key: String::new(),
            model: default_embedding_model(),
            dimension: default_vector_size(),
            batch_size: default_batch_size(),
            timeout_secs: default_timeout(),
            base_url: None,
        }
    }
}

/// Memory defaults configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryDefaultsConfig {
    /// Default confidence score
    #[serde(default = "default_confidence")]
    pub default_confidence: f32,
    /// Minimum confidence threshold
    #[serde(default = "min_confidence")]
    pub min_confidence_threshold: f32,
    /// Auto-link wiki references
    #[serde(default = "default_true")]
    pub auto_link: bool,
    /// Auto-extract wiki links from content
    #[serde(default = "default_true")]
    pub auto_extract_wikilinks: bool,
}

fn default_confidence() -> f32 {
    0.8
}

fn min_confidence() -> f32 {
    0.5
}

fn default_true() -> bool {
    true
}

impl Default for MemoryDefaultsConfig {
    fn default() -> Self {
        Self {
            default_confidence: default_confidence(),
            min_confidence_threshold: min_confidence(),
            auto_link: default_true(),
            auto_extract_wikilinks: default_true(),
        }
    }
}

/// Main application configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub qdrant: QdrantConfig,
    #[serde(default)]
    pub embeddings: EmbeddingConfig,
    #[serde(default)]
    pub memory_defaults: MemoryDefaultsConfig,
}

impl AppConfig {
    /// Load configuration from file and environment
    pub fn load() -> Result<Self> {
        let mut config = Self::default();

        // 1. Try to load from config file
        let config_paths = ["./config.yaml", "./config.yml", "./ai-recall.yaml"];
        for path in &config_paths {
            if std::path::Path::new(path).exists() {
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("Failed to read config file: {}", path))?;
                config = serde_yaml::from_str(&content)
                    .with_context(|| format!("Failed to parse config file: {}", path))?;
                break;
            }
        }

        // 2. Check config directory
        if let Some(config_dir) = dirs::config_dir() {
            let app_config = config_dir.join("ai-recall").join("config.yaml");
            if app_config.exists() {
                let content = std::fs::read_to_string(&app_config)
                    .with_context(|| "Failed to read app config")?;
                config =
                    serde_yaml::from_str(&content).with_context(|| "Failed to parse app config")?;
            }
        }

        // 3. Override with environment variables
        if let Ok(host) = std::env::var("AI_RECALL_SERVER_HOST") {
            config.server.host = host;
        }
        if let Ok(port) = std::env::var("AI_RECALL_SERVER_PORT") {
            if let Ok(port_num) = port.parse() {
                config.server.port = port_num;
            }
        }
        if let Ok(token) = std::env::var("AI_RECALL_SERVER_AUTH_TOKEN") {
            config.server.auth_token = Some(token);
        }
        if let Ok(data_dir) = std::env::var("AI_RECALL_STORAGE_DATA_DIR") {
            config.storage.data_dir = data_dir.into();
        }
        if let Ok(qdrant_url) = std::env::var("AI_RECALL_QDRANT_URL") {
            config.qdrant.url = qdrant_url;
        }
        if let Ok(api_key) = std::env::var("AI_RECALL_EMBEDDINGS_API_KEY") {
            config.embeddings.api_key = api_key;
        }
        if let Ok(provider) = std::env::var("AI_RECALL_EMBEDDINGS_PROVIDER") {
            config.embeddings.provider = provider;
        }
        if let Ok(model) = std::env::var("AI_RECALL_EMBEDDINGS_MODEL") {
            config.embeddings.model = model;
        }

        // 4. Validate
        if config.embeddings.api_key.is_empty() {
            return Err(anyhow::anyhow!(
                "Embedding API key is required. Set AI_RECALL_EMBEDDINGS_API_KEY environment variable or add to config file."
            ));
        }

        Ok(config)
    }

    /// Load from a specific config file
    pub fn from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path))?;
        let config: AppConfig = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path))?;
        Ok(config)
    }

    /// Get full data directory path
    pub fn data_dir(&self) -> &PathBuf {
        &self.storage.data_dir
    }

    /// Get wiki directory path
    pub fn wiki_dir(&self) -> PathBuf {
        self.storage.data_dir.join("wiki")
    }

    /// Get raw sources directory path
    pub fn raw_dir(&self) -> PathBuf {
        self.storage.data_dir.join("raw")
    }

    /// Get meta directory path
    pub fn meta_dir(&self) -> PathBuf {
        self.storage.data_dir.join(".meta")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.qdrant.vector_size, 1536);
    }
}
