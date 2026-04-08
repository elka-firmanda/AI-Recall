//! AI Recall - Self-hosted AI agent memory system with vector search
//!
//! This library provides core functionality for managing AI agent memories
//! with vector search, markdown storage, and graph relationships.

pub mod analysis;
pub mod auth;
pub mod config;
pub mod embeddings;
pub mod extractors;
pub mod graph;
pub mod mcp;
pub mod models;
pub mod storage;
pub mod upload;

// Re-export commonly used types
pub use config::AppConfig;
pub use models::{Memory, MemoryType, MemoryId};
