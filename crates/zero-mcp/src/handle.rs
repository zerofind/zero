use tokio_util::sync::CancellationToken;

use zero::prelude::IndexManager;

use crate::auth;
use crate::http;

pub struct McpConfig {
    pub port: u16,
    pub api_key: String,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            port: 45557,
            api_key: auth::load_or_create_api_key(),
        }
    }
}

pub struct McpHandle {
    pub cancel: CancellationToken,
    pub thread: Option<std::thread::JoinHandle<()>>,
    pub port: u16,
}

/// Start the MCP HTTP server on a dedicated thread with its own tokio runtime.
pub fn start_server(config: McpConfig, manager: IndexManager) -> McpHandle {
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let port = config.port;

    let thread = std::thread::Builder::new()
        .name("mcp-runtime".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("failed to create MCP tokio runtime");

            rt.block_on(async move {
                let router = http::build_router(manager, config.api_key, cancel_clone.clone());

                let addr = std::net::SocketAddr::from(([127, 0, 0, 1], config.port));
                let listener = match tokio::net::TcpListener::bind(addr).await {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!(port = config.port, error = %e, "MCP server: bind failed");
                        return;
                    }
                };

                tracing::info!(port = config.port, "MCP server started");

                axum::serve(listener, router)
                    .with_graceful_shutdown(cancel_clone.cancelled_owned())
                    .await
                    .ok();

                tracing::info!("MCP server stopped");
            });
        })
        .expect("failed to spawn MCP thread");

    McpHandle {
        cancel,
        thread: Some(thread),
        port,
    }
}

/// Stop the MCP server and join the thread.
pub fn stop_server(handle: &mut McpHandle) {
    handle.cancel.cancel();
    if let Some(thread) = handle.thread.take() {
        let _ = thread.join();
    }
}
