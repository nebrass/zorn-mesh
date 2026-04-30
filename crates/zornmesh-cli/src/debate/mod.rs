//! Multi-agent debate, v0.3 shape.
//!
//! v0.3 collapses the v0.2 broker/worker/daemon substrate into a single
//! parallel-spawn engine. The MCP host (Claude Code, Copilot CLI, Gemini
//! CLI, OpenCode) invokes the `zornmesh.debate` MCP tool; the bridge calls
//! `debate::run::run_debate`, which spawns each requested platform's CLI
//! as a subprocess in its own thread, applies a per-platform timeout, and
//! returns a structured per-platform result.
//!
//! Replaces:
//! - `debate::worker` (no persistent worker daemons)
//! - `debate::orchestrator` (no broker pub/sub orchestration)
//! - `debate::cli_runner` (no SDK-mediated cross-process coordination)
//!
//! Keeps:
//! - `debate::platforms`: per-platform argv adapter
//! - schema version constant for envelope-shaped audit records

pub mod audit;
pub mod platforms;
pub mod run;

pub use audit::{AuditLog, AuditRecord, audit_dir, read_audit, read_audit_in};
pub use platforms::{Platform, PlatformAdapter};
pub use run::{
    DEFAULT_MAX_OUTPUT_BYTES, DEFAULT_PER_PLATFORM_TIMEOUT, DebateRun, DebateRunOptions,
    PlatformResult, Status, run_debate,
};

/// Schema version pinned in the audit log so future v0.4+ readers can tell
/// what they're parsing. The value travels with every `debate_started`
/// record written by `run_debate`.
pub const DEBATE_SCHEMA_VERSION: &str = "zornmesh.debate.v1";

/// Common debate-pipeline error type. Returned only for problems that
/// prevent the debate from starting at all (invalid plan, no platforms).
/// Per-platform failures are reported as `Status::*` variants on the
/// individual `PlatformResult`s and do NOT cause the whole call to error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebateError {
    InvalidPlan(String),
    AuditWriteFailed(String),
}

impl DebateError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidPlan(_) => "E_DEBATE_INVALID_PLAN",
            Self::AuditWriteFailed(_) => "E_DEBATE_AUDIT_FAILED",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::InvalidPlan(m) | Self::AuditWriteFailed(m) => m,
        }
    }
}
