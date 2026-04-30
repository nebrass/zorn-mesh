//! Per-coding-agent platform adapters.
//!
//! Each adapter only declares (a) the program name to spawn and (b) any
//! per-platform argv. The actual subprocess handling — spawning, timeout,
//! bounded stdout, kill — lives in `debate::run`. This split keeps the
//! adapter trait tiny and makes adding a new platform a one-row change.

use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Claude,
    Copilot,
    Gemini,
    Opencode,
}

impl Platform {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "claude" => Some(Platform::Claude),
            "copilot" => Some(Platform::Copilot),
            "gemini" => Some(Platform::Gemini),
            "opencode" => Some(Platform::Opencode),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Platform::Claude => "claude",
            Platform::Copilot => "copilot",
            Platform::Gemini => "gemini",
            Platform::Opencode => "opencode",
        }
    }

    pub const fn all() -> &'static [Platform] {
        &[
            Platform::Claude,
            Platform::Copilot,
            Platform::Gemini,
            Platform::Opencode,
        ]
    }
}

pub trait PlatformAdapter: Send + Sync {
    fn platform(&self) -> Platform;
    /// Binary name on PATH. v0.3 uses simple names (`claude`, `copilot`,
    /// etc.) and resolves them via the `which`-style probe in `debate::run`
    /// before spawning, so MCP hosts launched from a Dock-stripped PATH get
    /// a clear `Status::CliMissing` rather than a mysterious spawn failure.
    fn program(&self) -> &'static str;
    /// Arguments passed to the CLI in non-interactive mode. Each platform
    /// has its own conventions — these were taken from the v0.2 worker
    /// adapters and confirmed against current CLI versions.
    fn args(&self) -> &'static [&'static str];
}

pub fn adapter_for(platform: Platform) -> Box<dyn PlatformAdapter> {
    match platform {
        Platform::Claude => Box::new(ClaudeAdapter),
        Platform::Copilot => Box::new(CopilotAdapter),
        Platform::Gemini => Box::new(GeminiAdapter),
        Platform::Opencode => Box::new(OpencodeAdapter),
    }
}

/// Build a `Command` from an adapter, configured with cwd if provided.
/// Stdio piping and the process-group split happen in `debate::run`.
pub fn default_command(adapter: &dyn PlatformAdapter, cwd: Option<&str>) -> Command {
    let mut cmd = Command::new(adapter.program());
    cmd.args(adapter.args());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd
}

pub struct ClaudeAdapter;
impl PlatformAdapter for ClaudeAdapter {
    fn platform(&self) -> Platform {
        Platform::Claude
    }
    fn program(&self) -> &'static str {
        "claude"
    }
    fn args(&self) -> &'static [&'static str] {
        // `claude --print` reads the prompt from stdin and emits the model's
        // text response to stdout; `--output-format text` keeps the response
        // free of tool-call framing.
        &["--print", "--output-format", "text"]
    }
}

pub struct CopilotAdapter;
impl PlatformAdapter for CopilotAdapter {
    fn platform(&self) -> Platform {
        Platform::Copilot
    }
    fn program(&self) -> &'static str {
        "copilot"
    }
    fn args(&self) -> &'static [&'static str] {
        // `copilot -p` reads the prompt from stdin in non-interactive mode.
        // The CLI requires a git repo as cwd; the orchestrator passes one
        // when the caller specified `--repo`.
        &["-p"]
    }
}

pub struct GeminiAdapter;
impl PlatformAdapter for GeminiAdapter {
    fn platform(&self) -> Platform {
        Platform::Gemini
    }
    fn program(&self) -> &'static str {
        "gemini"
    }
    fn args(&self) -> &'static [&'static str] {
        &["--print"]
    }
}

pub struct OpencodeAdapter;
impl PlatformAdapter for OpencodeAdapter {
    fn platform(&self) -> Platform {
        Platform::Opencode
    }
    fn program(&self) -> &'static str {
        "opencode"
    }
    fn args(&self) -> &'static [&'static str] {
        &["run"]
    }
}
