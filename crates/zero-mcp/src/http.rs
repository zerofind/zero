use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::Response;
use rmcp::transport::StreamableHttpServerConfig;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::StreamableHttpService;
use tokio_util::sync::CancellationToken;

use zero::prelude::{CodeIndex, IndexManager};

use crate::server::ZeroMcpServer;

/// Build the axum router with MCP streaming HTTP service + Bearer auth.
pub(crate) fn build_router(
    manager: IndexManager,
    code: CodeIndex,
    api_key: String,
    cancel: CancellationToken,
) -> Router {
    let session_manager = Arc::new(LocalSessionManager::default());

    let config = StreamableHttpServerConfig {
        sse_keep_alive: Some(Duration::from_secs(15)),
        sse_retry: Some(Duration::from_secs(3)),
        stateful_mode: true,
        cancellation_token: cancel,
        ..Default::default()
    };

    // Wrap CodeIndex in Arc so the closure can clone it for each session
    let code = Arc::new(std::sync::Mutex::new(code));

    let mcp_service = StreamableHttpService::new(
        move || {
            // Clone the IndexManager (it supports Clone)
            let mgr = manager.clone();
            // Create a new CodeIndex for each session by loading from disk
            let ci = match zero::code::CodeIndex::new() {
                Ok(ci) => ci,
                Err(_) => {
                    // Fallback: clone the shared one
                    let guard = code.lock().expect("code lock poisoned");
                    // We can't clone CodeIndex, so create a fresh one
                    drop(guard);
                    zero::code::CodeIndex::new().unwrap_or_else(|_| {
                        zero::code::CodeIndex::with_dir(
                            std::env::temp_dir().join("zero-code-fallback"),
                        )
                        .expect("fallback code index")
                    })
                }
            };
            Ok(ZeroMcpServer::new(mgr, ci))
        },
        session_manager,
        config,
    );

    let auth_key = Arc::new(api_key);

    Router::new()
        .nest_service("/mcp", mcp_service)
        .layer(middleware::from_fn(move |req, next| {
            let key = Arc::clone(&auth_key);
            bearer_auth(key, req, next)
        }))
}

async fn bearer_auth(
    expected_key: Arc<String>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = &header[7..];
            if token == expected_key.as_str() {
                Ok(next.run(req).await)
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
