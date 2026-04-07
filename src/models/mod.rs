use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Unique identifier for memories
pub type MemoryId = String;

/// Types of memories in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// Facts, decisions, stable knowledge
    Semantic,
    /// User preferences
    Profile,
    /// How-to guides and workflows
    Procedural,
    /// Temporary task context
    Working,
    /// Session summaries
    Episodic,
}

impl MemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryType::Semantic => "semantic",
            MemoryType::Profile => "profile",
            MemoryType::Procedural => "procedural",
            MemoryType::Working => "working",
            MemoryType::Episodic => "episodic",
        }
    }

    pub fn directory(&self) -> PathBuf {
        PathBuf::from("wiki").join(self.as_str())
    }
}

impl std::str::FromStr for MemoryType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "semantic" => Ok(MemoryType::Semantic),
            "profile" => Ok(MemoryType::Profile),
            "procedural" => Ok(MemoryType::Procedural),
            "working" => Ok(MemoryType::Working),
            "episodic" => Ok(MemoryType::Episodic),
            _ => Err(format!("Unknown memory type: {}", s)),
        }
    }
}

/// Core memory entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Unique identifier (UUID v4)
    pub id: MemoryId,
    /// Human-readable title
    pub title: String,
    /// Memory type classification
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    /// Full markdown content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// File path relative to data directory
    pub file_path: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Categorization tags
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Quality metric (0.0 - 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Source document references
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_refs: Vec<String>,
    /// Content hash for versioning
    pub version_hash: String,
    /// Related memory IDs
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_memories: Vec<String>,
    /// Embedding model used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    /// Embedding dimension
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_dimension: Option<usize>,
}

/// Frontmatter for markdown files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFrontmatter {
    pub id: MemoryId,
    pub title: String,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_refs: Vec<String>,
    pub version_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_dimension: Option<usize>,
}

impl From<&Memory> for MemoryFrontmatter {
    fn from(memory: &Memory) -> Self {
        Self {
            id: memory.id.clone(),
            title: memory.title.clone(),
            memory_type: memory.memory_type,
            created_at: memory.created_at,
            updated_at: memory.updated_at,
            tags: memory.tags.clone(),
            confidence: memory.confidence,
            source_refs: memory.source_refs.clone(),
            version_hash: memory.version_hash.clone(),
            embedding_model: memory.embedding_model.clone(),
            embedding_dimension: memory.embedding_dimension,
        }
    }
}

/// Search result with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryResult {
    pub id: MemoryId,
    pub title: String,
    pub memory_type: String,
    /// Relevance score (0.0 - 1.0)
    pub relevance_score: f32,
    /// Content snippet
    pub snippet: ContentSnippet,
    /// Full metadata
    pub metadata: MemoryMetadata,
    /// Related memory IDs
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_memories: Vec<String>,
    /// Number of backlinks
    pub backlink_count: usize,
}

/// Content snippet with context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentSnippet {
    /// Extracted text snippet
    pub text: String,
    /// Character range in original document
    pub char_range: (usize, usize),
    /// Key phrases that matched
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<String>,
    /// Whether content was truncated
    pub is_truncated: bool,
}

/// Metadata for search results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetadata {
    pub file_path: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_refs: Vec<String>,
    pub confidence: f32,
    pub version_hash: String,
}

impl From<&Memory> for MemoryMetadata {
    fn from(memory: &Memory) -> Self {
        Self {
            file_path: memory.file_path.clone(),
            created_at: memory.created_at,
            updated_at: memory.updated_at,
            tags: memory.tags.clone(),
            source_refs: memory.source_refs.clone(),
            confidence: memory.confidence.unwrap_or(0.8),
            version_hash: memory.version_hash.clone(),
        }
    }
}

/// Request to add a new memory
#[derive(Debug, Clone, Deserialize)]
pub struct AddMemoryRequest {
    pub title: String,
    pub content: String,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<String>,
    pub confidence: Option<f32>,
    #[serde(default = "default_auto_link")]
    pub auto_link: bool,
}

fn default_auto_link() -> bool {
    true
}

impl AddMemoryRequest {
    pub fn confidence_or_default(&self) -> f32 {
        self.confidence.unwrap_or(0.8)
    }
}

/// Response from adding a memory
#[derive(Debug, Clone, Serialize)]
pub struct AddMemoryResponse {
    pub id: MemoryId,
    pub file_path: String,
    pub created_at: DateTime<Utc>,
    pub embedded: bool,
    pub links_extracted: usize,
    pub version_hash: String,
}

/// Request to search memories
#[derive(Debug, Clone, Deserialize)]
pub struct SearchMemoryRequest {
    pub query: String,
    #[serde(rename = "type")]
    pub memory_type: Option<MemoryType>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub min_confidence: Option<f32>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub include_related: Option<bool>,
    pub threshold: Option<f32>,
}

fn default_limit() -> usize {
    10
}

/// Response from searching memories
#[derive(Debug, Clone, Serialize)]
pub struct SearchMemoryResponse {
    pub embedding_time_ms: u64,
    pub total_matches: usize,
    pub memories: Vec<MemoryResult>,
}

/// Request to get a memory
#[derive(Debug, Clone, Deserialize)]
pub struct GetMemoryRequest {
    pub id: MemoryId,
    pub version: Option<String>,
    #[serde(default = "default_include_content")]
    pub include_content: bool,
}

fn default_include_content() -> bool {
    true
}

/// Request to update a memory
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateMemoryRequest {
    pub id: MemoryId,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub add_tags: Option<Vec<String>>,
    pub confidence: Option<f32>,
    pub commit_message: Option<String>,
    #[serde(default)]
    pub force_version: bool,
}

/// Request to delete a memory
#[derive(Debug, Clone, Deserialize)]
pub struct DeleteMemoryRequest {
    pub id: MemoryId,
    #[serde(default)]
    pub permanent: bool,
}

/// Request to list memories
#[derive(Debug, Clone, Deserialize)]
pub struct ListMemoryRequest {
    #[serde(rename = "type")]
    pub memory_type: Option<MemoryType>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub since: Option<DateTime<Utc>>,
    #[serde(default = "default_list_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_list_limit() -> usize {
    20
}

/// Response from listing memories
#[derive(Debug, Clone, Serialize)]
pub struct ListMemoryResponse {
    pub data: Vec<MemorySummary>,
    pub pagination: Pagination,
}

/// Summary of a memory (for listing)
#[derive(Debug, Clone, Serialize)]
pub struct MemorySummary {
    pub id: MemoryId,
    pub title: String,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub created_at: DateTime<Utc>,
    pub confidence: f32,
    pub file_path: String,
}

impl From<&Memory> for MemorySummary {
    fn from(memory: &Memory) -> Self {
        Self {
            id: memory.id.clone(),
            title: memory.title.clone(),
            memory_type: memory.memory_type,
            created_at: memory.created_at,
            confidence: memory.confidence.unwrap_or(0.8),
            file_path: memory.file_path.clone(),
        }
    }
}

/// Pagination info
#[derive(Debug, Clone, Serialize)]
pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
    pub total: usize,
}
