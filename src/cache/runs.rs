//! Run types for automation execution tracking
//!
//! Runs track the execution history of automations, including progress,
//! results, and resume state for interrupted syncs.
//! The actual persistence is handled by ControlState (etch-backed).

use serde::{Deserialize, Serialize};

/// Status of a run
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Success,
    Failed,
    Partial,
    Cancelled,
}

impl RunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::Success => "success",
            RunStatus::Failed => "failed",
            RunStatus::Partial => "partial",
            RunStatus::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "running" => Some(RunStatus::Running),
            "success" => Some(RunStatus::Success),
            "failed" => Some(RunStatus::Failed),
            "cancelled" => Some(RunStatus::Cancelled),
            "partial" => Some(RunStatus::Partial),
            _ => None,
        }
    }
}

/// What triggered the run
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TriggerType {
    Mount,
    Change,
    Manual,
    Schedule,
}

impl TriggerType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TriggerType::Mount => "mount",
            TriggerType::Change => "change",
            TriggerType::Manual => "manual",
            TriggerType::Schedule => "schedule",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "manual" => Some(TriggerType::Manual),
            "mount" => Some(TriggerType::Mount),
            "change" => Some(TriggerType::Change),
            "schedule" => Some(TriggerType::Schedule),
            _ => None,
        }
    }
}

/// Progress status for a single path within a run
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathStatus {
    Pending,
    Running,
    Complete,
    Failed,
    Skipped,
}

/// Progress for a single path being synced
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathProgress {
    pub source: String,
    pub dest: String,
    pub status: PathStatus,
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub error: Option<String>,
}

/// Overall progress for a run (array of path progress)
pub type Progress = Vec<PathProgress>;

/// Summary of sync results
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResultSummary {
    pub files_added: u64,
    pub files_modified: u64,
    pub files_deleted: u64,
    pub files_unchanged: u64,
    pub bytes_transferred: u64,
    pub verify_failures: u64,
}

/// A single file change record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub op: String, // "add", "modify", "delete"
    pub path: String,
    pub size: Option<u64>,
}

/// Full result of a completed run
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunResult {
    pub summary: ResultSummary,
    #[serde(default)]
    pub changes: Vec<FileChange>,
    #[serde(default)]
    pub errors: Vec<String>,
}

/// A run record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: i64,
    pub automation_id: i64,

    pub started_at: i64,
    pub completed_at: Option<i64>,

    pub status: RunStatus,
    pub trigger: Option<TriggerType>,

    pub progress: Option<Progress>,
    pub result: Option<RunResult>,

    pub resumable: bool,
    pub resume_state: Option<String>,
}
