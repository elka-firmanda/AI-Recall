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

/// Static files embedded in binary
static STATIC_DIR: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/ui");

/// Start HTTP servers - API and UI on separate ports
pub async fn start_http_server(config: AppConfig) -> Result<()> {
    let handler = Arc::new(MemoryMcpHandler::new(config.clone()).await?);
    
    // Initialize session auth
    let ui_password = std::env::var("AI_RECALL_UI_PASSWORD")
        .unwrap_or("admin".to_string());
    let session_auth = Arc::new(crate::auth::session::SessionAuth::new(ui_password));
    
    // Initialize upload queue
    let upload_queue = Arc::new(crate::upload::UploadQueue::new(
        handler.clone(),
        Arc::new(config.clone())
    ));
    
    // Start upload processor
    let _processor_handle = upload_queue.start_processor();
    
    // Build API server routes (public/MCP endpoints)
    let api_app = build_api_router(
        handler.clone(),
        session_auth.clone(),
        upload_queue.clone(),
        config.server.auth_token.clone(),
    );
    
    // Start API server
    let api_addr: std::net::SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()?;
    let api_listener = tokio::net::TcpListener::bind(&api_addr).await?;
    info!("API server listening on {}", api_addr);
    
    // Build UI server routes (if enabled)
    if config.ui_server.enabled {
        let ui_addr: std::net::SocketAddr = format!("{}:{}", config.ui_server.host, config.ui_server.port)
            .parse()?;
        let ui_listener = tokio::net::TcpListener::bind(&ui_addr).await?;
        
        let ui_app = build_ui_router(
            handler.clone(),
            session_auth.clone(),
            upload_queue.clone(),
        );
        
        info!("UI server listening on {}", ui_addr);
        info!("Web interface available at http://{}:{}/", config.ui_server.host, config.ui_server.port);
        
        // Run both servers concurrently
        let api_server = axum::serve(api_listener, api_app);
        let ui_server = axum::serve(ui_listener, ui_app);
        
        tokio::select! {
            result = api_server => result?,
            result = ui_server => result?,
        }
    } else {
        info!("UI server disabled");
        axum::serve(api_listener, api_app).await?;
    }
    
    Ok(())
}

/// Build API server router (MCP endpoints + health)
fn build_api_router(
    handler: Arc<MemoryMcpHandler>,
    _session_auth: Arc<crate::auth::session::SessionAuth>,
    _upload_queue: Arc<crate::upload::UploadQueue>,
    auth_token: Option<String>,
) -> axum::Router {
    if let Some(token) = auth_token {
        let auth_state = crate::auth::AuthState::new(token);
        
        axum::Router::new()
            // Health check (public)
            .route("/health", axum::routing::get(health_check))
            // MCP endpoints (protected)
            .route("/mcp", axum::routing::post(handle_mcp_request))
            .route("/feedback", axum::routing::post(handle_feedback_request))
            .route("/contradictions", axum::routing::get(handle_contradictions_request))
            // State
            .layer(axum::extract::Extension(handler))
            // Auth middleware
            .layer(axum::middleware::from_fn_with_state(
                auth_state.clone(),
                crate::auth::auth_middleware,
            ))
            .with_state(auth_state)
    } else {
        axum::Router::new()
            .route("/health", axum::routing::get(health_check))
            .route("/mcp", axum::routing::post(handle_mcp_request))
            .route("/feedback", axum::routing::post(handle_feedback_request))
            .route("/contradictions", axum::routing::get(handle_contradictions_request))
            .layer(axum::extract::Extension(handler))
    }
}

/// Build UI server router (Web interface + upload API)
fn build_ui_router(
    handler: Arc<MemoryMcpHandler>,
    session_auth: Arc<crate::auth::session::SessionAuth>,
    upload_queue: Arc<crate::upload::UploadQueue>,
) -> axum::Router {
    axum::Router::new()
        // Static files and UI
        .route("/", axum::routing::get(serve_index))
        .route("/static/*path", axum::routing::get(serve_static))
        // API endpoints with session auth
        .route("/api/login", axum::routing::post(handle_login))
        .route("/api/logout", axum::routing::post(handle_logout))
        .route("/api/session", axum::routing::get(handle_session_check))
        .route("/api/upload", axum::routing::post(handle_upload))
        .route("/api/upload/queue", axum::routing::get(handle_upload_queue))
        .route("/api/upload/status/:id", axum::routing::get(handle_upload_status))
        .route("/api/memories", axum::routing::get(handle_list_memories))
        .route("/api/search", axum::routing::get(handle_search))
        // State
        .layer(axum::extract::Extension(handler))
        .layer(axum::extract::Extension(session_auth))
        .layer(axum::extract::Extension(upload_queue))
}

/// Health check endpoint
async fn health_check() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "healthy",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Serve index.html
async fn serve_index() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../../ui/index.html"))
}

/// Serve static files
async fn serve_static(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Result<axum::response::Response, StatusCode> {
    let file_path = path.trim_start_matches('/');
    
    if let Some(file) = STATIC_DIR.get_file(file_path) {
        let content = file.contents();
        let mime_type = match file_path {
            p if p.ends_with(".css") => "text/css",
            p if p.ends_with(".js") => "application/javascript",
            p if p.ends_with(".html") => "text/html",
            p if p.ends_with(".png") => "image/png",
            p if p.ends_with(".jpg") || p.ends_with(".jpeg") => "image/jpeg",
            p if p.ends_with(".svg") => "image/svg+xml",
            _ => "application/octet-stream",
        };
        
        let response = axum::response::Response::builder()
            .header("Content-Type", mime_type)
            .body(axum::body::Body::from(content.to_vec()))
            .unwrap();
        
        Ok(response)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Login endpoint
async fn handle_login(
    axum::extract::Extension(session_auth): axum::extract::Extension<Arc<crate::auth::session::SessionAuth>>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    let password = body.get("password")
        .and_then(|p| p.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    match session_auth.login(password, &addr.to_string()).await {
        Ok(Some(session_id)) => Ok(axum::Json(serde_json::json!({
            "success": true,
            "session_id": session_id,
        }))),
        Ok(None) => Err(StatusCode::UNAUTHORIZED),
        Err(msg) => Ok(axum::Json(serde_json::json!({
            "success": false,
            "error": msg,
        }))),
    }
}

/// Logout endpoint
async fn handle_logout(
    axum::extract::Extension(session_auth): axum::extract::Extension<Arc<crate::auth::session::SessionAuth>>,
    headers: axum::http::HeaderMap,
) -> axum::Json<serde_json::Value> {
    if let Some(session_id) = headers.get("X-Session-ID")
        .and_then(|h| h.to_str().ok()) {
        session_auth.logout(session_id).await;
    }
    
    axum::Json(serde_json::json!({
        "success": true,
    }))
}

/// Session check endpoint
async fn handle_session_check(
    axum::extract::Extension(session_auth): axum::extract::Extension<Arc<crate::auth::session::SessionAuth>>,
    headers: axum::http::HeaderMap,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    let session_id = headers.get("X-Session-ID")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    let valid = session_auth.validate_session(session_id).await;
    
    if valid {
        Ok(axum::Json(serde_json::json!({
            "valid": true,
        })))
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// File upload endpoint
async fn handle_upload(
    axum::extract::Extension(upload_queue): axum::extract::Extension<Arc<crate::upload::UploadQueue>>,
    mut multipart: axum::extract::Multipart,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    let mut filename = String::new();
    let mut file_data = Vec::new();
    
    while let Some(mut field) = multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        let name = field.name().unwrap_or("").to_string();
        
        if name == "file" {
            filename = field.file_name().unwrap_or("unknown").to_string();
            while let Some(chunk) = field.chunk().await.map_err(|_| StatusCode::BAD_REQUEST)? {
                file_data.extend_from_slice(&chunk);
            }
        }
    }
    
    if filename.is_empty() || file_data.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    let job_id = upload_queue.add_job(filename, file_data.len(), file_data).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(axum::Json(serde_json::json!({
        "success": true,
        "job_id": job_id,
    })))
}

/// Upload queue endpoint
async fn handle_upload_queue(
    axum::extract::Extension(upload_queue): axum::extract::Extension<Arc<crate::upload::UploadQueue>>,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    let stats = upload_queue.get_stats().await;
    let jobs = upload_queue.get_jobs().await;
    
    Ok(axum::Json(serde_json::json!({
        "stats": stats,
        "items": jobs,
    })))
}

/// Upload status endpoint
async fn handle_upload_status(
    axum::extract::Extension(upload_queue): axum::extract::Extension<Arc<crate::upload::UploadQueue>>,
    axum::extract::Path(job_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    if let Some(job) = upload_queue.get_job(&job_id).await {
        Ok(axum::Json(serde_json::json!(job)))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// List memories endpoint
async fn handle_list_memories(
    axum::extract::Extension(handler): axum::extract::Extension<Arc<MemoryMcpHandler>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    use crate::models::ListMemoryRequest;
    
    let limit = params.get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(20);
    
    let request = ListMemoryRequest {
        memory_type: params.get("type").and_then(|t| t.parse().ok()),
        tags: Vec::new(),
        since: None,
        limit,
        offset: 0,
    };
    
    match handler.memory_list(request).await {
        Ok(result) => Ok(axum::Json(serde_json::json!(result))),
        Err(e) => {
            tracing::error!("List memories error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Search memories endpoint
async fn handle_search(
    axum::extract::Extension(handler): axum::extract::Extension<Arc<MemoryMcpHandler>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    use crate::models::SearchMemoryRequest;
    
    let query = params.get("q")
        .cloned()
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    let limit = params.get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(10);
    
    let request = SearchMemoryRequest {
        query,
        memory_type: None,
        tags: Vec::new(),
        min_confidence: None,
        limit,
        include_related: None,
        threshold: None,
    };
    
    match handler.memory_search(request).await {
        Ok(result) => Ok(axum::Json(serde_json::json!(result))),
        Err(e) => {
            tracing::error!("Search error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
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
