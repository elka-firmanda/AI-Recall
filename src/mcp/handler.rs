use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, error, info, instrument};

use crate::config::AppConfig;
use crate::embeddings::EmbeddingClient;
use crate::graph::WikiLinkExtractor;
use crate::models::*;
use crate::storage::markdown::MarkdownStorage;
use crate::storage::qdrant::QdrantStorage;

/// Memory service for handling memory operations
#[derive(Clone)]
pub struct MemoryMcpHandler {
    config: Arc<AppConfig>,
    markdown: Arc<MarkdownStorage>,
    qdrant: Arc<QdrantStorage>,
    embeddings: Arc<EmbeddingClient>,
    link_extractor: Arc<WikiLinkExtractor>,
}

impl MemoryMcpHandler {
    pub async fn new(config: AppConfig) -> Result<Self> {
        let markdown = Arc::new(MarkdownStorage::new(config.storage.clone()));
        markdown.initialize()?;

        let qdrant = Arc::new(QdrantStorage::new(config.qdrant.clone()).await?);
        let embeddings = Arc::new(EmbeddingClient::new(config.embeddings.clone())?);
        let link_extractor = Arc::new(WikiLinkExtractor::new());

        info!("Memory MCP Handler initialized");

        Ok(Self {
            config: Arc::new(config),
            markdown,
            qdrant,
            embeddings,
            link_extractor,
        })
    }

    /// Generate a unique memory ID
    fn generate_id() -> String {
        format!("mem_{}", uuid::Uuid::new_v4())
    }

    /// Calculate content hash for versioning
    fn calculate_hash(content: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())[..16].to_string()
    }

    /// Add a new memory
    #[instrument(skip(self, request))]
    pub async fn memory_add(&self, request: AddMemoryRequest) -> Result<AddMemoryResponse> {
        debug!("Adding memory: {}", request.title);

        let id = Self::generate_id();
        let now = chrono::Utc::now();
        let version_hash = Self::calculate_hash(&request.content);

        // Generate embedding
        let embedding_start = std::time::Instant::now();
        let embedding = match self.embeddings.embed(&request.content).await {
            Ok(emb) => {
                debug!("Generated embedding in {:?}", embedding_start.elapsed());
                Some(emb)
            }
            Err(e) => {
                error!("Failed to generate embedding: {}", e);
                None
            }
        };

        // Extract wiki links
        let links = self.link_extractor.extract(&request.content);
        let links_extracted = links.len();

        // Create memory
        let memory = Memory {
            id: id.clone(),
            title: request.title.clone(),
            memory_type: request.memory_type,
            content: Some(request.content.clone()),
            file_path: String::new(), // Will be set by markdown storage
            created_at: now,
            updated_at: now,
            tags: request.tags.clone(),
            confidence: Some(request.confidence_or_default()),
            source_refs: request.source_refs.clone(),
            version_hash: version_hash.clone(),
            related_memories: vec![], // Will be populated from links
            embedding_model: embedding.as_ref().map(|_| self.embeddings.model().to_string()),
            embedding_dimension: embedding.as_ref().map(|e| e.len()),
        };

        // Store markdown
        let file_path = self.markdown.write_memory(&memory)?;

        // Store vector (if embedding succeeded)
        let embedded = if let Some(ref emb) = embedding {
            match self.qdrant.store_memory(&memory, emb.clone()).await {
                Ok(_) => {
                    debug!("Stored vector for memory {}", id);
                    true
                }
                Err(e) => {
                    warn!("Failed to store vector: {}", e);
                    false
                }
            }
        } else {
            false
        };

        let relative_path = file_path
            .strip_prefix(&self.config.data_dir())
            .unwrap_or(&file_path)
            .to_string_lossy()
            .to_string();

        let response = AddMemoryResponse {
            id: id.clone(),
            file_path: relative_path,
            created_at: now,
            embedded,
            links_extracted,
            version_hash,
        };

        info!("Added memory {} with {} links", id, links_extracted);

        Ok(response)
    }

    /// Get a memory by ID
    #[instrument(skip(self, request))]
    pub async fn memory_get(&self, request: GetMemoryRequest) -> Result<Option<Memory>> {
        debug!("Getting memory: {}", request.id);
        self.markdown.read_memory(&request.id)
    }

    /// Search memories
    #[instrument(skip(self, request))]
    pub async fn memory_search(&self, request: SearchMemoryRequest) -> Result<SearchMemoryResponse> {
        debug!("Searching memories: {}", request.query);

        let start = std::time::Instant::now();

        // Generate query embedding
        let query_embedding = match self.embeddings.embed(&request.query).await {
            Ok(emb) => emb,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to generate embedding: {}", e));
            }
        };

        let embedding_time_ms = start.elapsed().as_millis() as u64;

        // Search in Qdrant
        let search_results = match self
            .qdrant
            .search(
                query_embedding,
                request.limit,
                request.memory_type.as_ref().map(|t| t.as_str()),
                request.min_confidence,
            )
            .await
        {
            Ok(results) => results,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to search: {}", e));
            }
        };

        // Enrich with full content and build results
        let mut memories = Vec::new();
        
        for search_result in search_results {
            if let Ok(Some(memory)) = self.markdown.read_memory(&search_result.memory_id) {
                let snippet = memory.content.as_ref().map(|c| {
                    Self::extract_snippet(c, &request.query, 300)
                }).unwrap_or(ContentSnippet {
                    text: String::new(),
                    char_range: (0, 0),
                    highlights: vec![],
                    is_truncated: false,
                });

                let backlink_count = self.link_extractor.count_links(
                    memory.content.as_deref().unwrap_or("")
                );

                memories.push(MemoryResult {
                    id: memory.id.clone(),
                    title: memory.title.clone(),
                    memory_type: memory.memory_type.as_str().to_string(),
                    relevance_score: search_result.score,
                    snippet,
                    metadata: MemoryMetadata::from(&memory),
                    related_memories: memory.related_memories.clone(),
                    backlink_count,
                });
            }
        }

        let response = SearchMemoryResponse {
            embedding_time_ms,
            total_matches: memories.len(),
            memories,
        };

        debug!("Search completed in {}ms", embedding_time_ms);

        Ok(response)
    }

    /// List memories
    #[instrument(skip(self, request))]
    pub async fn memory_list(&self, request: ListMemoryRequest) -> Result<ListMemoryResponse> {
        debug!("Listing memories");

        let all_memories = self.markdown.list_memories(request.memory_type)?;

        // Apply filters
        let filtered: Vec<_> = all_memories
            .into_iter()
            .filter(|m| {
                // Filter by tags
                if !request.tags.is_empty() {
                    let has_tag = request.tags.iter().any(|tag| m.tags.contains(tag));
                    if !has_tag {
                        return false;
                    }
                }

                // Filter by date
                if let Some(since) = request.since {
                    if m.created_at < since {
                        return false;
                    }
                }

                true
            })
            .collect();

        let total = filtered.len();
        let offset = request.offset.min(total);
        let limit = request.limit.min(total - offset);
        
        let page: Vec<MemorySummary> = filtered
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|m| MemorySummary::from(&m))
            .collect();

        let response = ListMemoryResponse {
            data: page,
            pagination: Pagination {
                limit: request.limit,
                offset: request.offset,
                total,
            },
        };

        Ok(response)
    }

    /// Delete a memory
    #[instrument(skip(self, request))]
    pub async fn memory_delete(&self, request: DeleteMemoryRequest) -> Result<bool> {
        debug!("Deleting memory: {}", request.id);

        // Delete from markdown
        match self.markdown.delete_memory(&request.id, request.permanent) {
            Ok(true) => {
                // Also delete from Qdrant
                if let Err(e) = self.qdrant.delete_memory(&request.id).await {
                    warn!("Failed to delete vector: {}", e);
                }

                info!("Deleted memory {}", request.id);
                Ok(true)
            }
            Ok(false) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Update a memory
    #[instrument(skip(self, request))]
    pub async fn memory_update(&self, request: UpdateMemoryRequest) -> Result<serde_json::Value> {
        debug!("Updating memory: {}", request.id);

        // Get existing memory
        let mut memory = match self.markdown.read_memory(&request.id)? {
            Some(m) => m,
            None => {
                return Err(anyhow::anyhow!("Memory not found: {}", request.id));
            }
        };

        let now = chrono::Utc::now();
        let mut content_changed = false;

        // Update content if provided
        if let Some(new_content) = request.content {
            memory.content = Some(new_content.clone());
            memory.version_hash = Self::calculate_hash(&new_content);
            content_changed = true;
        }

        // Update tags
        if let Some(tags) = request.tags {
            memory.tags = tags;
        }

        if let Some(add_tags) = request.add_tags {
            for tag in add_tags {
                if !memory.tags.contains(&tag) {
                    memory.tags.push(tag);
                }
            }
        }

        // Update confidence
        if let Some(confidence) = request.confidence {
            memory.confidence = Some(confidence);
        }

        memory.updated_at = now;

        // Re-generate embedding if content changed
        if content_changed {
            if let Some(ref content) = memory.content {
                match self.embeddings.embed(content).await {
                    Ok(embedding) => {
                        // Update vector in Qdrant
                        if let Err(e) = self
                            .qdrant
                            .store_memory(&memory, embedding.clone())
                            .await
                        {
                            warn!("Failed to update vector: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to regenerate embedding: {}", e);
                    }
                }
            }
        }

        // Write updated memory
        self.markdown.write_memory(&memory)?;

        let response = serde_json::json!({
            "id": memory.id,
            "updated_at": now,
            "content_changed": content_changed,
            "new_version_hash": memory.version_hash,
        });

        info!("Updated memory {}", request.id);

        Ok(response)
    }

    /// Get system status
    pub async fn system_status(&self) -> Result<serde_json::Value> {
        let markdown_stats = self.markdown.get_stats()?;
        let qdrant_stats = self.qdrant.get_stats().await?;

        let response = serde_json::json!({
            "status": "healthy",
            "memories": {
                "total": markdown_stats.total_memories,
                "by_type": markdown_stats.by_type,
                "size_bytes": markdown_stats.total_size_bytes,
            },
            "vectors": {
                "count": qdrant_stats.vectors_count,
                "indexed": qdrant_stats.indexed_vectors_count,
            },
            "version": env!("CARGO_PKG_VERSION"),
        });

        Ok(response)
    }

    /// Extract snippet around matching content
    fn extract_snippet(content: &str, query: &str, max_length: usize) -> ContentSnippet {
        let query_lower = query.to_lowercase();
        let content_lower = content.to_lowercase();
        
        if let Some(pos) = content_lower.find(&query_lower) {
            let start = pos.saturating_sub(max_length / 4);
            let end = (pos + query.len() + max_length / 4).min(content.len());
            
            let snippet = &content[start..end];
            let is_truncated = start > 0 || end < content.len();
            
            ContentSnippet {
                text: snippet.to_string(),
                char_range: (start, end),
                highlights: vec![query.to_string()],
                is_truncated,
            }
        } else {
            // Return beginning of content
            let end = content.len().min(max_length);
            ContentSnippet {
                text: content[..end].to_string(),
                char_range: (0, end),
                highlights: vec![],
                is_truncated: content.len() > max_length,
            }
        }
    }

    /// Record feedback for a memory
    #[instrument(skip(self, request))]
    pub async fn record_feedback(&self, request: serde_json::Value) -> Result<serde_json::Value> {
        use crate::storage::feedback::{FeedbackStore, FeedbackRating};
        
        let memory_id = request.get("memory_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("memory_id is required"))?;
        
        let rating_str = request.get("rating")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("rating is required"))?;
        
        let rating: FeedbackRating = rating_str.parse()
            .map_err(|e: String| anyhow::anyhow!(e))?;
        
        let comment = request.get("comment")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        let feedback_store = FeedbackStore::new(self.config.data_dir().clone());
        
        feedback_store.record_feedback(memory_id, rating, None, comment, "user")?;
        
        info!("Recorded feedback for memory {}: {:?}", memory_id, rating);
        
        Ok(serde_json::json!({
            "memory_id": memory_id,
            "rating": rating.as_str(),
            "score": rating.score(),
            "recorded_at": chrono::Utc::now(),
        }))
    }

    /// Get feedback statistics for a memory
    #[instrument(skip(self))]
    pub async fn get_feedback_stats(&self, memory_id: &str) -> Result<serde_json::Value> {
        use crate::storage::feedback::FeedbackStore;
        
        let feedback_store = FeedbackStore::new(self.config.data_dir().clone());
        
        let stats = feedback_store.get_stats(memory_id)?;
        let feedback_entries = feedback_store.get_feedback(memory_id)?;
        
        Ok(serde_json::json!({
            "memory_id": memory_id,
            "total_feedback": stats.total_feedback,
            "relevance_score": stats.relevance_score,
            "useful_count": stats.useful_count,
            "irrelevant_count": stats.irrelevant_count,
            "outdated_count": stats.outdated_count,
            "wrong_count": stats.wrong_count,
            "recent_feedback": feedback_entries.iter().rev().take(5).map(|e| {
                serde_json::json!({
                    "rating": e.rating.as_str(),
                    "query": e.query,
                    "context": e.context,
                    "source": e.source,
                    "timestamp": e.timestamp,
                })
            }).collect::<Vec<_>>(),
        }))
    }

    /// Check for contradictions in memories
    #[instrument(skip(self))]
    pub async fn check_contradictions(&self, memory_id: Option<String>) -> Result<serde_json::Value> {
        use crate::analysis::contradictions::{ContradictionDetector, ContradictionConfig};
        use crate::storage::markdown::MarkdownStorage;
        
        // Create a new storage instance for the detector
        let storage = MarkdownStorage::new((*self.config).storage.clone());
        let config = ContradictionConfig::default();
        let detector = ContradictionDetector::new(config, storage);
        
        let contradictions = if let Some(ref id) = memory_id {
            detector.check_memory(id)?
        } else {
            detector.check_all()?
        };
        
        let report = detector.generate_report(&contradictions);
        
        info!("Found {} contradictions", contradictions.len());
        
        Ok(serde_json::json!({
            "contradictions_found": contradictions.len(),
            "checked_memory_id": memory_id,
            "contradictions": contradictions.iter().map(|c| {
                serde_json::json!({
                    "memory_a_id": c.memory_a_id,
                    "memory_b_id": c.memory_b_id,
                    "type": c.contradiction_type.as_str(),
                    "confidence": c.confidence,
                    "explanation": c.explanation,
                    "suggestion": c.suggestion,
                })
            }).collect::<Vec<_>>(),
            "report": report,
        }))
    }
}

use tracing::warn;

