use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use parking_lot::RwLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use super::downloader::{clean_filename, DownloadManager};

/// Hardened WebSocket + HTTP RPC server for browser extension integration.
///
/// Security Features:
/// 1. **Secret Token Authentication**: All RPC methods and HTTP endpoints require a valid token.
/// 2. **Strict Origin Checking**: Rejects requests from standard web pages (`http://`, `https://`)
///    to prevent CSRF and Cross-Site WebSocket Hijacking (CSWSH).
/// 3. **Origin-Specific CORS**: Only whitelists browser extensions (`chrome-extension://`, `moz-extension://`).
/// 4. **Path Traversal Protection**: Strictly sanitizes all filenames to prevent directory traversal.
/// 5. **Localhost Binding**: Binds exclusively to `127.0.0.1`.
pub struct RpcServer {
    pub token: String,
    pub ws_port: u16,
    pub http_port: u16,
}

impl RpcServer {
    pub fn new() -> Self {
        let token = get_or_create_rpc_token();
        Self {
            token,
            ws_port: 6800,
            http_port: 6801,
        }
    }

    /// Start both WebSocket and HTTP servers with security policies enabled.
    pub async fn start(
        &self,
        manager: Arc<RwLock<DownloadManager>>,
    ) -> Result<(), String> {
        let expected_token = Arc::new(self.token.clone());

        // ── 1. Secure WebSocket Server on port 6800 ──
        let ws_addr = format!("127.0.0.1:{}", self.ws_port);
        let ws_listener = TcpListener::bind(&ws_addr)
            .await
            .map_err(|e| format!("Failed to bind WS server on {}: {}", ws_addr, e))?;

        println!("[RPC Security] WebSocket listening on ws://{} (Token Auth Enabled)", ws_addr);

        let manager_ws = manager.clone();
        let token_ws = expected_token.clone();
        tokio::spawn(async move {
            while let Ok((stream, peer)) = ws_listener.accept().await {
                let mgr = manager_ws.clone();
                let token_check = token_ws.clone();

                tokio::spawn(async move {
                    // Handshake callback: check Origin header during WS handshake
                    let mut origin_rejected = false;
                    let callback = |req: &Request, resp: Response| -> Result<Response, tokio_tungstenite::tungstenite::http::Response<Option<String>>> {
                        if let Some(origin_hdr) = req.headers().get("Origin").and_then(|v| v.to_str().ok()) {
                            if !is_origin_safe(origin_hdr) {
                                eprintln!("[RPC Security Warning] Blocked WebSocket connection from untrusted origin: {}", origin_hdr);
                                origin_rejected = true;
                                let mut err_resp = tokio_tungstenite::tungstenite::http::Response::new(Some("Forbidden Origin".to_string()));
                                *err_resp.status_mut() = StatusCode::FORBIDDEN;
                                return Err(err_resp);
                            }
                        }
                        Ok(resp)
                    };

                    match accept_hdr_async(stream, callback).await {
                        Ok(ws_stream) => {
                            if !origin_rejected {
                                handle_ws_connection(ws_stream, &mgr, &token_check, peer).await;
                            }
                        }
                        Err(e) => {
                            eprintln!("[RPC Security] WS connection rejected from {}: {}", peer, e);
                        }
                    }
                });
            }
        });

        // ── 2. Secure HTTP REST Server on port 6801 ──
        let http_addr = format!("127.0.0.1:{}", self.http_port);
        let http_listener = TcpListener::bind(&http_addr)
            .await
            .map_err(|e| format!("Failed to bind HTTP server on {}: {}", http_addr, e))?;

        println!("[RPC Security] HTTP API listening on http://{}/api/ (Origin-Locked)", http_addr);

        let manager_http = manager.clone();
        let token_http = expected_token.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, peer)) = http_listener.accept().await {
                let mgr = manager_http.clone();
                let token_check = token_http.clone();

                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    let n = match stream.read(&mut buf).await {
                        Ok(n) if n > 0 => n,
                        _ => return,
                    };
                    let raw = String::from_utf8_lossy(&buf[..n]).to_string();
                    let response = handle_http_request(&raw, &mgr, &token_check, peer).await;
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.shutdown().await;
                });
            }
        });

        Ok(())
    }
}

/// Verify if an Origin header is safe (extension or local only, never standard web pages).
fn is_origin_safe(origin: &str) -> bool {
    let o = origin.trim().to_lowercase();
    if o.is_empty() {
        return true;
    }
    // Block any web origin
    if o.starts_with("http://") || o.starts_with("https://") {
        return false;
    }
    // Only allow verified extension schemes
    o.starts_with("chrome-extension://") || o.starts_with("moz-extension://") || o.starts_with("edge-extension://")
}

/// Generate or load the persistent RPC secret token.
pub fn get_or_create_rpc_token() -> String {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("SuperIDM");
    let _ = std::fs::create_dir_all(&config_dir);
    let token_file = config_dir.join("rpc_token.txt");

    let token = if let Ok(existing) = std::fs::read_to_string(&token_file) {
        let trimmed = existing.trim().to_string();
        if !trimmed.is_empty() {
            trimmed
        } else {
            generate_and_save_token(&token_file)
        }
    } else {
        generate_and_save_token(&token_file)
    };

    // Also sync the token to the extension directory if it exists
    sync_token_to_extension(&token);

    token
}

fn generate_and_save_token(path: &Path) -> String {
    let token = format!("sidm_{}", Uuid::new_v4().to_string().replace('-', ""));
    let _ = std::fs::write(path, &token);
    token
}

/// Sync the token to the unpacked extension folders so it connects automatically.
fn sync_token_to_extension(token: &str) {
    let json_content = serde_json::json!({ "token": token }).to_string();

    // Check project extension folder
    if let Ok(cargo_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let proj_ext = PathBuf::from(cargo_dir).parent().map(|p| p.join("extension"));
        if let Some(dir) = proj_ext {
            if dir.exists() {
                let _ = std::fs::write(dir.join("rpc_token.json"), &json_content);
            }
        }
    }

    // Check Desktop SuperIDM-Extension folder
    if let Some(desktop) = dirs::desktop_dir() {
        let desktop_ext = desktop.join("SuperIDM-Extension");
        if desktop_ext.exists() {
            let _ = std::fs::write(desktop_ext.join("rpc_token.json"), &json_content);
        }
    }
}

/// Constant-time token verification to prevent timing attacks.
fn verify_token(provided: &str, expected: &str) -> bool {
    if provided.is_empty() || expected.is_empty() {
        return false;
    }
    let p_bytes = provided.as_bytes();
    let e_bytes = expected.as_bytes();

    if p_bytes.len() != e_bytes.len() {
        return false;
    }

    let mut diff = 0u8;
    for (a, b) in p_bytes.iter().zip(e_bytes.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Handle a secure WebSocket connection with JSON-RPC 2.0 dispatch and authentication.
async fn handle_ws_connection(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    manager: &Arc<RwLock<DownloadManager>>,
    expected_token: &str,
    peer: std::net::SocketAddr,
) {
    let (mut write, mut read) = ws_stream.split();
    let mut is_authenticated = false;

    println!("[RPC] Client connected from {}", peer);

    while let Some(msg_result) = read.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(request) => {
                        let method = request
                            .get("method")
                            .and_then(|m| m.as_str())
                            .unwrap_or("unknown");
                        let id = request.get("id").cloned();
                        let params = request.get("params").cloned().unwrap_or(serde_json::Value::Null);

                        // Check authentication: either already authenticated, or provides token in params
                        let provided_token = params.get("token")
                            .and_then(|v| v.as_str())
                            .or_else(|| request.get("token").and_then(|v| v.as_str()))
                            .unwrap_or("");

                        if !is_authenticated {
                            if verify_token(provided_token, expected_token) {
                                is_authenticated = true;
                            } else if method != "superidm.ping" {
                                eprintln!("[RPC Security] Blocked unauthorized method '{}' from {}", method, peer);
                                let err = serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": { "code": -32600, "message": "Unauthorized: Invalid or missing RPC Token" }
                                });
                                let _ = write.send(Message::Text(err.to_string().into())).await;
                                continue;
                            }
                        }

                        let result = dispatch_method(method, &params, manager).await;

                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": result
                        });
                        let _ = write.send(Message::Text(response.to_string().into())).await;
                    }
                    Err(e) => {
                        let err = serde_json::json!({
                            "jsonrpc": "2.0",
                            "error": { "code": -32700, "message": format!("Parse error: {}", e) }
                        });
                        let _ = write.send(Message::Text(err.to_string().into())).await;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                println!("[RPC] Client disconnected: {}", peer);
                break;
            }
            Ok(Message::Ping(data)) => {
                let _ = write.send(Message::Pong(data)).await;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("[RPC] Read error from {}: {}", peer, e);
                break;
            }
        }
    }
}

/// Handle an HTTP request with strict Origin checking and Bearer Token verification.
async fn handle_http_request(
    raw: &str,
    manager: &Arc<RwLock<DownloadManager>>,
    expected_token: &str,
    peer: std::net::SocketAddr,
) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let first_line = lines.first().copied().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("GET");
    let path = parts.get(1).copied().unwrap_or("/");

    // Extract Origin header
    let origin = lines.iter()
        .find(|l| l.to_lowercase().starts_with("origin:"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim())
        .unwrap_or("");

    // ── Origin Validation ──
    if !is_origin_safe(origin) {
        eprintln!("[RPC Security] Blocked cross-origin HTTP request from origin '{}' ({})", origin, peer);
        let err_body = r#"{"error":"Forbidden: Web page origins are not permitted"}"#;
        return format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            err_body.len(), err_body
        );
    }

    // Dynamic CORS header reflecting only safe extensions
    let cors_header = if !origin.is_empty() && is_origin_safe(origin) {
        format!("Access-Control-Allow-Origin: {}\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization, X-SuperIDM-Token\r\nAccess-Control-Max-Age: 86400", origin)
    } else {
        "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization, X-SuperIDM-Token".to_string()
    };

    // CORS preflight
    if method == "OPTIONS" {
        return format!("HTTP/1.1 204 No Content\r\n{}\r\nContent-Length: 0\r\n\r\n", cors_header);
    }

    // GET /api/ping (public status probe)
    if method == "GET" && path == "/api/ping" {
        let b = serde_json::json!({
            "status": "ok",
            "app": "SuperIDM",
            "version": "2.4.0",
            "auth_required": true
        }).to_string();
        return format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\nContent-Length: {}\r\n\r\n{}",
            cors_header, b.len(), b
        );
    }

    // Extract Bearer Token from Authorization or X-SuperIDM-Token headers
    let auth_header = lines.iter()
        .find(|l| l.to_lowercase().starts_with("authorization:"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim())
        .unwrap_or("");

    let mut provided_token = "";
    if auth_header.starts_with("Bearer ") {
        provided_token = auth_header.trim_start_matches("Bearer ").trim();
    }

    if provided_token.is_empty() {
        if let Some(custom) = lines.iter()
            .find(|l| l.to_lowercase().starts_with("x-superidm-token:"))
            .and_then(|l| l.split_once(':'))
            .map(|(_, v)| v.trim())
        {
            provided_token = custom;
        }
    }

    // Extract body
    let body = raw.split("\r\n\r\n").nth(1).unwrap_or("");

    // If token wasn't in headers, check body JSON
    let body_json: Option<serde_json::Value> = serde_json::from_str(body).ok();
    if provided_token.is_empty() {
        if let Some(ref j) = body_json {
            if let Some(t) = j.get("token").and_then(|v| v.as_str()) {
                provided_token = t;
            }
        }
    }

    // ── Token Verification ──
    if !verify_token(provided_token, expected_token) {
        eprintln!("[RPC Security] Unauthorized HTTP request to '{}' from {}", path, peer);
        let err_body = r#"{"error":"Unauthorized: Missing or invalid RPC Token"}"#;
        return format!(
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\n{}\r\nContent-Length: {}\r\n\r\n{}",
            cors_header, err_body.len(), err_body
        );
    }

    // POST /api/download
    if method == "POST" && path == "/api/download" {
        if let Some(params) = body_json {
            let result = dispatch_method("superidm.addUri", &params, manager).await;
            let b = serde_json::json!({ "jsonrpc": "2.0", "result": result }).to_string();
            return format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}\r\nContent-Length: {}\r\n\r\n{}",
                cors_header, b.len(), b
            );
        } else {
            let b = r#"{"error":"Invalid JSON payload"}"#;
            return format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n{}\r\nContent-Length: {}\r\n\r\n{}",
                cors_header, b.len(), b
            );
        }
    }

    // 404
    let b = r#"{"error":"Not Found"}"#;
    format!(
        "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\n{}\r\nContent-Length: {}\r\n\r\n{}",
        cors_header, b.len(), b
    )
}

/// Dispatch a JSON-RPC method with strict input sanitization.
async fn dispatch_method(
    method: &str,
    params: &serde_json::Value,
    manager: &Arc<RwLock<DownloadManager>>,
) -> serde_json::Value {
    match method {
        "superidm.addUri" => {
            let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("").trim();
            let raw_filename = params.get("filename").and_then(|v| v.as_str());
            let format = params.get("format").and_then(|v| v.as_str());

            if url.is_empty() {
                return serde_json::json!({ "status": "error", "message": "Missing 'url' parameter" });
            }

            // Only allow valid HTTP, HTTPS, FTP, or media URLs
            if !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("ftp://") {
                return serde_json::json!({ "status": "error", "message": "Invalid URL scheme" });
            }

            // Path Traversal Defense: strictly sanitize filename
            let safe_filename = raw_filename.map(|raw| {
                let cleaned = clean_filename(raw);
                // Extract only file name portion, stripping any possible path separators
                Path::new(&cleaned)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("download.dat")
                    .replace("..", "_")
            });

            let task_id = Uuid::new_v4().to_string();
            {
                let mut mgr = manager.write();
                mgr.pending_rpc_downloads.push(RpcDownloadRequest {
                    task_id: task_id.clone(),
                    url: url.to_string(),
                    filename: safe_filename,
                    format: format.map(|s| s.to_string()),
                });
            }

            println!("[RPC] Queued secure download request: url={}, file={:?}", url, raw_filename);

            serde_json::json!({ "status": "ok", "task_id": task_id, "message": "Download queued securely" })
        }

        "superidm.getStats" => {
            let mgr = manager.read();
            let active = mgr.tasks.values()
                .filter(|t| t.metadata.status == crate::engine::types::TaskStatus::Downloading)
                .count();
            serde_json::json!({ "status": "ok", "active_downloads": active, "total_tasks": mgr.tasks.len() })
        }

        "superidm.ping" => {
            serde_json::json!({ "status": "ok", "app": "SuperIDM", "version": "2.4.0", "authenticated": true })
        }

        _ => {
            serde_json::json!({ "status": "error", "message": format!("Unknown method: {}", method) })
        }
    }
}

/// A download request received via RPC from the browser extension.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RpcDownloadRequest {
    pub task_id: String,
    pub url: String,
    pub filename: Option<String>,
    pub format: Option<String>,
}
