//! Platform plugins. Each adapter knows how to take a critique prompt and
//! invoke the underlying coding agent's headless / non-interactive mode,
//! then parse the response back to a string.
//!
//! v0.2 strategy: spawn the platform's CLI as a subprocess with stdin
//! carrying the prompt where supported, capture stdout, ignore stderr.
//! Each platform has its own conventions; the adapter trait abstracts them.

use std::{
    io::Write,
    process::{Command, Stdio},
    time::Duration,
};

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
}

/// One invocation of an underlying coding-agent CLI.
#[derive(Debug, Clone)]
pub struct PlatformInvocation {
    pub stdout: String,
    pub stderr: String,
    pub exit_status: Option<i32>,
}

pub trait PlatformAdapter: Send + Sync {
    fn platform(&self) -> Platform;
    /// Invoke the underlying agent CLI with the prompt. Implementors should
    /// honor `cwd` (repo path) and `timeout` strictly to bound runaway calls.
    fn invoke(
        &self,
        prompt: &str,
        cwd: Option<&str>,
        timeout: Duration,
    ) -> std::io::Result<PlatformInvocation>;
}

pub fn adapter_for(platform: Platform) -> Box<dyn PlatformAdapter> {
    match platform {
        Platform::Claude => Box::new(ClaudeAdapter),
        Platform::Copilot => Box::new(CopilotAdapter),
        Platform::Gemini => Box::new(GeminiAdapter),
        Platform::Opencode => Box::new(OpencodeAdapter),
    }
}

// ----- helpers -----

fn run_with_stdin(
    program: &str,
    args: &[&str],
    cwd: Option<&str>,
    stdin_text: &str,
    _timeout: Duration,
) -> std::io::Result<PlatformInvocation> {
    // Note: timeout is intentionally unused in v0.2; std::process doesn't have
    // a built-in wait-with-timeout. Workers wrap each call in a thread + recv
    // loop with a timeout (see worker.rs); the orchestrator's own deadline
    // additionally bounds end-to-end latency. Adding wait4-with-timeout via
    // libc is a v0.3 hardening item.
    let mut command = Command::new(program);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(stdin_text.as_bytes());
        // Drop the handle to signal EOF.
    }
    let output = child.wait_with_output()?;
    Ok(PlatformInvocation {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_status: output.status.code(),
    })
}

// ----- per-platform adapters -----

pub struct ClaudeAdapter;

impl PlatformAdapter for ClaudeAdapter {
    fn platform(&self) -> Platform {
        Platform::Claude
    }
    fn invoke(
        &self,
        prompt: &str,
        cwd: Option<&str>,
        timeout: Duration,
    ) -> std::io::Result<PlatformInvocation> {
        // `claude --print` reads the prompt from stdin and emits the model's
        // text response to stdout. `--output-format text` keeps the response
        // free of tool-call framing.
        run_with_stdin(
            "claude",
            &["--print", "--output-format", "text"],
            cwd,
            prompt,
            timeout,
        )
    }
}

pub struct CopilotAdapter;

impl PlatformAdapter for CopilotAdapter {
    fn platform(&self) -> Platform {
        Platform::Copilot
    }
    fn invoke(
        &self,
        prompt: &str,
        cwd: Option<&str>,
        timeout: Duration,
    ) -> std::io::Result<PlatformInvocation> {
        // GitHub Copilot CLI's headless invocation uses `copilot -p` with the
        // prompt piped over stdin since v1.0.30. The CLI insists on running
        // inside a git repo, which is satisfied by setting cwd to the repo
        // path the orchestrator passed in.
        run_with_stdin("copilot", &["-p"], cwd, prompt, timeout)
    }
}

pub struct GeminiAdapter;

impl PlatformAdapter for GeminiAdapter {
    fn platform(&self) -> Platform {
        Platform::Gemini
    }
    fn invoke(
        &self,
        prompt: &str,
        cwd: Option<&str>,
        timeout: Duration,
    ) -> std::io::Result<PlatformInvocation> {
        // `gemini --print` reads the prompt from stdin in non-interactive mode
        // and writes the model output to stdout. `--no-color` keeps the
        // output free of ANSI escapes.
        run_with_stdin("gemini", &["--print"], cwd, prompt, timeout)
    }
}

pub struct OpencodeAdapter;

impl PlatformAdapter for OpencodeAdapter {
    fn platform(&self) -> Platform {
        Platform::Opencode
    }
    fn invoke(
        &self,
        prompt: &str,
        cwd: Option<&str>,
        timeout: Duration,
    ) -> std::io::Result<PlatformInvocation> {
        // OpenCode's `opencode run` accepts the prompt via stdin and emits
        // text output. Sticking to text mode keeps the worker's parsing
        // trivial; structured output (JSON) can be a v0.3 addition.
        run_with_stdin("opencode", &["run"], cwd, prompt, timeout)
    }
}
