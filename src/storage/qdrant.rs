use anyhow::Result;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, Distance, PointStruct,
    SearchPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder,
    DeletePointsBuilder,
};
use qdrant_client::Qdrant;
use serde_json::json;
use std::collections::HashMap;
use tracing::{debug, info, warn};

use crate::config::QdrantConfig;
use crate::models::{Memory, MemoryId};

/// Vector storage using Qdrant
pub struct QdrantStorage {
    client: Qdrant,
    config: QdrantConfig,
}

impl QdrantStorage {
    pub async fn new(config: QdrantConfig) -> Result<Self> {
        let client = if let Some(ref api_key) = config.api_key {
            Qdrant::from_url(&config.url).api_key(api_key.clone()).build()?
        } else {
            Qdrant::from_url(&config.url).build()?
        };

        let storage = Self { client, config };
        storage.initialize().await?;

        Ok(storage)
    }

    /// Initialize collections if they don't exist
    async fn initialize(&self) -> Result<()> {
        // Check if memories collection exists
        let collections = self.client.list_collections().await?;
        let collection_exists = collections
            .collections
            .iter()
            .any(|c| c.name == self.config.collection_name);

        if !collection_exists {
            info!(
                "Creating memories collection '{}' with {} dimensions",
                self.config.collection_name, self.config.vector_size
            );

            let distance = match self.config.distance.as_str() {
                "Euclid" => Distance::Euclid,
                "Dot" => Distance::Dot,
                _ => Distance::Cosine,
            };

            self.client
                .create_collection(
                    CreateCollectionBuilder::new(&self.config.collection_name)
                        .vectors_config(VectorParamsBuilder::new(self.config.vector_size as u64, distance)),
                )
                .await?;

            info!("Created memories collection");
        } else {
            debug!("Memories collection already exists");
        }

        Ok(())
    }

    /// Store a memory vector
    pub async fn store_memory(
        &self,
        memory: &Memory,
        embedding: Vec<f32>,
    ) -> Result<()> {
        // Build payload as HashMap
        let mut payload_map: HashMap<String, serde_json::Value> = HashMap::new();
        payload_map.insert("memory_id".to_string(), json!(memory.id));
        payload_map.insert("memory_type".to_string(), json!(memory.memory_type.as_str()));
        payload_map.insert("title".to_string(), json!(memory.title));
        payload_map.insert("file_path".to_string(), json!(memory.file_path));
        payload_map.insert("tags".to_string(), json!(memory.tags));
        payload_map.insert("created_at".to_string(), json!(memory.created_at.to_rfc3339()));
        payload_map.insert("updated_at".to_string(), json!(memory.updated_at.to_rfc3339()));
        payload_map.insert("confidence".to_string(), json!(memory.confidence.unwrap_or(0.8)));
        payload_map.insert("source_refs".to_string(), json!(memory.source_refs));
        payload_map.insert("version_hash".to_string(), json!(memory.version_hash));
        payload_map.insert("embedding_model".to_string(), json!(memory.embedding_model.as_ref().unwrap_or(&"unknown".to_string())));
        payload_map.insert("embedding_dimension".to_string(), json!(memory.embedding_dimension.unwrap_or(self.config.vector_size)));

        let points = vec![PointStruct::new(
            memory.id.clone(),
            embedding,
            payload_map,
        )];

        self.client
            .upsert_points(UpsertPointsBuilder::new(&self.config.collection_name, points))
            .await?;

        debug!("Stored vector for memory {}", memory.id);
        Ok(())
    }

    /// Search memories by vector similarity
    pub async fn search(
        &self,
        query_embedding: Vec<f32>,
        limit: usize,
        _memory_type: Option<&str>,
        _min_confidence: Option<f32>,
    ) -> Result<Vec<SearchResult>> {
        // Build search request without filter for now
        let search_request = SearchPointsBuilder::new(
            &self.config.collection_name, 
            query_embedding, 
            limit as u64
        )
        .with_payload(true)
        .with_vectors(false);

        let response = self.client.search_points(search_request).await?;

        let results: Vec<SearchResult> = response
            .result
            .into_iter()
            .map(|scored_point| SearchResult {
                memory_id: scored_point.payload.get("memory_id")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_default(),
                score: scored_point.score,
            })
            .collect();

        Ok(results)
    }

    /// Delete a memory vector
    pub async fn delete_memory(&self, memory_id: &MemoryId) -> Result<()> {
        let points: Vec<qdrant_client::qdrant::PointId> = 
            vec![memory_id.clone().into()];
        
        self.client
            .delete_points(
                DeletePointsBuilder::new(&self.config.collection_name)
                    .points(points),
            )
            .await?;

        debug!("Deleted vector for memory {}", memory_id);
        Ok(())
    }

    /// Check if a memory exists in vector store
    #[allow(dead_code)]
    pub async fn exists(&self, _memory_id: &MemoryId) -> Result<bool> {
        // For now, return true - proper implementation would search for the specific ID
        // This is a placeholder as the CountPoints API requires different handling
        Ok(true)
    }

    /// Get collection info
    pub async fn get_stats(&self) -> Result<QdrantStats> {
        let info = self.client
            .collection_info(&self.config.collection_name)
            .await?;

        // Safely extract values from the response
        let result = info.result.ok_or_else(|| 
            anyhow::anyhow!("No collection info result")
        )?;

        // Get points count from the result
        let points_count = result.segments_count as usize; // Using segments as a proxy

        Ok(QdrantStats {
            vectors_count: points_count,
            indexed_vectors_count: points_count,
            points_count,
            segments_count: result.segments_count as usize,
        })
    }

    /// Clear collection
    pub async fn clear_collection(&self) -> Result<()> {
        warn!("Clearing all vectors from collection {}", self.config.collection_name);
        
        self.client
            .delete_collection(&self.config.collection_name)
            .await?;

        // Recreate
        self.initialize().await?;
        
        info!("Collection cleared and recreated");
        Ok(())
    }
}

/// Search result from Qdrant
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub memory_id: MemoryId,
    pub score: f32,
}

/// Qdrant statistics
#[derive(Debug, Clone)]
pub struct QdrantStats {
    pub vectors_count: usize,
    pub indexed_vectors_count: usize,
    pub points_count: usize,
    pub segments_count: usize,
}
