//! Append-only JSONL audit log for debates.
//!
//! Replaces v0.2's broker-mediated evidence store for debate-scoped events.
//! One file per debate at `$XDG_STATE_HOME/zornmesh/debates/<id>.jsonl`
//! (falling back to `$HOME/.local/state/zornmesh/debates` on systems without
//! `XDG_STATE_HOME`). Each line is a self-contained JSON object so a future
//! `zornmesh debate replay <id>` reader is just `BufReader::lines` +
//! `serde_json::from_str`.
//!
//! Why a flat file instead of the persistence subsystem the v0.2 substrate
//! used: the audit log here is a single-writer (the running debate) +
//! append-only sequence with no concurrent debate writing to the same file.
//! No need for the SQLite-style schema, hash chain, or retention policy that
//! the broader mesh store provides. If the user later wants those properties
//! they can configure the daemon to ingest these JSONL files into the store
//! out-of-band; v0.3 starts simple.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde_json::Value;

use super::run::PlatformResult;

/// Open audit-log handle. Files are flushed on every write so a crash
/// mid-debate still leaves the recorded portion on disk.
pub struct AuditLog {
    path: PathBuf,
    file: Mutex<File>,
}

impl AuditLog {
    /// Create the audit file under the standard XDG state dir.
    pub fn open(debate_id: &str) -> std::io::Result<Self> {
        Self::open_in(debate_id, &audit_dir()?)
    }

    /// Create the audit file under an explicit directory. Used by tests
    /// (the workspace forbids `unsafe`, so we cannot mutate env vars from
    /// test code; the override path is the supported alternative).
    pub fn open_in(debate_id: &str, dir: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join(format!("{debate_id}.jsonl"));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self {
            path,
            file: Mutex::new(file),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write(&self, record: &AuditRecord) -> std::io::Result<()> {
        let value = record.to_json();
        let mut buf = serde_json::to_vec(&value).expect("audit record is JSON-serializable");
        buf.push(b'\n');
        let mut guard = self
            .file
            .lock()
            .expect("audit file mutex not poisoned");
        guard.write_all(&buf)?;
        guard.flush()?;
        Ok(())
    }
}

/// One line in the JSONL audit. Each variant has a distinct `kind` so
/// readers can dispatch without prior schema knowledge.
#[derive(Debug, Clone)]
pub enum AuditRecord {
    DebateStarted {
        schema_version: String,
        debate_id: String,
        unix_ms: u64,
        originator: String,
        plan: String,
        repo: Option<String>,
        platforms: Vec<String>,
        per_platform_timeout_ms: u64,
        max_output_bytes: usize,
    },
    PlatformResult {
        debate_id: String,
        unix_ms: u64,
        result: PlatformResult,
    },
    DebateFinished {
        debate_id: String,
        unix_ms: u64,
        success_count: usize,
        total_platforms: usize,
    },
}

impl AuditRecord {
    pub fn to_json(&self) -> Value {
        match self {
            Self::DebateStarted {
                schema_version,
                debate_id,
                unix_ms,
                originator,
                plan,
                repo,
                platforms,
                per_platform_timeout_ms,
                max_output_bytes,
            } => serde_json::json!({
                "schema_version": schema_version,
                "kind": "debate_started",
                "debate_id": debate_id,
                "unix_ms": unix_ms,
                "originator": originator,
                "plan": plan,
                "repo": repo,
                "platforms": platforms,
                "per_platform_timeout_ms": per_platform_timeout_ms,
                "max_output_bytes": max_output_bytes,
            }),
            Self::PlatformResult {
                debate_id,
                unix_ms,
                result,
            } => serde_json::json!({
                "kind": "platform_result",
                "debate_id": debate_id,
                "unix_ms": unix_ms,
                "platform": result.platform.name(),
                "status": result.status.as_str(),
                "content": result.content,
                "stderr_excerpt": result.stderr_excerpt,
                "duration_ms": result.duration_ms,
                "exit_code": result.exit_code,
                "truncated": result.truncated,
                "bytes_read": result.bytes_read,
            }),
            Self::DebateFinished {
                debate_id,
                unix_ms,
                success_count,
                total_platforms,
            } => serde_json::json!({
                "kind": "debate_finished",
                "debate_id": debate_id,
                "unix_ms": unix_ms,
                "success_count": success_count,
                "total_platforms": total_platforms,
            }),
        }
    }
}

/// Resolve the directory where audit files live. Honors XDG_STATE_HOME, falls
/// back to `~/.local/state/zornmesh/debates`.
pub fn audit_dir() -> std::io::Result<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        let p = PathBuf::from(xdg).join("zornmesh").join("debates");
        return Ok(p);
    }
    let home = std::env::var_os("HOME").ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "HOME unset; cannot resolve audit dir",
        )
    })?;
    Ok(PathBuf::from(home)
        .join(".local")
        .join("state")
        .join("zornmesh")
        .join("debates"))
}

/// Read every record from a debate's audit file (used by `zornmesh debate replay`).
pub fn read_audit(debate_id: &str) -> std::io::Result<Vec<Value>> {
    read_audit_in(debate_id, &audit_dir()?)
}

/// Read records from an audit file in an explicit directory (test helper).
pub fn read_audit_in(debate_id: &str, dir: &Path) -> std::io::Result<Vec<Value>> {
    let path = dir.join(format!("{debate_id}.jsonl"));
    let bytes = fs::read(&path)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            out.push(v);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debate::platforms::Platform;
    use crate::debate::run::Status;

    #[test]
    fn write_and_read_round_trips() {
        let tmp = tempdir_unique();
        let log = AuditLog::open_in("deb-test-1", &tmp).expect("open audit");
        log.write(&AuditRecord::DebateStarted {
            schema_version: "zornmesh.debate.v1".to_owned(),
            debate_id: "deb-test-1".to_owned(),
            unix_ms: 100,
            originator: "agent.driver.test".to_owned(),
            plan: "test plan".to_owned(),
            repo: None,
            platforms: vec!["claude".to_owned()],
            per_platform_timeout_ms: 1000,
            max_output_bytes: 1024,
        })
        .expect("write start");
        log.write(&AuditRecord::PlatformResult {
            debate_id: "deb-test-1".to_owned(),
            unix_ms: 200,
            result: PlatformResult {
                platform: Platform::Claude,
                status: Status::Success,
                content: "ok".to_owned(),
                stderr_excerpt: String::new(),
                duration_ms: 50,
                exit_code: Some(0),
                truncated: false,
                bytes_read: 2,
            },
        })
        .expect("write result");
        log.write(&AuditRecord::DebateFinished {
            debate_id: "deb-test-1".to_owned(),
            unix_ms: 300,
            success_count: 1,
            total_platforms: 1,
        })
        .expect("write finish");

        let records = read_audit_in("deb-test-1", &tmp).expect("read audit");
        assert_eq!(records.len(), 3);
        assert_eq!(records[0]["kind"], "debate_started");
        assert_eq!(records[1]["kind"], "platform_result");
        assert_eq!(records[1]["platform"], "claude");
        assert_eq!(records[2]["kind"], "debate_finished");
    }

    fn tempdir_unique() -> PathBuf {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!("zornmesh-audit-test-{id}"));
        std::fs::create_dir_all(&p).expect("mkdir temp");
        p
    }
}
