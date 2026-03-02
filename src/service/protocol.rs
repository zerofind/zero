//! JSON-RPC 2.0 protocol types for service communication
//!
//! Implements the JSON-RPC 2.0 specification for bidirectional
//! communication between the XPC daemon and Rust service.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 request from client (XPC daemon)
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    /// Protocol version (must be "2.0")
    pub jsonrpc: String,

    /// Request identifier (used to match response)
    pub id: u64,

    /// Method name to invoke
    pub method: String,

    /// Method parameters (optional)
    #[serde(default)]
    pub params: Value,
}

impl JsonRpcRequest {
    /// Validate the request
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.jsonrpc != "2.0" {
            return Err("Invalid JSON-RPC version");
        }
        if self.method.is_empty() {
            return Err("Method name is required");
        }
        Ok(())
    }

    /// Get a parameter by key, returning None if not found
    pub fn get_param<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        self.params
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get a required parameter, returning error message if missing
    pub fn require_param<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<T, String> {
        self.get_param(key)
            .ok_or_else(|| format!("Missing required parameter: {}", key))
    }
}

/// JSON-RPC 2.0 response to client
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    /// Protocol version
    pub jsonrpc: &'static str,

    /// Request identifier (matches request)
    pub id: u64,

    /// Result on success
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,

    /// Error on failure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Create a successful response
    pub fn success<T: Serialize>(id: u64, result: T) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(serde_json::to_value(result).unwrap_or(Value::Null)),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: u64, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        }
    }

    /// Serialize to JSON line (compact, newline-terminated)
    pub fn to_line(&self) -> String {
        let mut json = serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"jsonrpc":"2.0","id":0,"error":{"code":-32603,"message":"Serialization failed"}}"#
                .to_string()
        });
        json.push('\n');
        json
    }
}

/// JSON-RPC 2.0 error object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code
    pub code: i32,

    /// Error message
    pub message: String,

    /// Additional error data (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    /// Create a new error
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Create error with additional data
    pub fn with_data<T: Serialize>(code: i32, message: impl Into<String>, data: T) -> Self {
        Self {
            code,
            message: message.into(),
            data: serde_json::to_value(data).ok(),
        }
    }

    // Standard JSON-RPC error codes

    /// Parse error (-32700)
    pub fn parse_error(details: impl Into<String>) -> Self {
        Self::new(-32700, format!("Parse error: {}", details.into()))
    }

    /// Invalid request (-32600)
    pub fn invalid_request(details: impl Into<String>) -> Self {
        Self::new(-32600, format!("Invalid request: {}", details.into()))
    }

    /// Method not found (-32601)
    pub fn method_not_found(method: &str) -> Self {
        Self::new(-32601, format!("Method not found: {}", method))
    }

    /// Invalid params (-32602)
    pub fn invalid_params(details: impl Into<String>) -> Self {
        Self::new(-32602, format!("Invalid params: {}", details.into()))
    }

    /// Internal error (-32603)
    pub fn internal_error(details: impl Into<String>) -> Self {
        Self::new(-32603, format!("Internal error: {}", details.into()))
    }

    // Application-specific error codes (start at -32000)

    /// Database error (-32001)
    pub fn database_error(details: impl Into<String>) -> Self {
        Self::new(-32001, format!("Database error: {}", details.into()))
    }

    /// File system error (-32002)
    pub fn filesystem_error(details: impl Into<String>) -> Self {
        Self::new(-32002, format!("File system error: {}", details.into()))
    }

    /// Not found (-32003)
    pub fn not_found(what: impl Into<String>) -> Self {
        Self::new(-32003, format!("Not found: {}", what.into()))
    }

    /// Operation in progress (-32004)
    pub fn operation_in_progress(details: impl Into<String>) -> Self {
        Self::new(-32004, format!("Operation in progress: {}", details.into()))
    }

    /// Index not ready (-32005)
    pub fn index_not_ready() -> Self {
        Self::new(
            -32005,
            "Search index not ready. Run 'zero search --index <path>' first.".to_string(),
        )
    }
}

/// JSON-RPC 2.0 notification (server → client, no response expected)
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcNotification {
    /// Protocol version
    pub jsonrpc: &'static str,

    /// Event method name
    pub method: String,

    /// Event parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcNotification {
    /// Create a new notification
    pub fn new<T: Serialize>(method: impl Into<String>, params: T) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.into(),
            params: serde_json::to_value(params).ok(),
        }
    }

    /// Create notification without params
    pub fn empty(method: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.into(),
            params: None,
        }
    }

    /// Serialize to JSON line (compact, newline-terminated)
    pub fn to_line(&self) -> String {
        let mut json = serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"jsonrpc":"2.0","method":"error","params":{"message":"Serialization failed"}}"#
                .to_string()
        });
        json.push('\n');
        json
    }
}

// Event notification types

/// USB mount event parameters
#[derive(Debug, Clone, Serialize)]
pub struct UsbMountedParams {
    pub mount_point: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_serial: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity_bytes: Option<u64>,
    pub timestamp_ms: u64,
}

/// USB unmount event parameters
#[derive(Debug, Clone, Serialize)]
pub struct UsbUnmountedParams {
    pub mount_point: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_serial: Option<String>,
    pub timestamp_ms: u64,
}

/// File change event parameters
#[derive(Debug, Clone, Serialize)]
pub struct FileChangedParams {
    pub kind: String,
    pub paths: Vec<String>,
    pub watch_root: String,
    pub timestamp_ms: u64,
}

/// Sync progress event parameters
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct SyncProgressParams {
    pub automation_id: i64,
    pub run_id: i64,
    pub phase: String,
    pub percent: f64,
    pub files_processed: u64,
    pub bytes_processed: u64,
    pub current_file: Option<String>,
    pub message: String,
}

/// Sync completed event parameters
#[derive(Debug, Clone, Serialize)]
pub struct SyncCompletedParams {
    pub automation_id: i64,
    pub run_id: i64,
    pub status: String,
    pub files_added: u64,
    pub files_modified: u64,
    pub files_deleted: u64,
    pub files_unchanged: u64,
    pub bytes_transferred: u64,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

/// Service ready event (sent on startup)
#[derive(Debug, Clone, Serialize)]
pub struct ServiceReadyParams {
    pub version: String,
    pub index_loaded: bool,
    pub file_count: Option<u64>,
    pub watchers_active: bool,
    pub automations_count: u64,
    pub recovered_runs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_parsing() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"search","params":{"query":"test"}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, 1);
        assert_eq!(req.method, "search");
        assert_eq!(req.get_param::<String>("query"), Some("test".to_string()));
    }

    #[test]
    fn test_response_success() {
        let resp = JsonRpcResponse::success(1, serde_json::json!({"count": 42}));
        let line = resp.to_line();
        assert!(line.contains("\"id\":1"));
        assert!(line.contains("\"count\":42"));
        assert!(line.ends_with('\n'));
    }

    #[test]
    fn test_notification() {
        let notif = JsonRpcNotification::new(
            "event.usb_mounted",
            UsbMountedParams {
                mount_point: "/Volumes/USB".to_string(),
                volume_name: Some("USB".to_string()),
                device_serial: None,
                volume_uuid: None,
                capacity_bytes: None,
                timestamp_ms: 1234567890,
            },
        );
        let line = notif.to_line();
        assert!(line.contains("event.usb_mounted"));
        assert!(line.contains("/Volumes/USB"));
    }
}
