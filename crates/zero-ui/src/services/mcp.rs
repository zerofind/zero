use gpui::*;

use zero::code::CodeIndex;
use zero::prelude::IndexManager;
use zero_mcp::{McpConfig, McpHandle, generate_api_key};

#[allow(dead_code)]
pub enum McpEvent {
    Started { port: u16 },
    Stopped,
    Error(String),
}

impl EventEmitter<McpEvent> for McpService {}

pub struct McpService {
    handle: Option<McpHandle>,
    running: bool,
    port: u16,
    api_key: String,
}

impl McpService {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            handle: None,
            running: false,
            port: 45557,
            api_key: zero_mcp::auth::load_or_create_api_key(),
        }
    }

    pub fn start(&mut self, manager: IndexManager, port: u16, cx: &mut Context<Self>) {
        if self.running {
            return;
        }

        let config = McpConfig {
            port,
            api_key: self.api_key.clone(),
        };

        // Create a CodeIndex for the MCP server
        let code = CodeIndex::new().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Failed to create CodeIndex for MCP, using fallback");
            CodeIndex::with_dir(std::env::temp_dir().join("zero-code-mcp"))
                .expect("fallback code index")
        });

        let handle = zero_mcp::start_server(config, manager, code);
        self.port = handle.port;
        self.handle = Some(handle);
        self.running = true;
        cx.emit(McpEvent::Started { port: self.port });
        cx.notify();
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = &mut self.handle {
            zero_mcp::stop_server(handle);
        }
        self.handle = None;
        self.running = false;
        cx.emit(McpEvent::Stopped);
        cx.notify();
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn regenerate_api_key(&mut self) {
        self.api_key = generate_api_key();
        // Persist the new key
        if let Some(data_dir) = zero::dirs::data_dir() {
            let path = data_dir.join("mcp_api_key");
            let _ = std::fs::write(&path, &self.api_key);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
}
