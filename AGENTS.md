# AI Recall - Agent Guidelines

> This file contains essential information for AI coding agents working on the AI Recall project.

## Project Overview

AI Recall is a self-hosted AI agent memory system with vector search, built in Rust. It uses Markdown files for storage, Qdrant for vector search, and implements the Model Context Protocol (MCP).

## Build Commands

```bash
# Check compilation without building
cargo check

# Build debug version
cargo build

# Build release version
cargo build --release

# Run the application
cargo run -- <command>

# Run all tests
cargo test

# Run a specific test by name pattern
cargo test test_name_pattern

# Run tests in a specific module
cargo test config::tests

# Run with output visible
cargo test -- --nocapture

# Lint with Clippy
cargo clippy

# Fix auto-fixable issues
cargo clippy --fix

# Format code
cargo fmt

# Check formatting without modifying
cargo fmt -- --check
```

## Code Style Guidelines

### Imports Order
1. Standard library (`std::`)
2. Third-party crates (alphabetical by crate name)
3. Internal modules (`crate::`)
4. Use `use anyhow::{Context, Result};` for error handling

Example:
```rust
use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::config::AppConfig;
use crate::models::Memory;
```

### Naming Conventions
- **Types**: PascalCase (`MemoryType`, `AppConfig`)
- **Functions/Methods**: snake_case (`memory_add`, `calculate_hash`)
- **Variables**: snake_case (`memory_id`, `query_embedding`)
- **Constants**: SCREAMING_SNAKE_CASE (`DEFAULT_LIMIT`)
- **Type aliases**: PascalCase with `_` for clarity (`MemoryId` = `String`)
- **File names**: snake_case (`markdown.rs`, `versioning.rs`)

### Error Handling
- Use `anyhow::Result` for functions that can fail
- Use `.with_context()` for adding context to errors
- Use `thiserror` for custom error types (if needed)
- Prefer `?` operator over `match` for error propagation
- Log errors with `tracing::error!` before returning

Example:
```rust
use anyhow::{Context, Result};

pub fn load_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config: {}", path))?;
    let config = serde_yaml::from_str(&content)
        .with_context(|| "Failed to parse config")?;
    Ok(config)
}
```

### Types & Structs
- Derive common traits: `Debug, Clone` (always), `Serialize, Deserialize` (for data)
- Use `#[serde(rename_all = "snake_case")]` for enums
- Use `#[serde(skip_serializing_if = "Option::is_none")]` for optional fields
- Use `#[serde(default)]` for collections
- Use type aliases for string IDs: `pub type MemoryId = String;`

### Async & Concurrency
- Use `tokio` for async runtime
- Use `Arc<>` for shared state in async contexts
- Instrument async functions with `#[instrument(skip(self, ...))]`
- Use `tracing` for logging: `debug!`, `info!`, `warn!`, `error!`

### Module Structure
Each module should follow this pattern:
```rust
// 1. Imports
use anyhow::Result;
// ...

// 2. Public types/structs
pub struct MyStruct { }

// 3. Implementation
impl MyStruct {
    pub fn new() -> Result<Self> { }
}

// 4. Re-exports at module level (in mod.rs)
pub mod submodule;
pub use submodule::{Type1, Type2};

// 5. Tests at end of file
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_something() {
        assert_eq!(1 + 1, 2);
    }
}
```

## Project Structure

```
src/
├── main.rs           # CLI entry point
├── config/           # Configuration management
├── models/           # Data structures (Memory, Request/Response types)
├── storage/          # Storage backends
│   ├── markdown.rs   # Markdown file storage
│   ├── qdrant.rs     # Vector database
│   ├── versioning.rs # Git-style versioning
│   └── feedback.rs   # Memory feedback system
├── embeddings/       # Embedding API client
├── graph/            # Graph relationship storage
│   └── storage.rs    # Petgraph-based graph
├── mcp/              # MCP server & handlers
│   ├── mod.rs        # HTTP/stdio transports
│   └── handler.rs    # Memory operations handler
├── analysis/         # Analysis tools
│   └── contradictions.rs  # Contradiction detection
└── auth/             # Authentication middleware
```

## Key Design Patterns

### Memory Types (Karpathy's Taxonomy + Extensions)
- `semantic` - Facts, decisions, stable knowledge
- `profile` - User preferences
- `procedural` - How-to guides and workflows
- `working` - Temporary task context
- `episodic` - Session summaries

### Error Handling Philosophy
- Use `anyhow` for application-level errors
- Add context at every layer
- Log before returning errors in async functions
- Use `warn!` for recoverable issues, `error!` for failures

### Tracing/Logging
- Use `#[instrument]` on async handler methods
- Skip large fields like `self`, `request` to avoid noise
- Use structured logging: `info!(memory_id = %id, "Message")`

## Testing

- Unit tests go in `#[cfg(test)]` module at end of file
- Use `tokio::test` for async tests
- Use `tempfile` crate for temporary directories in tests
- Mock external services (embeddings, Qdrant) using `mockall`

## Environment Variables

Key env vars used:
- `AI_RECALL_EMBEDDINGS_API_KEY` - Required for embeddings
- `AI_RECALL_QDRANT_URL` - Qdrant server URL
- `AI_RECALL_SERVER_AUTH_TOKEN` - HTTP auth token
- `AI_RECALL_STORAGE_DATA_DIR` - Data directory path

## Dependencies Notes

- `anyhow` / `thiserror` - Error handling
- `tokio` - Async runtime
- `axum` - HTTP server
- `serde` / `serde_yaml` - Serialization
- `tracing` - Logging
- `qdrant-client` - Vector database
- `petgraph` - Graph data structures
- `pulldown-cmark` - Markdown parsing
- `rmcp` - MCP protocol (limited use)
- `sha2` / `base64` / `rand` - Crypto utilities
- `chrono` - Date/time handling
- `uuid` - ID generation
- `clap` - CLI parsing
- `regex` - Pattern matching
- `walkdir` - Directory traversal

## Common Tasks

### Adding a new memory type
1. Add variant to `MemoryType` enum in `models/mod.rs`
2. Update `as_str()` and `from_str()` implementations
3. Add directory variant in `directory()` method
4. Update any type-match statements

### Adding a new API endpoint
1. Add request/response types to `models/mod.rs`
2. Implement handler method in `mcp/handler.rs`
3. Add route in `mcp/mod.rs` (HTTP) or tool in `tools/list`
4. Add CLI command in `main.rs` if needed

### Adding a new storage backend
1. Create module in `storage/`
2. Implement trait (if abstracting) or struct with methods
3. Re-export from `storage/mod.rs`
4. Initialize in `MemoryMcpHandler::new()`

## Things to Avoid

- Don't use `std::sync::Mutex` in async code (use `tokio::sync::Mutex` or `parking_lot`)
- Don't block the async runtime with synchronous IO
- Don't use `unwrap()` or `expect()` in production code (use `?` or proper error handling)
- Don't forget to add `skip_serializing_if` for optional fields
- Don't use `println!` for logging (use `tracing` macros)
