#![doc = "Command skeleton for the public zornmesh CLI."]

use std::{
    fs,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
};

pub const ROOT_HELP: &str = include_str!("../../../fixtures/cli/root-help.stdout");
pub const DAEMON_HELP: &str = include_str!("../../../fixtures/cli/daemon-help.stdout");
pub const TRACE_HELP: &str = include_str!("../../../fixtures/cli/trace-help.stdout");
pub const VERSION: &str = "zornmesh 0.1.0\n";
const READ_SCHEMA_VERSION: &str = "zornmesh.cli.read.v1";
const EVENT_SCHEMA_VERSION: &str = "zornmesh.cli.event.v1";

pub fn run(args: impl IntoIterator<Item = String>) -> i32 {
    match parse_invocation(args.into_iter().collect()) {
        Ok(invocation) => match dispatch(invocation) {
            Ok(()) => 0,
            Err(error) => error.emit(),
        },
        Err(error) => error.emit(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Human,
    Json,
    Ndjson,
}

impl OutputFormat {
    fn parse(raw: &str) -> Result<Self, CliError> {
        match raw {
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            "ndjson" => Ok(Self::Ndjson),
            other => Err(CliError::new(
                "E_UNSUPPORTED_OUTPUT_FORMAT",
                format!(
                    "unsupported output format '{other}'; supported formats: human, json, ndjson"
                ),
                ExitKind::UserError,
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueSource {
    Default,
    Config,
    Env,
    Cli,
}

impl ValueSource {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Config => "config",
            Self::Env => "env",
            Self::Cli => "cli",
        }
    }
}

#[derive(Debug, Clone)]
struct EffectiveConfig {
    socket_path: PathBuf,
    socket_source: ValueSource,
}

#[derive(Debug, Clone)]
struct Invocation {
    args: Vec<String>,
    output: OutputFormat,
    non_interactive: bool,
    config: EffectiveConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitKind {
    UserError,
    Validation,
    NotFound,
    DaemonUnreachable,
    TemporaryUnavailable,
    PermissionDenied,
    Io,
}

impl ExitKind {
    const fn code(self) -> i32 {
        match self {
            Self::UserError => 64,
            Self::Validation => 65,
            Self::NotFound => 66,
            Self::DaemonUnreachable => 69,
            Self::Io => 74,
            Self::TemporaryUnavailable => 75,
            Self::PermissionDenied => 77,
        }
    }
}

#[derive(Debug, Clone)]
struct CliError {
    code: &'static str,
    message: String,
    kind: ExitKind,
}

impl CliError {
    fn new(code: &'static str, message: impl Into<String>, kind: ExitKind) -> Self {
        Self {
            code,
            message: message.into(),
            kind,
        }
    }

    fn emit(&self) -> i32 {
        eprintln!("{}: {}", self.code, self.message);
        self.kind.code()
    }
}

fn parse_invocation(args: Vec<String>) -> Result<Invocation, CliError> {
    let mut output = OutputFormat::Human;
    let mut non_interactive = false;
    let mut config_path: Option<PathBuf> = None;
    let mut cli_socket_path: Option<PathBuf> = None;
    let mut command_args = Vec::new();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        if arg == "--output" {
            let value = iter.next().ok_or_else(|| {
                CliError::new(
                    "E_INVALID_CONFIG",
                    "--output requires a format",
                    ExitKind::UserError,
                )
            })?;
            output = OutputFormat::parse(&value)?;
        } else if let Some(value) = arg.strip_prefix("--output=") {
            output = OutputFormat::parse(value)?;
        } else if arg == "--non-interactive" {
            non_interactive = true;
        } else if arg == "--config" {
            let value = iter.next().ok_or_else(|| {
                CliError::new(
                    "E_INVALID_CONFIG",
                    "--config requires a path",
                    ExitKind::UserError,
                )
            })?;
            if value.is_empty() {
                return Err(CliError::new(
                    "E_INVALID_CONFIG",
                    "--config requires a non-empty path",
                    ExitKind::UserError,
                ));
            }
            config_path = Some(PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--config=") {
            if value.is_empty() {
                return Err(CliError::new(
                    "E_INVALID_CONFIG",
                    "--config requires a non-empty path",
                    ExitKind::UserError,
                ));
            }
            config_path = Some(PathBuf::from(value));
        } else if arg == "--socket" {
            let value = iter.next().ok_or_else(|| {
                CliError::new(
                    "E_INVALID_CONFIG",
                    "--socket requires a path",
                    ExitKind::UserError,
                )
            })?;
            if value.is_empty() {
                return Err(CliError::new(
                    "E_INVALID_CONFIG",
                    "--socket requires a non-empty path",
                    ExitKind::UserError,
                ));
            }
            cli_socket_path = Some(PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--socket=") {
            if value.is_empty() {
                return Err(CliError::new(
                    "E_INVALID_CONFIG",
                    "--socket requires a non-empty path",
                    ExitKind::UserError,
                ));
            }
            cli_socket_path = Some(PathBuf::from(value));
        } else {
            command_args.push(arg);
        }
    }

    Ok(Invocation {
        args: command_args,
        output,
        non_interactive,
        config: resolve_effective_config(config_path.as_deref(), cli_socket_path)?,
    })
}

fn resolve_effective_config(
    config_path: Option<&Path>,
    cli_socket_path: Option<PathBuf>,
) -> Result<EffectiveConfig, CliError> {
    let mut socket_path =
        zornmesh_rpc::local::default_socket_path().map_err(cli_error_from_local)?;
    let mut socket_source = ValueSource::Default;

    if let Some(path) = config_path
        && let Some(config_socket) = read_config_socket_path(path)?
    {
        socket_path = config_socket;
        socket_source = ValueSource::Config;
    }

    if let Some(env_socket) = std::env::var_os(zornmesh_rpc::local::ENV_SOCKET_PATH) {
        if env_socket.is_empty() {
            return Err(CliError::new(
                "E_INVALID_CONFIG",
                "ZORN_SOCKET_PATH must not be empty",
                ExitKind::UserError,
            ));
        }
        socket_path = PathBuf::from(env_socket);
        socket_source = ValueSource::Env;
    }

    if let Some(path) = cli_socket_path {
        socket_path = path;
        socket_source = ValueSource::Cli;
    }

    Ok(EffectiveConfig {
        socket_path,
        socket_source,
    })
}

fn read_config_socket_path(path: &Path) -> Result<Option<PathBuf>, CliError> {
    let raw = fs::read_to_string(path).map_err(|error| {
        let kind = if error.kind() == std::io::ErrorKind::PermissionDenied {
            ExitKind::PermissionDenied
        } else {
            ExitKind::UserError
        };
        CliError::new(
            if kind == ExitKind::PermissionDenied {
                "E_PERMISSION_DENIED"
            } else {
                "E_INVALID_CONFIG"
            },
            format!("failed to read config '{}': {error}", path.display()),
            kind,
        )
    })?;

    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = trimmed.split_once('=').ok_or_else(|| {
            CliError::new(
                "E_INVALID_CONFIG",
                format!("config line {} must use key=value", index + 1),
                ExitKind::UserError,
            )
        })?;
        let key = key.trim();
        let value = value.trim();
        if key == "socket_path" || key == "socket" {
            if value.is_empty() {
                return Err(CliError::new(
                    "E_INVALID_CONFIG",
                    format!("{key} must not be empty"),
                    ExitKind::UserError,
                ));
            }
            return Ok(Some(PathBuf::from(value)));
        }
    }

    Ok(None)
}

fn cli_error_from_local(error: zornmesh_rpc::local::LocalError) -> CliError {
    let kind = match error.code() {
        zornmesh_rpc::local::LocalErrorCode::ExistingOwner => ExitKind::TemporaryUnavailable,
        zornmesh_rpc::local::LocalErrorCode::LocalTrustUnsafe
        | zornmesh_rpc::local::LocalErrorCode::ElevatedPrivilege => ExitKind::PermissionDenied,
        zornmesh_rpc::local::LocalErrorCode::DaemonUnreachable => ExitKind::DaemonUnreachable,
        zornmesh_rpc::local::LocalErrorCode::InvalidConfig => ExitKind::UserError,
        zornmesh_rpc::local::LocalErrorCode::Io => ExitKind::Io,
    };
    CliError::new(error.code().as_str(), error.message(), kind)
}

fn dispatch(invocation: Invocation) -> Result<(), CliError> {
    match invocation.args.as_slice() {
        [] => print_help("help", ROOT_HELP, invocation.output),
        [flag] if is_help(flag) => print_help("help", ROOT_HELP, invocation.output),
        [flag] if flag == "--version" || flag == "-V" => print_version(invocation.output),
        [command] if command == "help" => print_help("help", ROOT_HELP, invocation.output),
        [command] if command == "daemon" => run_daemon(&[], &invocation),
        [command, rest @ ..] if command == "daemon" => run_daemon(rest, &invocation),
        [command] if command == "agents" => run_agents(&[], &invocation),
        [command, rest @ ..] if command == "agents" => run_agents(rest, &invocation),
        [command] if command == "doctor" => run_doctor(&[], &invocation),
        [command, rest @ ..] if command == "doctor" => run_doctor(rest, &invocation),
        [command] if command == "trace" => print_help("trace help", TRACE_HELP, invocation.output),
        [command, rest @ ..] if command == "trace" => run_trace(rest, &invocation),
        [command, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh command '{command}'"),
            ExitKind::UserError,
        )),
    }
}

fn print_version(output: OutputFormat) -> Result<(), CliError> {
    match output {
        OutputFormat::Human => {
            print!("{VERSION}");
            Ok(())
        }
        OutputFormat::Json => {
            println!(
                "{{\"schema_version\":\"{READ_SCHEMA_VERSION}\",\"command\":\"version\",\"status\":\"ok\",\"data\":{{\"version\":\"0.1.0\"}},\"warnings\":[]}}"
            );
            Ok(())
        }
        OutputFormat::Ndjson => Err(ndjson_not_supported("version")),
    }
}

fn print_help(command: &str, text: &str, output: OutputFormat) -> Result<(), CliError> {
    match output {
        OutputFormat::Human => {
            print!("{text}");
            Ok(())
        }
        OutputFormat::Json => {
            println!(
                "{{\"schema_version\":\"{READ_SCHEMA_VERSION}\",\"command\":{},\"status\":\"ok\",\"data\":{{\"text\":{}}},\"warnings\":[]}}",
                json_string(command),
                json_string(text)
            );
            Ok(())
        }
        OutputFormat::Ndjson => Err(ndjson_not_supported(command)),
    }
}

fn run_daemon(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => {
            if invocation.output != OutputFormat::Human {
                return Err(CliError::new(
                    "E_UNSUPPORTED_OUTPUT_FORMAT",
                    "daemon foreground mode supports only human output",
                    ExitKind::UserError,
                ));
            }
            let config = zornmesh_daemon::DaemonConfig::for_socket_path(
                invocation.config.socket_path.clone(),
            );
            match zornmesh_daemon::run_foreground(config) {
                Ok(report) => match report.outcome {
                    zornmesh_daemon::ShutdownOutcome::Clean => Ok(()),
                    zornmesh_daemon::ShutdownOutcome::BudgetExceeded => Err(CliError::new(
                        "E_DAEMON_UNAVAILABLE",
                        "daemon shutdown budget exceeded",
                        ExitKind::TemporaryUnavailable,
                    )),
                },
                Err(error) => Err(cli_error_from_daemon(error)),
            }
        }
        [flag] if is_help(flag) => print_help("daemon help", DAEMON_HELP, invocation.output),
        [command] if command == "status" => daemon_status(false, invocation),
        [command, rest @ ..] if command == "status" => {
            let mut require_ready = false;
            for arg in rest {
                if arg == "--require-ready" {
                    require_ready = true;
                } else if is_help(arg) {
                    return print_help("daemon help", DAEMON_HELP, invocation.output);
                } else {
                    return Err(CliError::new(
                        "E_UNSUPPORTED_COMMAND",
                        format!("unsupported zornmesh daemon status argument '{arg}'"),
                        ExitKind::UserError,
                    ));
                }
            }
            daemon_status(require_ready, invocation)
        }
        [command] if command == "shutdown" => daemon_shutdown(invocation),
        [command, rest @ ..] if command == "shutdown" && rest.iter().any(|arg| is_help(arg)) => {
            print_help("daemon help", DAEMON_HELP, invocation.output)
        }
        [arg, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh daemon argument '{arg}'"),
            ExitKind::UserError,
        )),
    }
}

fn daemon_status(require_ready: bool, invocation: &Invocation) -> Result<(), CliError> {
    let state = daemon_state(&invocation.config.socket_path)?;
    if require_ready && state != "ready" {
        return Err(daemon_unreachable(&invocation.config.socket_path));
    }

    match invocation.output {
        OutputFormat::Human => {
            println!("zornmesh daemon status");
            println!("state: {state}");
            println!("socket: {}", invocation.config.socket_path.display());
            println!(
                "socket_source: {}",
                invocation.config.socket_source.as_str()
            );
            if state == "unreachable" {
                println!(
                    "remediation: start the daemon with `zornmesh daemon --socket {}`",
                    invocation.config.socket_path.display()
                );
            }
            Ok(())
        }
        OutputFormat::Json => {
            println!(
                "{{\"schema_version\":\"{READ_SCHEMA_VERSION}\",\"command\":\"daemon status\",\"status\":\"ok\",\"data\":{{\"daemon_state\":{},\"socket_path\":{},\"socket_source\":{}}},\"warnings\":[]}}",
                json_string(state),
                json_string(&invocation.config.socket_path.display().to_string()),
                json_string(invocation.config.socket_source.as_str())
            );
            Ok(())
        }
        OutputFormat::Ndjson => Err(ndjson_not_supported("daemon status")),
    }
}

fn daemon_state(socket_path: &Path) -> Result<&'static str, CliError> {
    match UnixStream::connect(socket_path) {
        Ok(_) => Ok("ready"),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => Err(CliError::new(
            "E_PERMISSION_DENIED",
            format!(
                "permission denied while connecting to daemon socket {}",
                socket_path.display()
            ),
            ExitKind::PermissionDenied,
        )),
        Err(_) => Ok("unreachable"),
    }
}

fn daemon_unreachable(socket_path: &Path) -> CliError {
    CliError::new(
        "E_DAEMON_UNREACHABLE",
        format!(
            "daemon is unreachable at {}; start the daemon or choose another socket",
            socket_path.display()
        ),
        ExitKind::DaemonUnreachable,
    )
}

fn daemon_shutdown(invocation: &Invocation) -> Result<(), CliError> {
    if invocation.non_interactive {
        return Err(CliError::new(
            "E_NON_INTERACTIVE_PROMPT_REQUIRED",
            "daemon shutdown requires confirmation; rerun interactively or wait for Story 1.7 shutdown support",
            ExitKind::UserError,
        ));
    }

    Err(CliError::new(
        "E_CONFIRMATION_REQUIRED",
        "daemon shutdown requires confirmation; rerun with --non-interactive to fail fast",
        ExitKind::UserError,
    ))
}

fn run_agents(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => agents_list(invocation),
        [flag] if is_help(flag) => print_agents_help(invocation.output),
        [command, agent_id] if command == "inspect" => agents_inspect(agent_id),
        [command, ..] if command == "inspect" => Err(CliError::new(
            "E_VALIDATION_FAILED",
            "agents inspect requires an agent id",
            ExitKind::Validation,
        )),
        [arg, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh agents argument '{arg}'"),
            ExitKind::UserError,
        )),
    }
}

fn agents_list(invocation: &Invocation) -> Result<(), CliError> {
    match invocation.output {
        OutputFormat::Human => {
            println!("zornmesh agents");
            println!("status: unavailable");
            println!("agents: 0");
            println!("warning: agent registry is not available in this scaffold");
            println!("remediation: connect agents after identity registration is enabled");
            Ok(())
        }
        OutputFormat::Json => {
            println!(
                "{{\"schema_version\":\"{READ_SCHEMA_VERSION}\",\"command\":\"agents\",\"status\":\"ok\",\"data\":{{\"registry_status\":\"unavailable\",\"agents\":[]}},\"warnings\":[{{\"code\":\"W_AGENT_REGISTRY_UNAVAILABLE\",\"message\":\"agent registry is not available in this scaffold\"}}]}}"
            );
            Ok(())
        }
        OutputFormat::Ndjson => Err(ndjson_not_supported("agents")),
    }
}

fn agents_inspect(agent_id: &str) -> Result<(), CliError> {
    if agent_id.trim().is_empty() {
        return Err(CliError::new(
            "E_VALIDATION_FAILED",
            "agent id must not be empty",
            ExitKind::Validation,
        ));
    }

    Err(CliError::new(
        "E_NOT_FOUND",
        format!("agent '{agent_id}' was not found"),
        ExitKind::NotFound,
    ))
}

fn print_agents_help(output: OutputFormat) -> Result<(), CliError> {
    let help = "zornmesh agents\nList and inspect registered agents.\n\nUsage: zornmesh agents [COMMAND]\n\nCommands:\n  inspect <AGENT_ID>  Inspect one registered agent\n  help                Print agents help\n\nOptions:\n  -h, --help          Print help\n";
    print_help("agents help", help, output)
}

fn run_doctor(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => doctor(invocation),
        [flag] if is_help(flag) => print_doctor_help(invocation.output),
        [arg, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh doctor argument '{arg}'"),
            ExitKind::UserError,
        )),
    }
}

fn doctor(invocation: &Invocation) -> Result<(), CliError> {
    let daemon_state = daemon_state(&invocation.config.socket_path)?;
    match invocation.output {
        OutputFormat::Human => {
            println!("zornmesh doctor");
            println!("status: degraded");
            println!("daemon: {daemon_state}");
            println!("socket: {}", invocation.config.socket_path.display());
            println!(
                "socket_source: {}",
                invocation.config.socket_source.as_str()
            );
            println!("agent_registry: unavailable");
            if daemon_state == "unreachable" {
                println!(
                    "remediation: start the daemon with `zornmesh daemon --socket {}`",
                    invocation.config.socket_path.display()
                );
            }
            Ok(())
        }
        OutputFormat::Json => {
            println!(
                "{{\"schema_version\":\"{READ_SCHEMA_VERSION}\",\"command\":\"doctor\",\"status\":\"ok\",\"data\":{{\"health\":\"degraded\",\"daemon_state\":{},\"socket_path\":{},\"socket_source\":{},\"agent_registry\":\"unavailable\"}},\"warnings\":[{{\"code\":\"W_AGENT_REGISTRY_UNAVAILABLE\",\"message\":\"agent registry is not available in this scaffold\"}}]}}",
                json_string(daemon_state),
                json_string(&invocation.config.socket_path.display().to_string()),
                json_string(invocation.config.socket_source.as_str())
            );
            Ok(())
        }
        OutputFormat::Ndjson => Err(ndjson_not_supported("doctor")),
    }
}

fn print_doctor_help(output: OutputFormat) -> Result<(), CliError> {
    let help = "zornmesh doctor\nRun first-day local mesh diagnostics.\n\nUsage: zornmesh doctor [OPTIONS]\n\nOptions:\n      --output <FORMAT>  Select human, json, or ndjson output\n      --socket <PATH>    Override the local daemon socket path\n  -h, --help             Print help\n";
    print_help("doctor help", help, output)
}

fn run_trace(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_help("trace help", TRACE_HELP, invocation.output),
        [flag] if is_help(flag) => print_help("trace help", TRACE_HELP, invocation.output),
        [command] if command == "events" => trace_events(invocation.output),
        [command, rest @ ..] if command == "events" && rest.iter().any(|arg| is_help(arg)) => {
            print_help("trace help", TRACE_HELP, invocation.output)
        }
        [arg, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh trace argument '{arg}'"),
            ExitKind::UserError,
        )),
    }
}

fn trace_events(output: OutputFormat) -> Result<(), CliError> {
    match output {
        OutputFormat::Human => {
            println!("zornmesh trace events");
            println!("status: no_events");
            Ok(())
        }
        OutputFormat::Json | OutputFormat::Ndjson => {
            println!(
                "{{\"schema_version\":\"{EVENT_SCHEMA_VERSION}\",\"event_type\":\"trace.scaffolded\",\"sequence\":1,\"data\":{{\"status\":\"no_events\"}}}}"
            );
            Ok(())
        }
    }
}

fn ndjson_not_supported(command: &str) -> CliError {
    CliError::new(
        "E_UNSUPPORTED_OUTPUT_FORMAT",
        format!(
            "output format 'ndjson' is only supported for streaming read commands; command '{command}' supports human and json"
        ),
        ExitKind::UserError,
    )
}

fn cli_error_from_daemon(error: zornmesh_daemon::DaemonError) -> CliError {
    let kind = match error.code() {
        zornmesh_daemon::DaemonErrorCode::ExistingOwner => ExitKind::TemporaryUnavailable,
        zornmesh_daemon::DaemonErrorCode::LocalTrustUnsafe
        | zornmesh_daemon::DaemonErrorCode::ElevatedPrivilege => ExitKind::PermissionDenied,
        zornmesh_daemon::DaemonErrorCode::DaemonUnreachable => ExitKind::DaemonUnreachable,
        zornmesh_daemon::DaemonErrorCode::InvalidConfig => ExitKind::UserError,
        zornmesh_daemon::DaemonErrorCode::Io => ExitKind::Io,
    };
    CliError::new(error.code().as_str(), error.message(), kind)
}

fn is_help(arg: &str) -> bool {
    arg == "--help" || arg == "-h" || arg == "help"
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}
