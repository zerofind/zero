//! Service handler for processing JSON-RPC requests
//!
//! Routes incoming requests to appropriate handlers and manages
//! shared state (index, watchers, executor).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;
use tokio::sync::RwLock;

use automation::{AutomationEvent, Executor, ExecutorConfig};
use cache::CacheDb;
use search::{FileTypeCategory, IndexManager, SearchQuery};

use super::logging::ServiceLogger;
use super::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

/// Service handler that processes JSON-RPC requests
pub struct ServiceHandler {
    /// Database connection
    db: Arc<CacheDb>,

    /// Automation executor
    executor: Arc<Executor>,

    /// Search index manager (loaded into memory)
    index_manager: Arc<RwLock<Option<IndexManager>>>,

    /// Logger
    logger: ServiceLogger,
}

impl ServiceHandler {
    /// Create a new service handler
    pub fn new(db: CacheDb, logger: ServiceLogger) -> Result<Self, cache::CacheError> {
        let db = Arc::new(db);
        let executor_db = CacheDb::open()?;
        let executor = Executor::with_db(executor_db, ExecutorConfig::default());

        Ok(Self {
            db,
            executor: Arc::new(executor),
            index_manager: Arc::new(RwLock::new(None)),
            logger,
        })
    }

    /// Create handler with existing executor
    pub fn with_executor(db: Arc<CacheDb>, executor: Arc<Executor>, logger: ServiceLogger) -> Self {
        Self {
            db,
            executor,
            index_manager: Arc::new(RwLock::new(None)),
            logger,
        }
    }

    /// Get database reference
    pub fn db(&self) -> &CacheDb {
        &self.db
    }

    /// Get executor reference
    pub fn executor(&self) -> &Executor {
        &self.executor
    }

    /// Load search indexes into memory via `IndexManager`
    pub async fn load_index(&self) -> Result<u64, String> {
        self.logger.info("handler", "Loading search indexes");

        let manager = IndexManager::load().map_err(|e| e.to_string())?;
        let file_count = manager.total_file_count() as u64;

        let mut guard = self.index_manager.write().await;
        *guard = Some(manager);

        self.logger.info(
            "handler",
            &format!("Search index loaded: {file_count} files"),
        );

        Ok(file_count)
    }

    /// Check if index is loaded
    pub async fn is_index_loaded(&self) -> bool {
        self.index_manager.read().await.is_some()
    }

    /// Get index file count (if loaded)
    pub async fn index_file_count(&self) -> Option<u64> {
        self.index_manager
            .read()
            .await
            .as_ref()
            .map(|mgr| mgr.total_file_count() as u64)
    }

    /// Handle a JSON-RPC request and return response
    pub async fn handle_request(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let start = Instant::now();

        self.logger.debug(
            "handler",
            &format!("Request: id={} method={}", request.id, request.method),
        );

        // Validate request
        if let Err(msg) = request.validate() {
            return JsonRpcResponse::error(request.id, JsonRpcError::invalid_request(msg));
        }

        // Route to handler
        let result = match request.method.as_str() {
            // Health/status
            "ping" => self.handle_ping(request),
            "status" => self.handle_status(request).await,

            // Search
            "search" => self.handle_search(request).await,
            "search.index" => self.handle_search_index(request).await,
            "search.reload" => self.handle_search_reload(request).await,

            // Drives
            "drives" => self.handle_drives(request),

            // Automations
            "automation.list" => self.handle_automation_list(request),
            "automation.get" => self.handle_automation_get(request),
            "automation.create" => self.handle_automation_create(request),
            "automation.delete" => self.handle_automation_delete(request),
            "automation.run" => self.handle_automation_run(request).await,
            "automation.history" => self.handle_automation_history(request),

            // Unknown method
            _ => Err(JsonRpcError::method_not_found(&request.method)),
        };

        let duration = start.elapsed();
        self.logger.debug(
            "handler",
            &format!(
                "Response: id={} duration={:.2}ms success={}",
                request.id,
                duration.as_secs_f64() * 1000.0,
                result.is_ok()
            ),
        );

        match result {
            Ok(value) => JsonRpcResponse::success(request.id, value),
            Err(error) => JsonRpcResponse::error(request.id, error),
        }
    }

    // =========================================================================
    // Request handlers
    // =========================================================================

    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    fn handle_ping(&self, _request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        Ok(serde_json::json!({
            "alive": true,
            "version": env!("CARGO_PKG_VERSION"),
        }))
    }

    async fn handle_status(&self, _request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        let index_loaded = self.is_index_loaded().await;
        let file_count = self.index_file_count().await;

        let automations = self
            .db
            .list_automations()
            .map_err(|e| JsonRpcError::database_error(e.to_string()))?;

        Ok(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "index_loaded": index_loaded,
            "index_file_count": file_count,
            "automations_count": automations.len(),
        }))
    }

    async fn handle_search(&self, request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        let query: String = request
            .get_param("query")
            .unwrap_or_else(|| ".".to_string());
        let limit: usize = request.get_param("limit").unwrap_or(100);
        let type_filter: Option<String> = request.get_param("type");

        // Ensure index is loaded
        let guard = self.index_manager.read().await;
        let manager = guard.as_ref().ok_or_else(JsonRpcError::index_not_ready)?;

        // Build unified search query
        let type_category = type_filter.as_deref().and_then(FileTypeCategory::parse_str);

        let q = SearchQuery::text(&query, limit).with_type_opt(type_category);
        let results = manager.query(q);

        let total_indexed = manager.total_file_count();

        // Convert results to JSON
        let results_json: Vec<Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "path": r.node.path,
                    "name": r.node.name(),
                    "size": r.node.size,
                    "is_dir": r.node.is_directory(),
                    "score": r.score,
                })
            })
            .collect();

        Ok(serde_json::json!({
            "results": results_json,
            "count": results_json.len(),
            "total_indexed": total_indexed,
        }))
    }

    async fn handle_search_index(&self, request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        let path: String = request
            .require_param("path")
            .map_err(JsonRpcError::invalid_params)?;

        let path = PathBuf::from(&path);
        if !path.exists() {
            return Err(JsonRpcError::not_found(format!(
                "Path does not exist: {}",
                path.display()
            )));
        }

        self.logger
            .info("handler", &format!("Building index for {}", path.display()));

        // Build index via IndexManager
        let root = path.to_string_lossy().to_string();
        let mut manager =
            IndexManager::new().map_err(|e| JsonRpcError::filesystem_error(e.to_string()))?;

        manager
            .add_root(&root)
            .map_err(|e| JsonRpcError::filesystem_error(e.to_string()))?;

        let file_count = manager.total_file_count();
        let total_bytes = manager.total_bytes();

        // Update in-memory manager
        let mut guard = self.index_manager.write().await;
        *guard = Some(manager);

        self.logger.info(
            "handler",
            &format!("Index built: {file_count} files, {total_bytes} bytes"),
        );

        Ok(serde_json::json!({
            "success": true,
            "file_count": file_count,
            "total_bytes": total_bytes,
        }))
    }

    async fn handle_search_reload(&self, _request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        let file_count = self
            .load_index()
            .await
            .map_err(JsonRpcError::filesystem_error)?;

        Ok(serde_json::json!({
            "success": true,
            "file_count": file_count,
        }))
    }

    #[allow(clippy::unused_self)]
    fn handle_drives(&self, _request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        use disk::{DiskInfo, VolumeInfo};
        use std::path::Path;

        let volumes =
            VolumeInfo::all().map_err(|e| JsonRpcError::filesystem_error(e.to_string()))?;

        let mut drives = Vec::new();

        for vol in volumes {
            let usb_info = DiskInfo::for_path(Path::new(&vol.mount_point))
                .ok()
                .and_then(|d| d.usb);

            drives.push(serde_json::json!({
                "name": vol.name,
                "path": vol.mount_point,
                "total_bytes": vol.size_bytes,
                "used_bytes": vol.used_bytes(),
                "free_bytes": vol.free_bytes,
                "used_percent": vol.usage_percent(),
                "file_system": vol.file_system,
                "bsd_name": vol.bsd_name,
                "is_internal": vol.physical_drive.is_internal,
                "usb": usb_info.map(|u| serde_json::json!({
                    "product_name": u.product_name,
                    "vendor_name": u.vendor_name,
                    "vendor_id": u.vendor_id,
                    "product_id": u.product_id,
                    "serial_number": u.serial_number,
                    "speed": u.speed.name(),
                    "usb_version": u.usb_version,
                })),
            }));
        }

        Ok(serde_json::json!({
            "drives": drives,
        }))
    }

    fn handle_automation_list(&self, _request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        let list = self
            .db
            .list_automations()
            .map_err(|e| JsonRpcError::database_error(e.to_string()))?;

        let automations: Vec<Value> = list
            .iter()
            .map(|a| {
                serde_json::json!({
                    "id": a.id,
                    "name": a.name,
                    "enabled": a.enabled,
                    "dest_device_serial": a.dest_device_serial,
                    "dest_volume_name": a.dest_volume_name,
                    "dest_path": a.dest_path,
                    "triggers": {
                        "on_mount": a.triggers.on_mount,
                        "on_change": a.triggers.on_change,
                        "on_schedule": a.triggers.on_schedule,
                    },
                    "paths_count": a.paths.len(),
                    "created_at": a.created_at,
                    "updated_at": a.updated_at,
                })
            })
            .collect();

        Ok(serde_json::json!({
            "automations": automations,
            "total": automations.len(),
        }))
    }

    fn handle_automation_get(&self, request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        let id: i64 = request
            .require_param("id")
            .map_err(JsonRpcError::invalid_params)?;

        let automation = self
            .db
            .get_automation(id)
            .map_err(|e| JsonRpcError::database_error(e.to_string()))?
            .ok_or_else(|| JsonRpcError::not_found(format!("Automation {id}")))?;

        Ok(serde_json::json!({
            "id": automation.id,
            "name": automation.name,
            "enabled": automation.enabled,
            "dest_device_serial": automation.dest_device_serial,
            "dest_volume_name": automation.dest_volume_name,
            "dest_path": automation.dest_path,
            "triggers": {
                "on_mount": automation.triggers.on_mount,
                "on_change": automation.triggers.on_change,
                "on_schedule": automation.triggers.on_schedule,
            },
            "paths": automation.paths.iter().map(|p| serde_json::json!({
                "source": p.source,
                "dest": p.dest,
                "exclude": p.exclude,
            })).collect::<Vec<_>>(),
            "settings": {
                "verify": automation.settings.verify,
                "delete_orphans": automation.settings.delete_orphans,
                "notify": automation.settings.notify,
                "debounce_ms": automation.settings.debounce_ms,
            },
            "created_at": automation.created_at,
            "updated_at": automation.updated_at,
        }))
    }

    fn handle_automation_create(&self, request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        use cache::{NewAutomation, PathMapping, Settings, Triggers};

        let name: String = request
            .require_param("name")
            .map_err(JsonRpcError::invalid_params)?;
        let source_path: String = request
            .require_param("source_path")
            .map_err(JsonRpcError::invalid_params)?;
        let dest_path: String = request
            .require_param("dest_path")
            .map_err(JsonRpcError::invalid_params)?;

        let on_mount: bool = request.get_param("on_mount").unwrap_or(true);
        let on_change: bool = request.get_param("on_change").unwrap_or(false);
        let verify: bool = request.get_param("verify").unwrap_or(true);
        let delete_orphans: bool = request.get_param("delete_orphans").unwrap_or(false);

        let new_automation = NewAutomation {
            name: name.clone(),
            dest_device_serial: request.get_param("device_serial"),
            dest_volume_name: request.get_param("volume_name"),
            dest_path: Some(dest_path.clone()),
            triggers: Triggers {
                on_mount,
                on_change,
                on_schedule: None,
            },
            paths: vec![PathMapping {
                source: source_path,
                dest: dest_path,
                exclude: vec![],
            }],
            settings: Settings {
                verify,
                delete_orphans,
                notify: true,
                debounce_ms: 500,
            },
        };

        let automation = self
            .db
            .create_automation(new_automation)
            .map_err(|e| JsonRpcError::database_error(e.to_string()))?;

        self.logger.info(
            "handler",
            &format!(
                "Created automation: id={} name={}",
                automation.id, automation.name
            ),
        );

        Ok(serde_json::json!({
            "id": automation.id,
            "name": automation.name,
        }))
    }

    fn handle_automation_delete(&self, request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        let id: i64 = request
            .require_param("id")
            .map_err(JsonRpcError::invalid_params)?;

        // Get automation name before deleting
        let automation = self
            .db
            .get_automation(id)
            .map_err(|e| JsonRpcError::database_error(e.to_string()))?
            .ok_or_else(|| JsonRpcError::not_found(format!("Automation {id}")))?;

        self.db
            .delete_automation(id)
            .map_err(|e| JsonRpcError::database_error(e.to_string()))?;

        self.logger.info(
            "handler",
            &format!("Deleted automation: id={} name={}", id, automation.name),
        );

        Ok(serde_json::json!({
            "id": id,
            "name": automation.name,
            "deleted": true,
        }))
    }

    async fn handle_automation_run(&self, request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        let id: i64 = request
            .require_param("id")
            .map_err(JsonRpcError::invalid_params)?;

        self.logger
            .info("handler", &format!("Running automation: id={id}"));

        let run_id = self
            .executor
            .handle_event(AutomationEvent::Manual { automation_id: id })
            .await
            .map_err(|e| JsonRpcError::internal_error(e.to_string()))?
            .first()
            .copied()
            .ok_or_else(|| {
                JsonRpcError::internal_error(
                    "Automation did not start - check if destination is available",
                )
            })?;

        Ok(serde_json::json!({
            "automation_id": id,
            "run_id": run_id,
            "status": "started",
        }))
    }

    fn handle_automation_history(&self, request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        let id: i64 = request
            .require_param("id")
            .map_err(JsonRpcError::invalid_params)?;
        let limit: usize = request.get_param("limit").unwrap_or(10);

        let automation = self
            .db
            .get_automation(id)
            .map_err(|e| JsonRpcError::database_error(e.to_string()))?
            .ok_or_else(|| JsonRpcError::not_found(format!("Automation {id}")))?;

        let history = self
            .db
            .list_runs_for_automation(id, limit)
            .map_err(|e| JsonRpcError::database_error(e.to_string()))?;

        let runs: Vec<Value> = history
            .iter()
            .map(|r| {
                // Sum up progress from all paths
                let (files_total, bytes_done) = r.progress.as_ref().map_or((0, 0), |paths| {
                    paths.iter().fold((0u64, 0u64), |(ft, bd), p| {
                        (ft + p.files_total, bd + p.bytes_done)
                    })
                });

                serde_json::json!({
                    "id": r.id,
                    "status": r.status.as_str(),
                    "trigger": r.trigger.as_ref().map(cache::TriggerType::as_str),
                    "started_at": r.started_at,
                    "completed_at": r.completed_at,
                    "files_total": files_total,
                    "bytes_transferred": bytes_done,
                })
            })
            .collect();

        Ok(serde_json::json!({
            "automation_id": id,
            "automation_name": automation.name,
            "runs": runs,
            "total": runs.len(),
        }))
    }
}
