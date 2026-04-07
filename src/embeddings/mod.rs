use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::config::EmbeddingConfig;

/// Client for embedding API (OpenAI or OpenRouter)
pub struct EmbeddingClient {
    http: Client,
    config: EmbeddingConfig,
    base_url: String,
}

/// Request to embedding API
#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

/// Response from embedding API
#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    model: String,
    usage: Usage,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: u32,
    total_tokens: u32,
}

/// Error response from API
#[derive(Debug, Deserialize)]
struct ApiError {
    error: ApiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct ApiErrorDetail {
    message: String,
    #[serde(rename = "type")]
    error_type: String,
}

impl EmbeddingClient {
    pub fn new(config: EmbeddingConfig) -> Result<Self> {
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| match config.provider.as_str() {
                "openai" => "https://api.openai.com/v1".to_string(),
                "openrouter" => "https://openrouter.ai/api/v1".to_string(),
                _ => "https://api.openai.com/v1".to_string(),
            });

        let http = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .context("Failed to create HTTP client")?;

        info!(
            "Initialized embedding client for provider '{}' with model '{}'",
            config.provider, config.model
        );

        Ok(Self {
            http,
            config,
            base_url,
        })
    }

    /// Generate embeddings for a single text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let results = self.embed_batch(vec![text.to_string()]).await?;
        Ok(results.into_iter().next().context("No embedding returned")?)
    }

    /// Generate embeddings for multiple texts (batched)
    pub async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        debug!("Generating embeddings for {} texts", texts.len());

        let url = format!("{}/embeddings", self.base_url);
        
        let request = EmbeddingRequest {
            model: self.config.model.clone(),
            input: texts,
        };

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send embedding request")?;

        let status = response.status();
        
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            
            // Try to parse as API error
            if let Ok(api_error) = serde_json::from_str::<ApiError>(&error_text) {
                anyhow::bail!(
                    "Embedding API error ({}): {} - {}",
                    status,
                    api_error.error.error_type,
                    api_error.error.message
                );
            }
            
            anyhow::bail!(
                "Embedding API request failed ({}): {}",
                status,
                error_text
            );
        }

        let embedding_response: EmbeddingResponse = response
            .json()
            .await
            .context("Failed to parse embedding response")?;

        // Sort by index to maintain order
        let mut embeddings: Vec<(usize, Vec<f32>)> = embedding_response
            .data
            .into_iter()
            .map(|d| (d.index, d.embedding))
            .collect();
        
        embeddings.sort_by_key(|(idx, _)| *idx);

        let result: Vec<Vec<f32>> = embeddings.into_iter().map(|(_, emb)| emb).collect();

        debug!(
            "Generated {} embeddings using {} tokens",
            result.len(),
            embedding_response.usage.total_tokens
        );

        // Validate dimension
        if let Some(first) = result.first() {
            let actual_dim = first.len();
            if actual_dim != self.config.dimension {
                warn!(
                    "Embedding dimension mismatch: expected {}, got {}",
                    self.config.dimension, actual_dim
                );
            }
        }

        Ok(result)
    }

    /// Get provider name
    pub fn provider(&self) -> &str {
        &self.config.provider
    }

    /// Get model name
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Get dimension
    pub fn dimension(&self) -> usize {
        self.config.dimension
    }

    /// Test connection to embedding API
    pub async fn health_check(&self) -> Result<()> {
        let test_text = "test";
        let _ = self.embed(test_text).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> EmbeddingConfig {
        EmbeddingConfig {
            provider: "openai".to_string(),
            api_key: std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "test-key".to_string()),
            model: "text-embedding-3-small".to_string(),
            dimension: 1536,
            batch_size: 100,
            timeout_secs: 30,
            base_url: None,
        }
    }

    #[test]
    fn test_client_creation() {
        let config = create_test_config();
        let client = EmbeddingClient::new(config);
        assert!(client.is_ok());
    }

    // Note: embed and embed_batch tests require valid API key
    // These are integration tests and should be run with caution
}
