//! Parallel-spawn debate engine.
//!
//! v0.3 collapses the v0.2 broker/worker/daemon substrate into a single
//! function that spawns each requested platform CLI as a subprocess in its
//! own OS thread, reads bounded stdout, applies a per-platform timeout, and
//! returns a structured per-platform result. No persistent processes, no
//! pub/sub, no shared state.
//!
//! Failure-mode protections (per the v0.3 design review):
//! - Per-platform timeout via a shared `Arc<Mutex<Child>>` that the
//!   aggregator can `kill()` when its deadline fires.
//! - Bounded stdout reads (default 256 KiB) so a runaway model can't
//!   exhaust process memory; output past the cap is truncated with a
//!   sentinel marker.
//! - Process group detach (`process_group(0)`) so SIGTERM cascades to
//!   clean up subprocess trees if the parent is killed mid-debate.
//! - Missing-binary detection up front via `which`-style PATH probe;
//!   reported as a `Status::CliMissing` rather than a tool-call error.

use std::{
    io::Write as _,
    os::unix::process::CommandExt as _,
    path::PathBuf,
    process::Stdio,
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use super::{
    DEBATE_SCHEMA_VERSION, DebateError,
    audit::{AuditLog, AuditRecord},
    platforms::{Platform, adapter_for, default_command},
};

/// Default cap on per-platform stdout we keep in memory. Output past this is
/// truncated with a `[truncated; X bytes total]` sentinel so the user can
/// tell that the model went off and wrote a novel.
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 256 * 1024;

/// Default per-platform wall-clock timeout. Models occasionally hang on
/// rate-limit retry-after of 60+ seconds; cap the per-platform window so
/// one slow CLI can't take the whole debate down with it.
pub const DEFAULT_PER_PLATFORM_TIMEOUT: Duration = Duration::from_secs(60);

/// One debate run: parallel-spawn all requested platforms, collect, audit, return.
#[derive(Debug, Clone)]
pub struct DebateRunOptions {
    pub plan: String,
    pub repo: Option<String>,
    pub platforms: Vec<Platform>,
    pub originator: String,
    pub per_platform_timeout: Duration,
    pub max_output_bytes: usize,
    pub max_tokens_hint: Option<u64>,
    /// Override for the audit log directory. When `None`, the audit log
    /// resolves via `XDG_STATE_HOME` then `HOME`. Tests pass this
    /// explicitly because the workspace forbids `unsafe`, so they cannot
    /// mutate process env vars.
    pub audit_dir_override: Option<PathBuf>,
    /// When set, the engine resolves each platform's CLI as
    /// `<prefix>/<program>` instead of probing PATH. Tests use this to
    /// point at fake binaries without mutating the process PATH (the
    /// workspace forbids `unsafe`, which gates `std::env::set_var` in
    /// edition 2024).
    pub program_dir_override: Option<PathBuf>,
}

impl DebateRunOptions {
    pub fn new(plan: impl Into<String>) -> Self {
        Self {
            plan: plan.into(),
            repo: None,
            platforms: vec![
                Platform::Claude,
                Platform::Copilot,
                Platform::Gemini,
                Platform::Opencode,
            ],
            originator: "agent.driver.cli".to_owned(),
            per_platform_timeout: DEFAULT_PER_PLATFORM_TIMEOUT,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            max_tokens_hint: None,
            audit_dir_override: None,
            program_dir_override: None,
        }
    }

    pub fn with_audit_dir_override(mut self, dir: impl Into<PathBuf>) -> Self {
        self.audit_dir_override = Some(dir.into());
        self
    }

    pub fn with_program_dir_override(mut self, dir: impl Into<PathBuf>) -> Self {
        self.program_dir_override = Some(dir.into());
        self
    }

    pub fn with_repo(mut self, repo: impl Into<String>) -> Self {
        self.repo = Some(repo.into());
        self
    }

    pub fn with_platforms(mut self, platforms: Vec<Platform>) -> Self {
        self.platforms = platforms;
        self
    }

    pub fn with_originator(mut self, originator: impl Into<String>) -> Self {
        self.originator = originator.into();
        self
    }

    pub fn with_per_platform_timeout(mut self, timeout: Duration) -> Self {
        self.per_platform_timeout = timeout;
        self
    }

    pub fn with_max_output_bytes(mut self, bytes: usize) -> Self {
        self.max_output_bytes = bytes;
        self
    }
}

/// One platform's contribution to a debate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformResult {
    pub platform: Platform,
    pub status: Status,
    /// Trimmed model response. Empty on errors.
    pub content: String,
    /// First 4 KiB of stderr if the subprocess produced any. Useful for
    /// debugging "exit 0 but garbage" scenarios.
    pub stderr_excerpt: String,
    /// Wall-clock duration from spawn to return.
    pub duration_ms: u64,
    /// Subprocess exit code, if it terminated normally.
    pub exit_code: Option<i32>,
    /// True if the response was truncated past `max_output_bytes`.
    pub truncated: bool,
    /// Bytes consumed by the response BEFORE truncation, useful as a
    /// rough cost / verbosity signal.
    pub bytes_read: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Subprocess returned, exit 0, captured useful stdout.
    Success,
    /// Subprocess returned exit 0 but stdout was empty / whitespace-only.
    EmptyResponse,
    /// Subprocess exited non-zero. `stderr_excerpt` carries detail.
    NonZeroExit,
    /// Per-platform timeout fired; subprocess was killed.
    Timeout,
    /// `which <cli>` failed at startup; subprocess was never spawned.
    CliMissing,
    /// Spawn or pipe IO failure that wasn't covered above.
    SpawnFailed,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Success => "success",
            Status::EmptyResponse => "empty_response",
            Status::NonZeroExit => "non_zero_exit",
            Status::Timeout => "timeout",
            Status::CliMissing => "cli_missing",
            Status::SpawnFailed => "spawn_failed",
        }
    }
}

/// Final shape returned to callers.
#[derive(Debug, Clone)]
pub struct DebateRun {
    pub debate_id: String,
    pub schema_version: String,
    pub originator: String,
    pub plan: String,
    pub repo: Option<String>,
    pub started_unix_ms: u64,
    pub finished_unix_ms: u64,
    pub results: Vec<PlatformResult>,
    /// Path to the JSONL audit file written for this debate.
    pub audit_path: Option<PathBuf>,
}

impl DebateRun {
    /// Number of successful platforms.
    pub fn success_count(&self) -> usize {
        self.results.iter().filter(|r| r.status == Status::Success).count()
    }

    /// Number of platforms whose CLI was not on PATH.
    pub fn missing_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.status == Status::CliMissing)
            .count()
    }

    /// Synthesizes a human-readable consensus including dissent if any.
    pub fn human_consensus(&self) -> String {
        let mut s = String::new();
        s.push_str("ORIGINAL PLAN:\n");
        s.push_str(&self.plan);
        s.push_str("\n\nCRITIQUES:\n");
        if self.results.iter().all(|r| r.status != Status::Success) {
            s.push_str("(no successful critiques)\n");
        }
        for r in &self.results {
            match r.status {
                Status::Success => {
                    s.push_str(&format!(
                        "[{}] ({}ms{})\n{}\n\n",
                        r.platform.name(),
                        r.duration_ms,
                        if r.truncated { ", truncated" } else { "" },
                        r.content,
                    ));
                }
                Status::EmptyResponse => {
                    s.push_str(&format!(
                        "[{}] (empty response, exit={:?})\n",
                        r.platform.name(),
                        r.exit_code,
                    ));
                }
                Status::NonZeroExit => {
                    s.push_str(&format!(
                        "[{}] (failed, exit={:?})\n  stderr: {}\n",
                        r.platform.name(),
                        r.exit_code,
                        r.stderr_excerpt.lines().next().unwrap_or(""),
                    ));
                }
                Status::Timeout => {
                    s.push_str(&format!(
                        "[{}] (timed out after {}ms)\n",
                        r.platform.name(),
                        r.duration_ms,
                    ));
                }
                Status::CliMissing => {
                    s.push_str(&format!(
                        "[{}] (CLI not found on PATH; install it or remove from --platforms)\n",
                        r.platform.name(),
                    ));
                }
                Status::SpawnFailed => {
                    s.push_str(&format!(
                        "[{}] (spawn failed: {})\n",
                        r.platform.name(),
                        r.stderr_excerpt.lines().next().unwrap_or(""),
                    ));
                }
            }
        }
        s
    }
}

/// Run a debate end-to-end and return the structured outcome.
pub fn run_debate(options: DebateRunOptions) -> Result<DebateRun, DebateError> {
    if options.plan.trim().is_empty() {
        return Err(DebateError::InvalidPlan(
            "plan must be a non-empty string".to_owned(),
        ));
    }
    if options.platforms.is_empty() {
        return Err(DebateError::InvalidPlan(
            "no platforms requested for debate".to_owned(),
        ));
    }

    let debate_id = generate_debate_id();
    let started_unix_ms = current_unix_ms();
    let prompt = build_critique_prompt(&options.plan, options.repo.as_deref(), &options.originator);

    // Pre-flight: open the audit log first so we can record everything.
    let audit_log = match &options.audit_dir_override {
        Some(dir) => AuditLog::open_in(&debate_id, dir).ok(),
        None => AuditLog::open(&debate_id).ok(),
    };

    if let Some(audit) = &audit_log {
        let _ = audit.write(&AuditRecord::DebateStarted {
            schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
            debate_id: debate_id.clone(),
            unix_ms: started_unix_ms,
            originator: options.originator.clone(),
            plan: options.plan.clone(),
            repo: options.repo.clone(),
            platforms: options.platforms.iter().map(|p| p.name().to_owned()).collect(),
            per_platform_timeout_ms: options.per_platform_timeout.as_millis() as u64,
            max_output_bytes: options.max_output_bytes,
        });
    }

    // Spawn one OS thread per platform. Each thread owns its subprocess and
    // sends its `PlatformResult` over a channel as soon as it's done.
    let (tx, rx) = mpsc::channel::<PlatformResult>();
    let mut handles = Vec::with_capacity(options.platforms.len());
    for platform in &options.platforms {
        let platform = *platform;
        let prompt = prompt.clone();
        let repo = options.repo.clone();
        let timeout = options.per_platform_timeout;
        let max_bytes = options.max_output_bytes;
        let program_dir = options.program_dir_override.clone();
        let tx = tx.clone();
        let handle = thread::spawn(move || {
            let result = invoke_platform(
                platform,
                &prompt,
                repo.as_deref(),
                timeout,
                max_bytes,
                program_dir.as_deref(),
            );
            let _ = tx.send(result);
        });
        handles.push(handle);
    }
    drop(tx); // close the sender-side once spawned; rx will see Disconnected after last result

    // Drain channel; total wall-clock cap = per_platform_timeout + small slack.
    let total_deadline =
        Instant::now() + options.per_platform_timeout + Duration::from_secs(2);
    let expected = options.platforms.len();
    let mut results: Vec<PlatformResult> = Vec::with_capacity(expected);
    while results.len() < expected {
        let remaining = total_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok(r) => {
                if let Some(audit) = &audit_log {
                    let _ = audit.write(&AuditRecord::PlatformResult {
                        debate_id: debate_id.clone(),
                        unix_ms: current_unix_ms(),
                        result: r.clone(),
                    });
                }
                results.push(r);
            }
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    // Best-effort: join the worker threads so we don't leak OS resources.
    for h in handles {
        let _ = h.join();
    }

    let finished_unix_ms = current_unix_ms();
    if let Some(audit) = &audit_log {
        let _ = audit.write(&AuditRecord::DebateFinished {
            debate_id: debate_id.clone(),
            unix_ms: finished_unix_ms,
            success_count: results.iter().filter(|r| r.status == Status::Success).count(),
            total_platforms: expected,
        });
    }

    Ok(DebateRun {
        debate_id,
        schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
        originator: options.originator,
        plan: options.plan,
        repo: options.repo,
        started_unix_ms,
        finished_unix_ms,
        results,
        audit_path: audit_log.map(|a| a.path().to_owned()),
    })
}

/// Spawn the platform's CLI, write the prompt to its stdin, read bounded
/// stdout, enforce timeout. Returns a structured result for every outcome
/// except an unrecoverable mpsc panic (which is fine to propagate).
fn invoke_platform(
    platform: Platform,
    prompt: &str,
    cwd: Option<&str>,
    timeout: Duration,
    max_bytes: usize,
    program_dir: Option<&std::path::Path>,
) -> PlatformResult {
    let started = Instant::now();

    // Resolve the binary. With program_dir set we look there first
    // (test path); otherwise fall back to PATH probing.
    let adapter = adapter_for(platform);
    let cli_program = adapter.program();
    let resolved = match program_dir {
        Some(dir) => {
            let candidate = dir.join(cli_program);
            if candidate.is_file() {
                Some(candidate)
            } else {
                None
            }
        }
        None => which_cli(cli_program),
    };
    if resolved.is_none() {
        return PlatformResult {
            platform,
            status: Status::CliMissing,
            content: String::new(),
            stderr_excerpt: format!("`{cli_program}` not found on PATH"),
            duration_ms: started.elapsed().as_millis() as u64,
            exit_code: None,
            truncated: false,
            bytes_read: 0,
        };
    }
    let resolved = resolved.expect("checked above");

    // Build the command using the resolved absolute path so PATH
    // stripping (Dock-launched MCP hosts on macOS) doesn't break us.
    let mut command = std::process::Command::new(&resolved);
    command.args(adapter.args());
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    let _ = default_command; // silence unused warning if compiler can't see use
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0); // detach so we can kill the whole process tree on timeout

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(err) => {
            return PlatformResult {
                platform,
                status: Status::SpawnFailed,
                content: String::new(),
                stderr_excerpt: format!("spawn `{cli_program}`: {err}"),
                duration_ms: started.elapsed().as_millis() as u64,
                exit_code: None,
                truncated: false,
                bytes_read: 0,
            };
        }
    };

    let pid = child.id();
    // Write the prompt; close stdin to signal EOF.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes());
        // drop closes the pipe
    }
    // Take stdout/stderr handles for the reader threads BEFORE we move
    // the Child into the waiter thread.
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_handle = stdout.map(|s| thread::spawn(move || read_bounded(s, max_bytes)));
    let stderr_handle = stderr.map(|s| thread::spawn(move || read_bounded(s, 4 * 1024)));

    // The waiter thread owns the Child and blocks on wait(). On timeout we
    // kill the subprocess via its PID using the system `kill` command —
    // this avoids needing libc::kill (which would require `unsafe`, which
    // the workspace forbids), and process_group(0) means the SIGTERM
    // cascades to child processes the platform CLI may have started.
    let (wait_tx, wait_rx) = mpsc::channel::<std::io::Result<std::process::ExitStatus>>();
    let waiter_handle = thread::spawn(move || {
        let result = child.wait();
        let _ = wait_tx.send(result);
    });

    let exit_status: Option<std::io::Result<std::process::ExitStatus>>;
    let mut timed_out = false;
    match wait_rx.recv_timeout(timeout) {
        Ok(result) => {
            exit_status = Some(result);
        }
        Err(RecvTimeoutError::Timeout) => {
            timed_out = true;
            // Kill the subprocess group. `kill -TERM -<pid>` targets the
            // entire process group when the leader's pgid == its pid, which
            // is what `process_group(0)` set up.
            let _ = std::process::Command::new("kill")
                .args(["-TERM", &format!("-{pid}")])
                .status();
            // Give it a beat to die; if it doesn't, SIGKILL.
            let cleanup_result = wait_rx.recv_timeout(Duration::from_secs(2));
            match cleanup_result {
                Ok(result) => exit_status = Some(result),
                Err(_) => {
                    let _ = std::process::Command::new("kill")
                        .args(["-KILL", &format!("-{pid}")])
                        .status();
                    let _ = wait_rx.recv_timeout(Duration::from_secs(1));
                    exit_status = None;
                }
            }
        }
        Err(RecvTimeoutError::Disconnected) => {
            exit_status = None;
        }
    }
    let _ = waiter_handle.join();

    let stdout_bytes = stdout_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_else(|| (Vec::new(), 0, false));
    let stderr_bytes = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_else(|| (Vec::new(), 0, false));

    let duration_ms = started.elapsed().as_millis() as u64;
    let exit_code = exit_status
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .and_then(|s| s.code());

    let content = String::from_utf8_lossy(&stdout_bytes.0).into_owned();
    let stderr_excerpt = String::from_utf8_lossy(&stderr_bytes.0).into_owned();

    let status = if timed_out {
        Status::Timeout
    } else if let Some(code) = exit_code {
        if code == 0 {
            if content.trim().is_empty() {
                Status::EmptyResponse
            } else {
                Status::Success
            }
        } else {
            Status::NonZeroExit
        }
    } else {
        Status::SpawnFailed
    };

    PlatformResult {
        platform,
        status,
        content: content.trim().to_owned(),
        stderr_excerpt: stderr_excerpt.trim().to_owned(),
        duration_ms,
        exit_code,
        truncated: stdout_bytes.2,
        bytes_read: stdout_bytes.1,
    }
}

/// Read up to `cap` bytes from a reader; returns (bytes_read, total_seen, truncated).
/// Continues consuming the reader past `cap` to avoid a SIGPIPE from the
/// child if it tried to write more — but doesn't keep the bytes.
fn read_bounded<R: std::io::Read>(mut reader: R, cap: usize) -> (Vec<u8>, usize, bool) {
    let mut buf = Vec::with_capacity(cap.min(64 * 1024));
    let mut total = 0usize;
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if buf.len() < cap {
                    let to_take = (cap - buf.len()).min(n);
                    buf.extend_from_slice(&chunk[..to_take]);
                    if buf.len() == cap {
                        truncated = true;
                    }
                }
                // Past the cap: keep draining so the child doesn't get SIGPIPE.
            }
            Err(_) => break,
        }
    }
    if truncated {
        let marker = format!("\n[truncated; {total} bytes total]");
        if buf.len() + marker.len() <= cap + marker.len() {
            buf.extend_from_slice(marker.as_bytes());
        }
    }
    (buf, total, truncated)
}

/// Look up a binary on PATH, mimicking `which`. Returns the resolved path.
fn which_cli(program: &str) -> Option<PathBuf> {
    if program.is_empty() {
        return None;
    }
    if program.contains(std::path::MAIN_SEPARATOR) {
        let p = PathBuf::from(program);
        if p.is_file() {
            return Some(p);
        }
        return None;
    }
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn build_critique_prompt(plan: &str, repo: Option<&str>, originator: &str) -> String {
    let repo_line = repo
        .map(|r| format!("Repo context: {r}\n"))
        .unwrap_or_default();
    format!(
        "You are participating in a multi-agent debate over a coding plan.\n\
         Originator: {originator}\n\
         {repo_line}\
         \n\
         Plan:\n{plan}\n\
         \n\
         Provide a focused critique in 4-8 sentences:\n\
         - What is correct about this plan.\n\
         - What is missing or risky.\n\
         - One specific improvement.\n\
         Be concrete; do not reformulate the plan.\n"
    )
}

fn generate_debate_id() -> String {
    let now = current_unix_ms();
    let counter = NEXT_DEBATE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("deb-{now:x}-{counter:04x}")
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

static NEXT_DEBATE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_bounded_truncates_past_cap() {
        let big = vec![b'x'; 10_000];
        let (out, total, truncated) = read_bounded(&big[..], 100);
        assert!(truncated);
        assert_eq!(total, 10_000);
        // First 100 bytes match, plus a truncation marker.
        assert!(out.starts_with(&big[..100]));
        assert!(String::from_utf8_lossy(&out).contains("[truncated; 10000 bytes total]"));
    }

    #[test]
    fn read_bounded_returns_all_when_under_cap() {
        let small = b"hello";
        let (out, total, truncated) = read_bounded(&small[..], 1024);
        assert!(!truncated);
        assert_eq!(total, 5);
        assert_eq!(out, b"hello");
    }
}
