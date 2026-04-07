pub mod handler;

use anyhow::Result;
use tracing::{info, warn};
use std::sync::Arc;

use crate::config::AppConfig;
use crate::mcp::handler::MemoryMcpHandler;

/// Start MCP server with stdio transport
/// This implements the Model Context Protocol over stdin/stdout
pub async fn start_stdio_server(config: AppConfig) -> Result<()> {
    info!("Starting MCP stdio server");

    let handler = Arc::new(MemoryMcpHandler::new(config).await?);
    
    info!("MCP stdio handler initialized");
    info!("Waiting for JSON-RPC requests on stdin...");

    // Simple stdio JSON-RPC loop
    // Note: Full rmcp integration would use their transport layer
    // This is a working implementation that reads JSON-RPC requests
    
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let reader = tokio::io::BufReader::new(stdin);
    let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
    
    use tokio::io::AsyncWriteExt;
    
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        
        // Parse JSON-RPC request
        match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(request) => {
                // Handle the request
                let response = handle_jsonrpc_request(&handler, request).await;
                
                // Send response
                let response_str = serde_json::to_string(&response)?;
                stdout.write_all(response_str.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
            Err(e) => {
                warn!("Failed to parse JSON-RPC request: {}", e);
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": "Parse error",
                    }
                });
                stdout.write_all(error_response.to_string().as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
        }
    }
    
    Ok(())
}

/// Handle a JSON-RPC request and return a response
async fn handle_jsonrpc_request(
    handler: &MemoryMcpHandler,
    request: serde_json::Value,
) -> serde_json::Value {
    let id = request.get("id").cloned().unwrap_or(serde_json::Value::Null);
    let method = request.get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("");
    let params = request.get("params").cloned().unwrap_or(serde_json::json!({}));
    
    info!("Handling method: {}", method);
    
    match method {
        "initialize" => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "serverInfo": {
                        "name": "ai-recall",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "capabilities": {
                        "tools": {},
                    }
                }
            })
        }
        
        "tools/list" => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": [
                        {
                            "name": "memory_add",
                            "description": "Add a new memory to the knowledge base",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "title": {"type": "string"},
                                    "content": {"type": "string"},
                                    "type": {"type": "string", "enum": ["semantic", "profile", "procedural", "working", "episodic"]},
                                    "tags": {"type": "array", "items": {"type": "string"}},
                                    "confidence": {"type": "number"},
                                },
                                "required": ["title", "content", "type"]
                            }
                        },
                        {
                            "name": "memory_search",
                            "description": "Search memories by semantic similarity",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "query": {"type": "string"},
                                    "limit": {"type": "number"},
                                },
                                "required": ["query"]
                            }
                        },
                        {
                            "name": "memory_get",
                            "description": "Get a memory by ID",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "id": {"type": "string"},
                                },
                                "required": ["id"]
                            }
                        },
                        {
                            "name": "memory_list",
                            "description": "List memories with optional filtering",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "type": {"type": "string"},
                                    "limit": {"type": "number"},
                                }
                            }
                        },
                        {
                            "name": "system_status",
                            "description": "Get system status and health",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "feedback_record",
                            "description": "Record feedback for a memory",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "memory_id": {"type": "string"},
                                    "rating": {"type": "string", "enum": ["useful", "irrelevant", "outdated", "wrong"]},
                                    "comment": {"type": "string"},
                                },
                                "required": ["memory_id", "rating"]
                            }
                        },
                        {
                            "name": "feedback_stats",
                            "description": "Get feedback statistics for a memory",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "memory_id": {"type": "string"},
                                },
                                "required": ["memory_id"]
                            }
                        },
                        {
                            "name": "contradictions_check",
                            "description": "Check for contradictions in memories",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "memory_id": {"type": "string"},
                                }
                            }
                        },
                    ]
                }
            })
        }
        
        "tools/call" => {
            // Extract tool name and arguments
            let tool_name = params.get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));
            
            // Call the appropriate handler method
            match call_tool(handler, tool_name, arguments).await {
                Ok(result) => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{"type": "text", "text": result}]
                    }
                }),
                Err(e) => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32000,
                        "message": e.to_string(),
                    }
                })
            }
        }
        
        _ => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {}", method),
                }
            })
        }
    }
}

/// Call a specific tool handler
async fn call_tool(
    handler: &MemoryMcpHandler,
    tool_name: &str,
    arguments: serde_json::Value,
) -> Result<String> {
    use crate::models::*;
    use serde_json::from_value;
    
    match tool_name {
        "memory_add" => {
            let request: AddMemoryRequest = from_value(arguments)?;
            let result = handler.memory_add(request).await?;
            Ok(serde_json::to_string_pretty(&result)?)
        }
        "memory_search" => {
            let request: SearchMemoryRequest = from_value(arguments)?;
            let result = handler.memory_search(request).await?;
            Ok(serde_json::to_string_pretty(&result)?)
        }
        "memory_get" => {
            let request: GetMemoryRequest = from_value(arguments)?;
            let result = handler.memory_get(request).await?;
            match result {
                Some(memory) => Ok(serde_json::to_string_pretty(&memory)?),
                None => Ok("Memory not found".to_string()),
            }
        }
        "memory_list" => {
            let request: ListMemoryRequest = from_value(arguments)?;
            let result = handler.memory_list(request).await?;
            Ok(serde_json::to_string_pretty(&result)?)
        }
        "system_status" => {
            let result = handler.system_status().await?;
            Ok(serde_json::to_string_pretty(&result)?)
        }
        "feedback_record" => {
            let result = handler.record_feedback(arguments).await?;
            Ok(serde_json::to_string_pretty(&result)?)
        }
        "feedback_stats" => {
            let memory_id = arguments.get("memory_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let result = handler.get_feedback_stats(memory_id).await?;
            Ok(serde_json::to_string_pretty(&result)?)
        }
        "contradictions_check" => {
            let memory_id = arguments.get("memory_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let result = handler.check_contradictions(memory_id).await?;
            Ok(serde_json::to_string_pretty(&result)?)
        }
        _ => Err(anyhow::anyhow!("Unknown tool: {}", tool_name))
    }
}

/// Start HTTP server with MCP endpoint
pub async fn start_http_server(config: AppConfig) -> Result<()> {
    info!("Starting MCP HTTP server on {}:{}", config.server.host, config.server.port);

    let addr: std::net::SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()?;

    let handler = Arc::new(MemoryMcpHandler::new(config.clone()).await?);

    // Build HTTP routes with optional authentication
    let app = if let Some(token) = &config.server.auth_token {
        info!("Authentication enabled");
        let auth_state = crate::auth::AuthState::new(token.clone());
        
        axum::Router::new()
            .route("/health", axum::routing::get(health_check))
            .route("/mcp", axum::routing::post(handle_mcp_request))
            .route("/feedback", axum::routing::post(handle_feedback_request))
            .route("/contradictions", axum::routing::get(handle_contradictions_request))
            .layer(axum::extract::Extension(handler))
            .layer(axum::middleware::from_fn_with_state(
                auth_state.clone(),
                crate::auth::auth_middleware,
            ))
            .with_state(auth_state)
    } else {
        warn!("No auth token configured - running without authentication!");
        axum::Router::new()
            .route("/health", axum::routing::get(health_check))
            .route("/mcp", axum::routing::post(handle_mcp_request))
            .route("/feedback", axum::routing::post(handle_feedback_request))
            .route("/contradictions", axum::routing::get(handle_contradictions_request))
            .layer(axum::extract::Extension(handler))
    };

    // Start HTTP server
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    info!("HTTP server listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Health check endpoint
async fn health_check() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "healthy",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// MCP request handler for HTTP
async fn handle_mcp_request(
    axum::extract::Extension(handler): axum::extract::Extension<Arc<MemoryMcpHandler>>,
    axum::extract::Json(request): axum::extract::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    let response = handle_jsonrpc_request(&handler, request).await;
    axum::Json(response)
}

/// Feedback request handler for HTTP
async fn handle_feedback_request(
    axum::extract::Extension(handler): axum::extract::Extension<Arc<MemoryMcpHandler>>,
    axum::extract::Json(request): axum::extract::Json<serde_json::Value>,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    match handler.record_feedback(request).await {
        Ok(result) => Ok(axum::Json(result)),
        Err(e) => {
            tracing::error!("Feedback error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Contradictions request handler for HTTP
async fn handle_contradictions_request(
    axum::extract::Extension(handler): axum::extract::Extension<Arc<MemoryMcpHandler>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    let memory_id = params.get("memory_id").cloned();
    match handler.check_contradictions(memory_id).await {
        Ok(result) => Ok(axum::Json(result)),
        Err(e) => {
            tracing::error!("Contradiction check error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

use axum::http::StatusCode;
