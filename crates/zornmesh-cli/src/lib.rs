#![doc = "Command skeleton for the public zornmesh CLI."]

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    io::{self, BufRead, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use zornmesh_broker::SubjectPattern;
use zornmesh_store::{
    DeadLetterFailureCategory, DeadLetterQuery, EvidenceAuditEntry, EvidenceEnvelopeRecord,
    EvidenceQuery, EvidenceStateTransitionInput, EvidenceStore, FileEvidenceStore,
};

pub const ROOT_HELP: &str = include_str!("../../../fixtures/cli/root-help.stdout");
pub const DAEMON_HELP: &str = include_str!("../../../fixtures/cli/daemon-help.stdout");
pub const TRACE_HELP: &str = include_str!("../../../fixtures/cli/trace-help.stdout");
pub const TAIL_HELP: &str = include_str!("../../../fixtures/cli/tail-help.stdout");
pub const REPLAY_HELP: &str = "zornmesh replay\nRedeliver a previously sent envelope by message ID.\n\nUsage: zornmesh replay <MESSAGE_ID> [OPTIONS]\n\nOptions:\n      --evidence <PATH>            Read this evidence log\n      --preview                    Emit a preview without delivery side effect\n      --yes                        Confirm replay without preview\n      --confirmation-token <TOKEN> Confirm a previously previewed replay\n      --output <FORMAT>            Select human or json output\n  -h, --help                       Print help\n";
pub const VERSION: &str = "zornmesh 0.1.0\n";
const READ_SCHEMA_VERSION: &str = "zornmesh.cli.read.v1";
const EVENT_SCHEMA_VERSION: &str = "zornmesh.cli.event.v1";
const DOCTOR_SCHEMA_VERSION: &str = "zornmesh.cli.doctor.v1";
const CLI_VERSION: &str = "0.1.0";
pub const MCP_BRIDGE_PROTOCOL_VERSION: &str = "2025-03-26";
const DEFAULT_SHUTDOWN_BUDGET_MS: u64 = 10_000;
const MAX_SHUTDOWN_BUDGET_MS: u64 = 60_000;
const DEFAULT_INSPECT_LIMIT: usize = 50;
const MAX_INSPECT_LIMIT: usize = 100;
const SUPPORTED_SHELLS: &str = "bash, zsh, fish";
const ENV_EVIDENCE_PATH: &str = "ZORN_EVIDENCE_PATH";
static NEXT_BRIDGE_CORRELATION_ID: AtomicU64 = AtomicU64::new(1);
const BASH_COMPLETION: &str = r#"_zornmesh()
{
    local cur prev commands global_opts daemon_commands shells formats
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    commands="daemon doctor agents stdio inspect trace tail replay completion help"
    daemon_commands="status shutdown help"
    shells="bash zsh fish"
    formats="human json ndjson"
    global_opts="--config --socket --output --non-interactive --help -h --version -V"

    case "${prev}" in
        --output)
            COMPREPLY=( $(compgen -W "${formats}" -- "${cur}") )
            return 0
            ;;
        --config|--socket)
            COMPREPLY=( $(compgen -f -- "${cur}") )
            return 0
            ;;
        completion)
            COMPREPLY=( $(compgen -W "${shells}" -- "${cur}") )
            return 0
            ;;
    esac

    case "${COMP_WORDS[1]}" in
        daemon)
            COMPREPLY=( $(compgen -W "${daemon_commands} ${global_opts}" -- "${cur}") )
            ;;
        *)
            COMPREPLY=( $(compgen -W "${commands} ${global_opts}" -- "${cur}") )
            ;;
    esac
}
complete -F _zornmesh zornmesh
"#;
const ZSH_COMPLETION: &str = r#"#compdef zornmesh

_zornmesh() {
  local -a commands daemon_commands shells formats global_opts
  commands=(daemon doctor agents stdio inspect trace tail replay completion help)
  daemon_commands=(status shutdown help)
  shells=(bash zsh fish)
  formats=(human json ndjson)
  global_opts=(--config --socket --output --non-interactive --help -h --version -V)

  _arguments \
    '--config[Read CLI defaults from a key=value config file]:path:_files' \
    '--socket[Override the local daemon socket path]:path:_files' \
    '--output[Select human, json, or ndjson output]:format:($formats)' \
    '--non-interactive[Fail fast instead of prompting]' \
    '(-h --help)'{-h,--help}'[Print help]' \
    '(-V --version)'{-V,--version}'[Print version]' \
    '1:command:($commands)' \
    '2:daemon command:($daemon_commands)' \
    '2:shell:($shells)'
}

_zornmesh "$@"
"#;
const FISH_COMPLETION: &str = r#"complete -c zornmesh -f -a "daemon doctor agents stdio inspect trace tail replay completion help"
complete -c zornmesh -n "__fish_seen_subcommand_from daemon" -f -a "status shutdown help"
complete -c zornmesh -n "__fish_seen_subcommand_from completion" -f -a "bash zsh fish"
complete -c zornmesh -l config -d "Read CLI defaults from a key=value config file" -r
complete -c zornmesh -l socket -d "Set --socket path for the local daemon" -r
complete -c zornmesh -l output -d "Set --output to human, json, or ndjson" -x -a "human json ndjson"
complete -c zornmesh -l non-interactive -d "Use --non-interactive fail-fast mode"
complete -c zornmesh -s h -l help -d "Print help"
complete -c zornmesh -s V -l version -d "Print version"
"#;

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

#[derive(Debug, Clone)]
struct DiagnosticWarning {
    code: &'static str,
    message: String,
}

impl DiagnosticWarning {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
struct DaemonInspection {
    state: &'static str,
    socket_ownership: &'static str,
    socket_permissions: &'static str,
    trust_status: &'static str,
    trust_posture: &'static str,
    remediation: Option<String>,
    warnings: Vec<DiagnosticWarning>,
}

impl DaemonInspection {
    fn shutdown_status(&self) -> &'static str {
        if self.state == "ready" {
            "idle"
        } else {
            "unavailable"
        }
    }
}

#[derive(Debug, Clone)]
struct ShutdownCliReport {
    daemon_state: &'static str,
    outcome: &'static str,
    socket_path: String,
    shutdown_budget_ms: u64,
    in_flight_work: &'static str,
    remediation: Option<String>,
    warnings: Vec<DiagnosticWarning>,
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
        [command] if command == "stdio" => run_stdio(&[], &invocation),
        [command, rest @ ..] if command == "stdio" => run_stdio(rest, &invocation),
        [command] if command == "inspect" => run_inspect(&[], &invocation),
        [command, rest @ ..] if command == "inspect" => run_inspect(rest, &invocation),
        [command] if command == "doctor" => run_doctor(&[], &invocation),
        [command, rest @ ..] if command == "doctor" => run_doctor(rest, &invocation),
        [command] if command == "completion" => print_completion_help(invocation.output),
        [command, rest @ ..] if command == "completion" => run_completion(rest, &invocation),
        [command] if command == "trace" => print_help("trace help", TRACE_HELP, invocation.output),
        [command, rest @ ..] if command == "trace" => run_trace(rest, &invocation),
        [command] if command == "tail" => print_help("tail help", TAIL_HELP, invocation.output),
        [command, rest @ ..] if command == "tail" => run_tail(rest, &invocation),
        [command] if command == "replay" => print_help("replay help", REPLAY_HELP, invocation.output),
        [command, rest @ ..] if command == "replay" => run_replay(rest, &invocation),
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
                "{{\"schema_version\":\"{READ_SCHEMA_VERSION}\",\"command\":\"version\",\"status\":\"ok\",\"data\":{{\"version\":\"{CLI_VERSION}\"}},\"warnings\":[]}}"
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
        let report = shutdown_report(invocation)?;
        return print_shutdown_report(&report, invocation.output);
    }

    Err(CliError::new(
        "E_CONFIRMATION_REQUIRED",
        "daemon shutdown requires confirmation; rerun with --non-interactive to fail fast",
        ExitKind::UserError,
    ))
}

fn shutdown_report(invocation: &Invocation) -> Result<ShutdownCliReport, CliError> {
    let shutdown_budget_ms = configured_shutdown_budget_ms()?;
    let inspection = inspect_local_daemon(&invocation.config.socket_path)?;
    let socket_path = invocation.config.socket_path.display().to_string();

    if inspection.state != "ready" {
        return Ok(ShutdownCliReport {
            daemon_state: inspection.state,
            outcome: if inspection.state == "blocked" {
                "blocked"
            } else {
                "not-running"
            },
            socket_path,
            shutdown_budget_ms,
            in_flight_work: "unavailable",
            remediation: inspection.remediation,
            warnings: Vec::new(),
        });
    }

    let lock_path = daemon_lock_path(&invocation.config.socket_path);
    let pid = match read_daemon_pid(&lock_path) {
        Ok(Some(pid)) => pid,
        Ok(None) => {
            return Ok(ShutdownCliReport {
                daemon_state: "ready",
                outcome: "unverifiable",
                socket_path,
                shutdown_budget_ms,
                in_flight_work: "unavailable",
                remediation: Some(format!(
                    "daemon ownership lock is unavailable at {}; send SIGTERM to the daemon process if known",
                    lock_path.display()
                )),
                warnings: vec![DiagnosticWarning::new(
                    "W_SHUTDOWN_PID_UNAVAILABLE",
                    "daemon ownership lock did not provide a process id",
                )],
            });
        }
        Err(message) => {
            return Ok(ShutdownCliReport {
                daemon_state: "ready",
                outcome: "unverifiable",
                socket_path,
                shutdown_budget_ms,
                in_flight_work: "unavailable",
                remediation: Some(message.clone()),
                warnings: vec![DiagnosticWarning::new(
                    "W_SHUTDOWN_PID_UNVERIFIABLE",
                    message,
                )],
            });
        }
    };

    if let Err(message) = send_sigterm(pid) {
        return Ok(ShutdownCliReport {
            daemon_state: "ready",
            outcome: "unverifiable",
            socket_path,
            shutdown_budget_ms,
            in_flight_work: "unavailable",
            remediation: Some(message.clone()),
            warnings: vec![DiagnosticWarning::new(
                "W_SHUTDOWN_SIGNAL_UNAVAILABLE",
                message,
            )],
        });
    }

    let deadline = Instant::now() + Duration::from_millis(shutdown_budget_ms);
    while Instant::now() < deadline {
        if daemon_state(&invocation.config.socket_path)? != "ready" {
            return Ok(ShutdownCliReport {
                daemon_state: "draining",
                outcome: "clean",
                socket_path,
                shutdown_budget_ms,
                in_flight_work: "unavailable",
                remediation: None,
                warnings: Vec::new(),
            });
        }
        thread::sleep(Duration::from_millis(10));
    }

    Ok(ShutdownCliReport {
        daemon_state: "draining",
        outcome: "shutdown-budget-exceeded",
        socket_path,
        shutdown_budget_ms,
        in_flight_work: "unavailable",
        remediation: Some("daemon did not finish shutdown within the configured budget".to_owned()),
        warnings: vec![DiagnosticWarning::new(
            "W_SHUTDOWN_BUDGET_EXCEEDED",
            "daemon did not finish shutdown within the configured budget",
        )],
    })
}

fn print_shutdown_report(report: &ShutdownCliReport, output: OutputFormat) -> Result<(), CliError> {
    match output {
        OutputFormat::Human => {
            println!("zornmesh daemon shutdown");
            println!("state: {}", report.daemon_state);
            println!("outcome: {}", report.outcome);
            println!("socket: {}", report.socket_path);
            println!("shutdown_budget_ms: {}", report.shutdown_budget_ms);
            println!("in_flight_work: {}", report.in_flight_work);
            if let Some(remediation) = &report.remediation {
                println!("remediation: {remediation}");
            }
            Ok(())
        }
        OutputFormat::Json => {
            println!(
                "{{\"schema_version\":\"{READ_SCHEMA_VERSION}\",\"command\":\"daemon shutdown\",\"status\":\"ok\",\"data\":{{\"daemon_state\":{},\"outcome\":{},\"socket_path\":{},\"shutdown_budget_ms\":{},\"in_flight_work\":{},\"remediation\":{}}},\"warnings\":{}}}",
                json_string(report.daemon_state),
                json_string(report.outcome),
                json_string(&report.socket_path),
                report.shutdown_budget_ms,
                json_string(report.in_flight_work),
                json_optional_string(report.remediation.as_deref()),
                warnings_json(&report.warnings)
            );
            Ok(())
        }
        OutputFormat::Ndjson => Err(ndjson_not_supported("daemon shutdown")),
    }
}

fn configured_shutdown_budget_ms() -> Result<u64, CliError> {
    match std::env::var(zornmesh_rpc::local::ENV_SHUTDOWN_BUDGET_MS) {
        Ok(raw) => {
            let millis = raw.parse::<u64>().map_err(|error| {
                CliError::new(
                    "E_INVALID_CONFIG",
                    format!("ZORN_SHUTDOWN_BUDGET_MS must be milliseconds: {error}"),
                    ExitKind::UserError,
                )
            })?;
            Ok(millis.min(MAX_SHUTDOWN_BUDGET_MS))
        }
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_SHUTDOWN_BUDGET_MS),
        Err(std::env::VarError::NotUnicode(_)) => Err(CliError::new(
            "E_INVALID_CONFIG",
            "ZORN_SHUTDOWN_BUDGET_MS must be valid UTF-8",
            ExitKind::UserError,
        )),
    }
}

fn daemon_lock_path(socket_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", socket_path.display()))
}

fn read_daemon_pid(lock_path: &Path) -> Result<Option<u32>, String> {
    let raw = match fs::read_to_string(lock_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "failed to read daemon ownership lock '{}': {error}",
                lock_path.display()
            ));
        }
    };

    let pid = raw.trim().parse::<u32>().map_err(|error| {
        format!(
            "daemon ownership lock '{}' has invalid pid: {error}",
            lock_path.display()
        )
    })?;
    Ok(Some(pid))
}

fn send_sigterm(pid: u32) -> Result<(), String> {
    let output = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .output()
        .map_err(|error| format!("failed to invoke kill for daemon pid {pid}: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        Err(format!("failed to signal daemon pid {pid}"))
    } else {
        Err(format!("failed to signal daemon pid {pid}: {stderr}"))
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InspectCollection {
    Messages,
    DeadLetters,
    Audit,
    Metadata,
}

impl InspectCollection {
    fn parse(raw: &str) -> Result<Self, CliError> {
        match raw {
            "messages" => Ok(Self::Messages),
            "dead-letters" | "dead_letters" => Ok(Self::DeadLetters),
            "audit" | "audit-log" | "audit_log" => Ok(Self::Audit),
            "metadata" | "schema" | "version" | "sbom" => Ok(Self::Metadata),
            other => Err(CliError::new(
                "E_UNSUPPORTED_COMMAND",
                format!("unsupported zornmesh inspect collection '{other}'"),
                ExitKind::UserError,
            )),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::DeadLetters => "dead-letters",
            Self::Audit => "audit",
            Self::Metadata => "metadata",
        }
    }

    const fn empty_noun(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::DeadLetters => "dead letters",
            Self::Audit => "audit entries",
            Self::Metadata => "metadata records",
        }
    }

    fn command(self) -> String {
        format!("inspect {}", self.as_str())
    }
}

#[derive(Debug, Clone, Default)]
struct InspectFilters {
    correlation_id: Option<String>,
    trace_id: Option<String>,
    agent_id: Option<String>,
    subject: Option<String>,
    delivery_state: Option<String>,
    failure_category: Option<DeadLetterFailureCategory>,
    since_unix_ms: Option<u64>,
    until_unix_ms: Option<u64>,
}

impl InspectFilters {
    fn time_window(&self) -> Option<(u64, u64)> {
        match (self.since_unix_ms, self.until_unix_ms) {
            (Some(since), Some(until)) => Some((since, until)),
            (Some(since), None) => Some((since, u64::MAX)),
            (None, Some(until)) => Some((0, until)),
            (None, None) => None,
        }
    }

    fn chips(&self) -> Vec<InspectFilterChip> {
        let mut chips = Vec::new();
        push_filter_chip(&mut chips, "correlation_id", self.correlation_id.as_deref());
        push_filter_chip(&mut chips, "trace_id", self.trace_id.as_deref());
        push_filter_chip(&mut chips, "agent_id", self.agent_id.as_deref());
        push_filter_chip(&mut chips, "subject", self.subject.as_deref());
        push_filter_chip(&mut chips, "delivery_state", self.delivery_state.as_deref());
        if let Some(category) = self.failure_category {
            push_filter_chip(&mut chips, "failure_category", Some(category.as_str()));
        }
        if let Some(since) = self.since_unix_ms {
            chips.push(InspectFilterChip::new("since", since.to_string()));
        }
        if let Some(until) = self.until_unix_ms {
            chips.push(InspectFilterChip::new("until", until.to_string()));
        }
        chips
    }
}

#[derive(Debug, Clone)]
struct InspectFilterChip {
    key: &'static str,
    value: String,
    label: String,
}

impl InspectFilterChip {
    fn new(key: &'static str, value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            key,
            label: format!("{key}={value}"),
            value,
        }
    }
}

#[derive(Debug, Clone)]
struct InspectOptions {
    evidence_path: Option<PathBuf>,
    filters: InspectFilters,
    limit: usize,
    cursor: usize,
}

impl Default for InspectOptions {
    fn default() -> Self {
        Self {
            evidence_path: None,
            filters: InspectFilters::default(),
            limit: DEFAULT_INSPECT_LIMIT,
            cursor: 0,
        }
    }
}

fn run_inspect(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_inspect_help(invocation.output),
        [flag] if is_help(flag) => print_inspect_help(invocation.output),
        [collection, rest @ ..] if rest.iter().any(|arg| is_help(arg)) => {
            InspectCollection::parse(collection)?;
            print_inspect_help(invocation.output)
        }
        [collection, rest @ ..] => {
            let collection = InspectCollection::parse(collection)?;
            let options = parse_inspect_options(rest)?;
            inspect_collection(collection, options, invocation.output)
        }
    }
}

fn parse_inspect_options(args: &[String]) -> Result<InspectOptions, CliError> {
    let mut options = InspectOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if let Some(value) = arg.strip_prefix("--evidence=") {
            options.evidence_path = Some(parse_non_empty_path("--evidence", value)?);
            index += 1;
        } else if let Some(value) = arg.strip_prefix("--evidence-path=") {
            options.evidence_path = Some(parse_non_empty_path("--evidence-path", value)?);
            index += 1;
        } else if let Some(value) = arg.strip_prefix("--store=") {
            options.evidence_path = Some(parse_non_empty_path("--store", value)?);
            index += 1;
        } else if matches!(arg.as_str(), "--evidence" | "--evidence-path" | "--store") {
            let value = inspect_option_value(args, index, arg)?;
            options.evidence_path = Some(parse_non_empty_path(arg, value)?);
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--correlation-id=") {
            options.filters.correlation_id = Some(parse_non_empty_string(arg, value)?);
            index += 1;
        } else if arg == "--correlation-id" || arg == "--correlation" {
            let value = inspect_option_value(args, index, arg)?;
            options.filters.correlation_id = Some(parse_non_empty_string(arg, value)?);
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--trace-id=") {
            options.filters.trace_id = Some(parse_non_empty_string(arg, value)?);
            index += 1;
        } else if arg == "--trace-id" {
            let value = inspect_option_value(args, index, arg)?;
            options.filters.trace_id = Some(parse_non_empty_string(arg, value)?);
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--agent-id=") {
            options.filters.agent_id = Some(parse_non_empty_string(arg, value)?);
            index += 1;
        } else if arg == "--agent-id" {
            let value = inspect_option_value(args, index, arg)?;
            options.filters.agent_id = Some(parse_non_empty_string(arg, value)?);
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--subject=") {
            options.filters.subject = Some(parse_non_empty_string(arg, value)?);
            index += 1;
        } else if arg == "--subject" {
            let value = inspect_option_value(args, index, arg)?;
            options.filters.subject = Some(parse_non_empty_string(arg, value)?);
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--delivery-state=") {
            options.filters.delivery_state = Some(parse_non_empty_string(arg, value)?);
            index += 1;
        } else if let Some(value) = arg.strip_prefix("--state=") {
            options.filters.delivery_state = Some(parse_non_empty_string(arg, value)?);
            index += 1;
        } else if arg == "--delivery-state" || arg == "--state" {
            let value = inspect_option_value(args, index, arg)?;
            options.filters.delivery_state = Some(parse_non_empty_string(arg, value)?);
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--failure-category=") {
            options.filters.failure_category = Some(parse_failure_category(value)?);
            index += 1;
        } else if arg == "--failure-category" {
            let value = inspect_option_value(args, index, arg)?;
            options.filters.failure_category = Some(parse_failure_category(value)?);
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--since=") {
            options.filters.since_unix_ms = Some(parse_u64_option("--since", value)?);
            index += 1;
        } else if arg == "--since" {
            let value = inspect_option_value(args, index, arg)?;
            options.filters.since_unix_ms = Some(parse_u64_option(arg, value)?);
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--until=") {
            options.filters.until_unix_ms = Some(parse_u64_option("--until", value)?);
            index += 1;
        } else if arg == "--until" {
            let value = inspect_option_value(args, index, arg)?;
            options.filters.until_unix_ms = Some(parse_u64_option(arg, value)?);
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--limit=") {
            options.limit = parse_limit(value)?;
            index += 1;
        } else if arg == "--limit" {
            let value = inspect_option_value(args, index, arg)?;
            options.limit = parse_limit(value)?;
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--cursor=") {
            options.cursor = parse_cursor(value)?;
            index += 1;
        } else if arg == "--cursor" {
            let value = inspect_option_value(args, index, arg)?;
            options.cursor = parse_cursor(value)?;
            index += 2;
        } else {
            return Err(CliError::new(
                "E_UNSUPPORTED_COMMAND",
                format!("unsupported zornmesh inspect argument '{arg}'"),
                ExitKind::UserError,
            ));
        }
    }

    if let Some((since, until)) = options.filters.time_window()
        && since > until
    {
        return Err(CliError::new(
            "E_VALIDATION_FAILED",
            format!("inspect time window is invalid: since {since} is after until {until}"),
            ExitKind::Validation,
        ));
    }

    Ok(options)
}

fn inspect_option_value<'a>(
    args: &'a [String],
    index: usize,
    option: &str,
) -> Result<&'a str, CliError> {
    args.get(index + 1).map(String::as_str).ok_or_else(|| {
        CliError::new(
            "E_VALIDATION_FAILED",
            format!("{option} requires a value"),
            ExitKind::Validation,
        )
    })
}

fn parse_non_empty_path(option: &str, value: &str) -> Result<PathBuf, CliError> {
    Ok(PathBuf::from(parse_non_empty_string(option, value)?))
}

fn parse_non_empty_string(option: &str, value: &str) -> Result<String, CliError> {
    if value.trim().is_empty() {
        return Err(CliError::new(
            "E_VALIDATION_FAILED",
            format!("{option} requires a non-empty value"),
            ExitKind::Validation,
        ));
    }
    Ok(value.to_owned())
}

fn parse_u64_option(option: &str, value: &str) -> Result<u64, CliError> {
    value.parse::<u64>().map_err(|error| {
        CliError::new(
            "E_VALIDATION_FAILED",
            format!("{option} must be an unsigned integer millisecond timestamp: {error}"),
            ExitKind::Validation,
        )
    })
}

fn parse_limit(value: &str) -> Result<usize, CliError> {
    let limit = value.parse::<usize>().map_err(|error| {
        CliError::new(
            "E_VALIDATION_FAILED",
            format!("inspect limit must be an integer: {error}"),
            ExitKind::Validation,
        )
    })?;
    if limit == 0 {
        return Err(CliError::new(
            "E_VALIDATION_FAILED",
            "inspect limit must be greater than zero",
            ExitKind::Validation,
        ));
    }
    if limit > MAX_INSPECT_LIMIT {
        return Err(CliError::new(
            "E_VALIDATION_FAILED",
            format!("inspect limit {limit} exceeds maximum {MAX_INSPECT_LIMIT}"),
            ExitKind::Validation,
        ));
    }
    Ok(limit)
}

fn parse_cursor(value: &str) -> Result<usize, CliError> {
    value.parse::<usize>().map_err(|error| {
        CliError::new(
            "E_VALIDATION_FAILED",
            format!("inspect cursor must be an integer offset: {error}"),
            ExitKind::Validation,
        )
    })
}

fn parse_failure_category(value: &str) -> Result<DeadLetterFailureCategory, CliError> {
    DeadLetterFailureCategory::from_wire(value).ok_or_else(|| {
        CliError::new(
            "E_VALIDATION_FAILED",
            format!("unsupported failure category '{value}'"),
            ExitKind::Validation,
        )
    })
}

fn inspect_collection(
    collection: InspectCollection,
    options: InspectOptions,
    output: OutputFormat,
) -> Result<(), CliError> {
    let evidence_path = resolve_evidence_path(options.evidence_path.as_ref())?;
    let chips = options.filters.chips();
    let mut warnings = Vec::new();
    let (availability, mut records, metadata) = match evidence_path.as_ref() {
        Some(path) => match fs::metadata(path) {
            Ok(_) => match FileEvidenceStore::open_evidence(path) {
                Ok(store) => (
                    "available",
                    inspect_records(collection, &store, &options.filters),
                    inspect_metadata_json("available", Some(path), Some(&store), None),
                ),
                Err(error) => {
                    let message = format!("evidence store is unavailable: {error}");
                    warnings.push(DiagnosticWarning::new(
                        "W_EVIDENCE_STORE_UNAVAILABLE",
                        message.clone(),
                    ));
                    (
                        "unavailable",
                        Vec::new(),
                        inspect_metadata_json("unavailable", Some(path), None, Some(&message)),
                    )
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let message = "evidence store does not exist".to_owned();
                warnings.push(DiagnosticWarning::new(
                    "W_EVIDENCE_STORE_UNAVAILABLE",
                    message.clone(),
                ));
                (
                    "unavailable",
                    Vec::new(),
                    inspect_metadata_json("unavailable", Some(path), None, Some(&message)),
                )
            }
            Err(error) => {
                let message = format!("evidence store cannot be inspected: {error}");
                warnings.push(DiagnosticWarning::new(
                    "W_EVIDENCE_STORE_UNAVAILABLE",
                    message.clone(),
                ));
                (
                    "unavailable",
                    Vec::new(),
                    inspect_metadata_json("unavailable", Some(path), None, Some(&message)),
                )
            }
        },
        None => {
            let message = format!(
                "evidence store path is not configured; pass --evidence <PATH> or set {ENV_EVIDENCE_PATH}"
            );
            warnings.push(DiagnosticWarning::new(
                "W_EVIDENCE_STORE_UNAVAILABLE",
                message.clone(),
            ));
            (
                "unavailable",
                Vec::new(),
                inspect_metadata_json("unavailable", None, None, Some(&message)),
            )
        }
    };

    if collection == InspectCollection::Metadata && availability == "available" {
        records.push(serde_json::json!({
            "kind": "schema",
            "status": "available",
            "schema_version": zornmesh_store::EVIDENCE_STORE_SCHEMA_VERSION,
        }));
        records.push(serde_json::json!({
            "kind": "release_integrity",
            "status": "unsupported_placeholder",
            "signature": "unverifiable",
            "sbom": "unavailable",
        }));
    }

    let total = records.len();
    let page_records = records
        .into_iter()
        .skip(options.cursor)
        .take(options.limit)
        .collect::<Vec<_>>();
    let returned = page_records.len();
    let next_offset = options.cursor.saturating_add(returned);
    let next_cursor = if next_offset < total {
        Some(next_offset.to_string())
    } else {
        None
    };
    let state = inspect_state(availability, total, next_cursor.as_deref());
    let empty_message = if state == "empty" {
        Some(format!(
            "no {} matched the inspect filters",
            collection.empty_noun()
        ))
    } else if state == "unavailable" {
        Some("inspect data is unavailable".to_owned())
    } else {
        None
    };
    let next_actions = inspect_next_actions(state);

    match output {
        OutputFormat::Human => print_inspect_human(InspectHumanReport {
            collection,
            availability,
            state,
            returned,
            total,
            limit: options.limit,
            next_cursor: next_cursor.as_deref(),
            chips: &chips,
            records: &page_records,
            empty_message: empty_message.as_deref(),
            next_actions: &next_actions,
            warnings: &warnings,
        }),
        OutputFormat::Json => {
            let data = serde_json::json!({
                "collection": collection.as_str(),
                "availability": availability,
                "state": state,
                "records": page_records,
                "filters": chips.iter().map(filter_chip_json).collect::<Vec<_>>(),
                "pagination": {
                    "cursor": options.cursor.to_string(),
                    "limit": options.limit,
                    "default_limit": DEFAULT_INSPECT_LIMIT,
                    "max_limit": MAX_INSPECT_LIMIT,
                    "returned": returned,
                    "total": total,
                    "next_cursor": next_cursor,
                    "complete": next_offset >= total,
                },
                "metadata": metadata,
                "empty": empty_message,
                "next_actions": next_actions,
            });
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": READ_SCHEMA_VERSION,
                    "command": collection.command(),
                    "status": "ok",
                    "data": data,
                    "warnings": warnings.iter().map(warning_json).collect::<Vec<_>>(),
                })
            );
            Ok(())
        }
        OutputFormat::Ndjson => Err(ndjson_not_supported(&collection.command())),
    }
}

fn resolve_evidence_path(cli_path: Option<&PathBuf>) -> Result<Option<PathBuf>, CliError> {
    if let Some(path) = cli_path {
        return Ok(Some(path.clone()));
    }
    match std::env::var(ENV_EVIDENCE_PATH) {
        Ok(raw) if raw.trim().is_empty() => Err(CliError::new(
            "E_INVALID_CONFIG",
            format!("{ENV_EVIDENCE_PATH} must not be empty"),
            ExitKind::UserError,
        )),
        Ok(raw) => Ok(Some(PathBuf::from(raw))),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(CliError::new(
            "E_INVALID_CONFIG",
            format!("{ENV_EVIDENCE_PATH} must be valid UTF-8"),
            ExitKind::UserError,
        )),
    }
}

fn inspect_records(
    collection: InspectCollection,
    store: &FileEvidenceStore,
    filters: &InspectFilters,
) -> Vec<serde_json::Value> {
    match collection {
        InspectCollection::Messages => inspect_message_records(store, filters),
        InspectCollection::DeadLetters => inspect_dead_letter_records(store, filters),
        InspectCollection::Audit => inspect_audit_records(store, filters),
        InspectCollection::Metadata => Vec::new(),
    }
}

fn inspect_message_records(
    store: &FileEvidenceStore,
    filters: &InspectFilters,
) -> Vec<serde_json::Value> {
    let mut query = EvidenceQuery::new();
    if let Some(value) = filters.correlation_id.as_deref() {
        query = query.correlation_id(value);
    }
    if let Some(value) = filters.trace_id.as_deref() {
        query = query.trace_id(value);
    }
    if let Some(value) = filters.agent_id.as_deref() {
        query = query.agent_id(value);
    }
    if let Some(value) = filters.subject.as_deref() {
        query = query.subject(value);
    }
    if let Some(value) = filters.delivery_state.as_deref() {
        query = query.delivery_state(value);
    }
    if let Some((since, until)) = filters.time_window() {
        query = query.time_window(since, until);
    }

    let mut records = store.query_envelopes(query);
    records.sort_by(|left, right| {
        (left.daemon_sequence(), left.message_id())
            .cmp(&(right.daemon_sequence(), right.message_id()))
    });
    records.iter().map(message_record_json).collect()
}

fn inspect_dead_letter_records(
    store: &FileEvidenceStore,
    filters: &InspectFilters,
) -> Vec<serde_json::Value> {
    let mut query = DeadLetterQuery::new();
    if let Some(value) = filters.correlation_id.as_deref() {
        query = query.correlation_id(value);
    }
    if let Some(value) = filters.trace_id.as_deref() {
        query = query.trace_id(value);
    }
    if let Some(value) = filters.agent_id.as_deref() {
        query = query.agent_id(value);
    }
    if let Some(value) = filters.subject.as_deref() {
        query = query.subject(value);
    }
    if let Some(value) = filters.failure_category {
        query = query.failure_category(value);
    }
    if let Some((since, until)) = filters.time_window() {
        query = query.time_window(since, until);
    }

    let mut records = store.query_dead_letters(query);
    if let Some(state) = filters.delivery_state.as_deref() {
        records.retain(|record| record.terminal_state() == state);
    }
    records.sort_by(|left, right| {
        (left.daemon_sequence(), left.message_id())
            .cmp(&(right.daemon_sequence(), right.message_id()))
    });
    records
        .iter()
        .map(|record| {
            serde_json::json!({
                "daemon_sequence": record.daemon_sequence(),
                "message_id": record.message_id(),
                "source_agent": record.source_agent(),
                "intended_target": record.intended_target(),
                "subject": record.subject(),
                "correlation_id": record.correlation_id(),
                "trace_id": record.trace_id(),
                "terminal_state": record.terminal_state(),
                "failure_category": record.failure_category().as_str(),
                "safe_details": record.safe_details(),
                "attempt_count": record.attempt_count(),
                "last_failure_category": record.last_failure_category().as_str(),
                "first_attempted_unix_ms": record.first_attempted_unix_ms(),
                "last_attempted_unix_ms": record.last_attempted_unix_ms(),
                "terminal_unix_ms": record.terminal_unix_ms(),
                "payload_len": record.payload_len(),
                "payload_content_type": record.payload_content_type(),
            })
        })
        .collect()
}

fn inspect_audit_records(
    store: &FileEvidenceStore,
    filters: &InspectFilters,
) -> Vec<serde_json::Value> {
    let timestamps = store
        .query_envelopes(EvidenceQuery::new())
        .into_iter()
        .map(|record| (record.message_id().to_owned(), record.timestamp_unix_ms()))
        .collect::<HashMap<_, _>>();
    let mut entries = store
        .audit_entries()
        .into_iter()
        .filter(|entry| audit_matches_filters(entry, filters, &timestamps))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        (left.daemon_sequence(), left.message_id(), left.action()).cmp(&(
            right.daemon_sequence(),
            right.message_id(),
            right.action(),
        ))
    });
    entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "daemon_sequence": entry.daemon_sequence(),
                "message_id": entry.message_id(),
                "timestamp_unix_ms": timestamps.get(entry.message_id()).copied(),
                "previous_audit_hash": entry.previous_audit_hash(),
                "current_audit_hash": entry.current_audit_hash(),
                "actor": entry.actor(),
                "action": entry.action(),
                "capability_or_subject": entry.capability_or_subject(),
                "correlation_id": entry.correlation_id(),
                "trace_id": entry.trace_id(),
                "state_from": entry.state_from(),
                "state_to": entry.state_to(),
                "outcome_details": entry.outcome_details(),
            })
        })
        .collect()
}

fn audit_matches_filters(
    entry: &EvidenceAuditEntry,
    filters: &InspectFilters,
    timestamps: &HashMap<String, u64>,
) -> bool {
    if filters
        .correlation_id
        .as_deref()
        .is_some_and(|value| entry.correlation_id() != value)
    {
        return false;
    }
    if filters
        .trace_id
        .as_deref()
        .is_some_and(|value| entry.trace_id() != value)
    {
        return false;
    }
    if filters
        .agent_id
        .as_deref()
        .is_some_and(|value| entry.actor() != value)
    {
        return false;
    }
    if filters
        .subject
        .as_deref()
        .is_some_and(|value| entry.capability_or_subject() != value)
    {
        return false;
    }
    if filters
        .delivery_state
        .as_deref()
        .is_some_and(|value| entry.state_to() != value)
    {
        return false;
    }
    if let Some((since, until)) = filters.time_window() {
        let Some(timestamp) = timestamps.get(entry.message_id()).copied() else {
            return false;
        };
        if timestamp < since || timestamp > until {
            return false;
        }
    }
    true
}

fn message_record_json(record: &EvidenceEnvelopeRecord) -> serde_json::Value {
    serde_json::json!({
        "daemon_sequence": record.daemon_sequence(),
        "message_id": record.message_id(),
        "source_agent": record.source_agent(),
        "target_or_subject": record.target_or_subject(),
        "subject": record.subject(),
        "timestamp_unix_ms": record.timestamp_unix_ms(),
        "correlation_id": record.correlation_id(),
        "trace_id": record.trace_id(),
        "span_id": record.span_id(),
        "parent_message_id": record.parent_message_id(),
        "delivery_state": record.delivery_state(),
        "payload_len": record.payload_len(),
        "payload_content_type": record.payload_content_type(),
    })
}

fn inspect_metadata_json(
    status: &'static str,
    path: Option<&Path>,
    store: Option<&FileEvidenceStore>,
    reason: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "evidence_store": {
            "status": status,
            "path": path.map(|path| path.display().to_string()),
            "schema_version": store.map(|_| zornmesh_store::EVIDENCE_STORE_SCHEMA_VERSION),
            "indexes": store.map(EvidenceStore::index_names).unwrap_or_default(),
            "reason": reason,
        },
        "runtime": {
            "status": "unsupported_placeholder",
            "reason": "daemon runtime metadata requires a live daemon inspect API",
        },
        "release_integrity": {
            "status": "unsupported_placeholder",
            "signature": "unverifiable",
            "sbom": "unavailable",
        },
    })
}

fn inspect_state(availability: &str, total: usize, next_cursor: Option<&str>) -> &'static str {
    if availability != "available" {
        "unavailable"
    } else if total == 0 {
        "empty"
    } else if next_cursor.is_some() {
        "partial"
    } else {
        "complete"
    }
}

fn inspect_next_actions(state: &str) -> Vec<&'static str> {
    match state {
        "empty" => vec!["trace", "tail", "doctor", "retention checks"],
        "unavailable" => vec!["doctor", "configure evidence store", "retention checks"],
        _ => Vec::new(),
    }
}

struct InspectHumanReport<'a> {
    collection: InspectCollection,
    availability: &'static str,
    state: &'static str,
    returned: usize,
    total: usize,
    limit: usize,
    next_cursor: Option<&'a str>,
    chips: &'a [InspectFilterChip],
    records: &'a [serde_json::Value],
    empty_message: Option<&'a str>,
    next_actions: &'a [&'static str],
    warnings: &'a [DiagnosticWarning],
}

fn print_inspect_human(report: InspectHumanReport<'_>) -> Result<(), CliError> {
    println!("zornmesh inspect {}", report.collection.as_str());
    println!("status: {}", report.availability);
    println!("state: {}", report.state);
    println!("records: {}", report.returned);
    if !report.chips.is_empty() {
        println!("filters: {}", filter_summary(report.chips));
    }
    if let Some(message) = report.empty_message {
        println!("empty: {message}");
    }
    for warning in report.warnings {
        println!("warning: {}", warning.message);
    }
    for record in report.records {
        println!("record: {}", inspect_human_record_summary(record));
    }
    if !report.next_actions.is_empty() {
        println!("next_actions: {}", report.next_actions.join(", "));
    }
    if let Some(cursor) = report.next_cursor {
        println!(
            "pagination: next_cursor={cursor} limit={} total={}",
            report.limit, report.total
        );
    } else {
        println!("pagination: complete");
    }
    Ok(())
}

fn inspect_human_record_summary(record: &serde_json::Value) -> String {
    let sequence = record
        .get("daemon_sequence")
        .and_then(serde_json::Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unavailable".to_owned());
    let message_id = record
        .get("message_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("metadata");
    let state = record
        .get("delivery_state")
        .or_else(|| record.get("terminal_state"))
        .or_else(|| record.get("state_to"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("metadata");
    format!("sequence={sequence} message_id={message_id} state={state}")
}

fn filter_summary(chips: &[InspectFilterChip]) -> String {
    chips
        .iter()
        .map(|chip| chip.label.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn filter_chip_json(chip: &InspectFilterChip) -> serde_json::Value {
    serde_json::json!({
        "key": chip.key,
        "value": chip.value,
        "label": chip.label,
    })
}

fn warning_json(warning: &DiagnosticWarning) -> serde_json::Value {
    serde_json::json!({
        "code": warning.code,
        "message": warning.message,
    })
}

fn push_filter_chip(chips: &mut Vec<InspectFilterChip>, key: &'static str, value: Option<&str>) {
    if let Some(value) = value {
        chips.push(InspectFilterChip::new(key, value));
    }
}

fn print_inspect_help(output: OutputFormat) -> Result<(), CliError> {
    let help = "zornmesh inspect\nInspect persisted messages, dead letters, audit entries, and metadata.\n\nUsage: zornmesh inspect <messages|dead-letters|audit|metadata> [OPTIONS]\n\nOptions:\n      --evidence <PATH>           Read this evidence log\n      --correlation-id <ID>       Filter by correlation ID\n      --trace-id <ID>             Filter by trace ID\n      --agent-id <ID>             Filter by source, target, or actor agent ID\n      --subject <SUBJECT>         Filter by subject or audit capability/subject\n      --delivery-state <STATE>    Filter by delivery or terminal state\n      --failure-category <CAT>    Filter dead letters by failure category\n      --since <UNIX_MS>           Include records at or after this timestamp\n      --until <UNIX_MS>           Include records at or before this timestamp\n      --limit <N>                 Page size, default 50 and maximum 100\n      --cursor <OFFSET>           Stable offset cursor from a previous page\n  -h, --help                      Print help\n";
    print_help("inspect", help, output)
}

fn run_stdio(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [flag] if is_help(flag) => print_stdio_help(invocation.output),
        [flag, agent_id] if flag == "--as-agent" => {
            if agent_id.trim().is_empty() {
                return Err(CliError::new(
                    "E_VALIDATION_FAILED",
                    "stdio --as-agent requires a non-empty agent id",
                    ExitKind::Validation,
                ));
            }
            if invocation.output != OutputFormat::Human {
                return Err(CliError::new(
                    "E_UNSUPPORTED_OUTPUT_FORMAT",
                    "stdio bridge writes MCP JSON-RPC to stdout and does not support --output",
                    ExitKind::UserError,
                ));
            }
            stdio_agent_session(agent_id, invocation)
        }
        [flag] if flag == "--as-agent" => Err(CliError::new(
            "E_VALIDATION_FAILED",
            "stdio --as-agent requires an agent id",
            ExitKind::Validation,
        )),
        [] => Err(CliError::new(
            "E_VALIDATION_FAILED",
            "stdio requires --as-agent <id>",
            ExitKind::Validation,
        )),
        [arg, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh stdio argument '{arg}'"),
            ExitKind::UserError,
        )),
    }
}

fn print_stdio_help(output: OutputFormat) -> Result<(), CliError> {
    let help = "zornmesh stdio\nBridge an MCP-compatible host into the local mesh over stdio.\n\nUsage: zornmesh stdio --as-agent <AGENT_ID>\n\nOptions:\n      --as-agent <AGENT_ID>  Register the MCP host as this mesh agent\n  -h, --help                Print help\n";
    print_help("stdio help", help, output)
}

fn stdio_agent_session(agent_id: &str, invocation: &Invocation) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let (uid, _daemon) = match connect_stdio_daemon(&invocation.config.socket_path) {
        Ok(connected) => connected,
        Err(error) => {
            writeln!(
                stdout,
                "{}",
                mcp_error_json(&serde_json::Value::Null, &error)
            )
            .map_err(stdio_io_error)?;
            return Ok(());
        }
    };

    let broker = zornmesh_broker::Broker::new();
    let credentials = zornmesh_broker::PeerCredentials::new(uid, uid, std::process::id());
    let policy = zornmesh_broker::SocketTrustPolicy::new(uid, uid, 0o600);
    let mut bridge = StdioBridge::new(broker, agent_id, "MCP Host", credentials, policy);
    let stdin = io::stdin();
    run_stdio_loop(stdin.lock(), stdout, &mut bridge).map_err(stdio_io_error)
}

fn connect_stdio_daemon(socket_path: &Path) -> Result<(u32, UnixStream), StdioBridgeError> {
    let uid = zornmesh_rpc::local::effective_uid().map_err(stdio_daemon_error_from_local)?;
    let stream = zornmesh_rpc::local::connect_trusted_socket(socket_path, uid)
        .map_err(stdio_daemon_error_from_local)?;
    Ok((uid, stream))
}

fn stdio_daemon_error_from_local(error: zornmesh_rpc::local::LocalError) -> StdioBridgeError {
    StdioBridgeError::new(
        StdioBridgeErrorCode::DaemonUnavailable,
        format!("daemon connection failed with {}", error.code().as_str()),
    )
}

fn stdio_io_error(error: io::Error) -> CliError {
    CliError::new(
        "E_DAEMON_IO",
        format!("stdio bridge I/O failed: {error}"),
        ExitKind::Io,
    )
}

fn run_stdio_loop<R, W>(reader: R, mut writer: W, bridge: &mut StdioBridge) -> io::Result<()>
where
    R: BufRead,
    W: Write,
{
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_jsonrpc_line(bridge, &line);
        writeln!(writer, "{response}")?;
    }
    let _ = bridge.handle_message(BridgeMessage::HostClosed);
    Ok(())
}

fn handle_jsonrpc_line(bridge: &mut StdioBridge, line: &str) -> String {
    let raw: serde_json::Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => {
            return mcp_error_json(
                &serde_json::Value::Null,
                &StdioBridgeError::new(
                    StdioBridgeErrorCode::MalformedMessage,
                    "malformed MCP JSON-RPC input",
                ),
            );
        }
    };
    let id = raw.get("id").cloned().unwrap_or(serde_json::Value::Null);
    let Some(method) = raw.get("method").and_then(serde_json::Value::as_str) else {
        return mcp_error_json(
            &id,
            &StdioBridgeError::new(
                StdioBridgeErrorCode::MalformedMessage,
                "MCP JSON-RPC request must include a method",
            ),
        );
    };
    let message = if method == "initialize" {
        let protocol_version = raw
            .get("params")
            .and_then(|params| params.get("protocolVersion"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        BridgeMessage::Initialize { protocol_version }
    } else {
        let params = raw
            .get("params")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}))
            .to_string();
        BridgeMessage::Request {
            method: method.to_owned(),
            params,
        }
    };
    bridge_response_json(&id, &bridge.handle_message(message))
}

fn bridge_response_json(id: &serde_json::Value, response: &BridgeResponse) -> String {
    match response {
        BridgeResponse::InitializeAck { protocol_version } => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": protocol_version,
                "serverInfo": {
                    "name": "zornmesh-stdio",
                    "version": CLI_VERSION,
                },
                "capabilities": {
                    "tools": {},
                },
            },
        })
        .to_string(),
        BridgeResponse::ToolList { tools } => {
            let tools = tools.iter().map(baseline_mcp_tool_json).collect::<Vec<_>>();
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": tools,
                },
            })
            .to_string()
        }
        BridgeResponse::Mapped {
            correlation_id,
            trace_id,
            capability_id,
            capability_version,
            internal_operation,
            safe_params,
        } => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "status": "mapped",
                "correlation_id": correlation_id,
                "trace_id": trace_id,
                "capability_id": capability_id,
                "capability_version": capability_version,
                "internal_operation": internal_operation,
                "safe_params": safe_params,
            },
        })
        .to_string(),
        BridgeResponse::UnsupportedCapability {
            code,
            capability_id,
            capability_version,
            reason,
            remediation,
            safe_params,
        } => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "status": "unsupported_capability",
                "code": code,
                "capability_id": capability_id,
                "capability_version": capability_version,
                "reason": reason,
                "remediation": remediation,
                "safe_params": safe_params,
            },
        })
        .to_string(),
        BridgeResponse::Error(error) => mcp_error_json(id, error),
        BridgeResponse::Closed => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "status": "closed",
            },
        })
        .to_string(),
    }
}

fn mcp_error_json(id: &serde_json::Value, error: &StdioBridgeError) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": error.code().jsonrpc_code(),
            "message": error.safe_message(),
            "data": {
                "code": error.code().as_str(),
                "retryable": error.code().retryable(),
            },
        },
    })
    .to_string()
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
    let report = inspect_local_daemon(&invocation.config.socket_path)?;
    let remediation = report.remediation.as_deref();
    match invocation.output {
        OutputFormat::Human => {
            println!("zornmesh doctor");
            println!("status: degraded");
            println!("daemon: {}", report.state);
            println!("version: {CLI_VERSION}");
            println!("socket: {}", invocation.config.socket_path.display());
            println!(
                "socket_source: {}",
                invocation.config.socket_source.as_str()
            );
            println!("socket_ownership: {}", report.socket_ownership);
            println!("socket_permissions: {}", report.socket_permissions);
            println!("schema: available ({DOCTOR_SCHEMA_VERSION})");
            println!("otel: unavailable");
            println!("signature: unverifiable");
            println!("sbom: unavailable");
            println!("trust: {}", report.trust_status);
            println!("shutdown: {}", report.shutdown_status());
            if let Some(remediation) = remediation {
                println!("remediation: {remediation}");
            }
            Ok(())
        }
        OutputFormat::Json => {
            println!(
                "{{\"schema_version\":\"{READ_SCHEMA_VERSION}\",\"command\":\"doctor\",\"status\":\"ok\",\"data\":{{\"health\":\"degraded\",\"diagnostics_schema\":\"{DOCTOR_SCHEMA_VERSION}\",\"daemon\":{{\"status\":{},\"version\":\"{CLI_VERSION}\",\"socket_path\":{},\"socket_source\":{},\"remediation\":{}}},\"socket\":{{\"ownership\":{},\"permissions\":{}}},\"schema\":{{\"status\":\"available\",\"version\":\"{DOCTOR_SCHEMA_VERSION}\"}},\"otel\":{{\"status\":\"unavailable\",\"endpoint\":\"unconfigured\"}},\"signature\":{{\"status\":\"unverifiable\",\"identity\":\"unavailable\"}},\"sbom\":{{\"status\":\"unavailable\",\"identity\":\"unavailable\"}},\"trust\":{{\"status\":{},\"posture\":{}}},\"shutdown\":{{\"status\":{},\"in_flight_work\":\"unavailable\"}}}},\"warnings\":{}}}",
                json_string(report.state),
                json_string(&invocation.config.socket_path.display().to_string()),
                json_string(invocation.config.socket_source.as_str()),
                json_optional_string(remediation),
                json_string(report.socket_ownership),
                json_string(report.socket_permissions),
                json_string(report.trust_status),
                json_string(report.trust_posture),
                json_string(report.shutdown_status()),
                warnings_json(&report.warnings)
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

fn inspect_local_daemon(socket_path: &Path) -> Result<DaemonInspection, CliError> {
    let current_uid = zornmesh_rpc::local::effective_uid().map_err(cli_error_from_local)?;
    let start_remediation = format!(
        "start the daemon with `zornmesh daemon --socket {}`",
        socket_path.display()
    );
    let repair_remediation =
        "repair local daemon socket ownership and permissions, or remove the unsafe socket path"
            .to_owned();

    let metadata = match fs::symlink_metadata(socket_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut warnings = vec![DiagnosticWarning::new(
                "W_DAEMON_UNREACHABLE",
                "daemon is unreachable; start the daemon or choose another socket",
            )];
            warnings.extend(missing_evidence_warnings());
            return Ok(DaemonInspection {
                state: "unreachable",
                socket_ownership: "unavailable",
                socket_permissions: "unavailable",
                trust_status: "degraded",
                trust_posture: "daemon-unreachable",
                remediation: Some(start_remediation),
                warnings,
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            let mut warnings = vec![DiagnosticWarning::new(
                "W_LOCAL_TRUST_UNSAFE",
                "daemon socket cannot be inspected safely by the current user",
            )];
            warnings.extend(missing_evidence_warnings());
            return Ok(DaemonInspection {
                state: "blocked",
                socket_ownership: "unverifiable",
                socket_permissions: "unverifiable",
                trust_status: "unsafe",
                trust_posture: "socket-inspection-blocked",
                remediation: Some(repair_remediation),
                warnings,
            });
        }
        Err(error) => {
            return Err(CliError::new(
                "E_DAEMON_IO",
                format!(
                    "failed to inspect daemon socket '{}': {error}",
                    socket_path.display()
                ),
                ExitKind::Io,
            ));
        }
    };

    let socket_ownership = if metadata.uid() == current_uid {
        "current-user"
    } else {
        "other-user"
    };
    let socket_permissions = if metadata.permissions().mode() & 0o077 == 0 {
        "private"
    } else {
        "unsafe"
    };
    let parent_is_trusted = socket_path
        .parent()
        .and_then(|parent| fs::symlink_metadata(parent).ok())
        .is_some_and(|parent| {
            parent.uid() == current_uid && parent.permissions().mode() & 0o077 == 0
        });
    let socket_is_trusted = metadata.file_type().is_socket()
        && socket_ownership == "current-user"
        && socket_permissions == "private"
        && parent_is_trusted;

    if !socket_is_trusted {
        let mut warnings = vec![DiagnosticWarning::new(
            "W_LOCAL_TRUST_UNSAFE",
            "daemon socket is not private to the current user",
        )];
        warnings.extend(missing_evidence_warnings());
        return Ok(DaemonInspection {
            state: "blocked",
            socket_ownership,
            socket_permissions,
            trust_status: "unsafe",
            trust_posture: "unsafe-socket",
            remediation: Some(repair_remediation),
            warnings,
        });
    }

    match UnixStream::connect(socket_path) {
        Ok(_) => Ok(DaemonInspection {
            state: "ready",
            socket_ownership,
            socket_permissions,
            trust_status: "trusted",
            trust_posture: "local-private-socket",
            remediation: None,
            warnings: missing_evidence_warnings(),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            let mut warnings = vec![DiagnosticWarning::new(
                "W_LOCAL_TRUST_UNSAFE",
                "daemon socket refused the current user's connection",
            )];
            warnings.extend(missing_evidence_warnings());
            Ok(DaemonInspection {
                state: "blocked",
                socket_ownership,
                socket_permissions,
                trust_status: "unsafe",
                trust_posture: "socket-connection-blocked",
                remediation: Some(repair_remediation),
                warnings,
            })
        }
        Err(_) => {
            let mut warnings = vec![DiagnosticWarning::new(
                "W_DAEMON_UNREACHABLE",
                "daemon is unreachable; start the daemon or choose another socket",
            )];
            warnings.extend(missing_evidence_warnings());
            Ok(DaemonInspection {
                state: "unreachable",
                socket_ownership,
                socket_permissions,
                trust_status: "degraded",
                trust_posture: "daemon-unreachable",
                remediation: Some(start_remediation),
                warnings,
            })
        }
    }
}

fn missing_evidence_warnings() -> Vec<DiagnosticWarning> {
    vec![
        DiagnosticWarning::new(
            "W_OTEL_UNAVAILABLE",
            "OTel reachability evidence is not configured for this build",
        ),
        DiagnosticWarning::new(
            "W_SIGNATURE_UNVERIFIABLE",
            "build signature evidence is unavailable for this build",
        ),
        DiagnosticWarning::new(
            "W_SBOM_UNAVAILABLE",
            "SBOM identity evidence is unavailable for this build",
        ),
    ]
}

fn run_completion(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [flag] if is_help(flag) => print_completion_help(invocation.output),
        [shell] => print_completion(shell, invocation.output),
        [shell, rest @ ..] if rest.iter().any(|arg| is_help(arg)) => {
            print_completion_help(invocation.output)
        }
        [] => Err(CliError::new(
            "E_UNSUPPORTED_SHELL",
            format!("completion requires a shell; supported shells: {SUPPORTED_SHELLS}"),
            ExitKind::UserError,
        )),
        [shell, arg, ..] if is_supported_shell(shell) => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh completion argument '{arg}'"),
            ExitKind::UserError,
        )),
        [shell, ..] => Err(unsupported_shell(shell)),
    }
}

fn print_completion(shell: &str, output: OutputFormat) -> Result<(), CliError> {
    if output != OutputFormat::Human {
        return Err(CliError::new(
            "E_UNSUPPORTED_OUTPUT_FORMAT",
            "completion generation supports only human output",
            ExitKind::UserError,
        ));
    }

    match shell {
        "bash" => {
            print!("{BASH_COMPLETION}");
            Ok(())
        }
        "zsh" => {
            print!("{ZSH_COMPLETION}");
            Ok(())
        }
        "fish" => {
            print!("{FISH_COMPLETION}");
            Ok(())
        }
        other => Err(unsupported_shell(other)),
    }
}

fn unsupported_shell(shell: &str) -> CliError {
    CliError::new(
        "E_UNSUPPORTED_SHELL",
        format!("unsupported shell '{shell}'; supported shells: {SUPPORTED_SHELLS}"),
        ExitKind::UserError,
    )
}

fn is_supported_shell(shell: &str) -> bool {
    matches!(shell, "bash" | "zsh" | "fish")
}

fn print_completion_help(output: OutputFormat) -> Result<(), CliError> {
    let help = "zornmesh completion\nGenerate shell completions.\n\nUsage: zornmesh completion <SHELL>\n\nShells:\n  bash\n  zsh\n  fish\n\nOptions:\n  -h, --help  Print help\n";
    print_help("completion help", help, output)
}

fn run_trace(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_help("trace help", TRACE_HELP, invocation.output),
        [flag] if is_help(flag) => print_help("trace help", TRACE_HELP, invocation.output),
        [command] if command == "events" => trace_events(invocation.output),
        [command, rest @ ..] if command == "events" && rest.iter().any(|arg| is_help(arg)) => {
            print_help("trace help", TRACE_HELP, invocation.output)
        }
        [command, arg, ..] if command == "events" => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh trace argument '{arg}'"),
            ExitKind::UserError,
        )),
        [correlation_id, rest @ ..] => {
            let correlation_id = parse_non_empty_string("correlation id", correlation_id)?;
            let options = parse_trace_options(rest)?;
            trace_correlation(&correlation_id, options, invocation.output)
        }
    }
}

#[derive(Debug, Clone, Default)]
struct TailOptions {
    evidence_path: Option<PathBuf>,
}

fn parse_tail_options(args: &[String]) -> Result<TailOptions, CliError> {
    let mut options = TailOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if let Some(value) = arg.strip_prefix("--evidence=") {
            options.evidence_path = Some(parse_non_empty_path("--evidence", value)?);
            index += 1;
        } else if let Some(value) = arg.strip_prefix("--evidence-path=") {
            options.evidence_path = Some(parse_non_empty_path("--evidence-path", value)?);
            index += 1;
        } else if let Some(value) = arg.strip_prefix("--store=") {
            options.evidence_path = Some(parse_non_empty_path("--store", value)?);
            index += 1;
        } else if matches!(arg.as_str(), "--evidence" | "--evidence-path" | "--store") {
            let value = inspect_option_value(args, index, arg)?;
            options.evidence_path = Some(parse_non_empty_path(arg, value)?);
            index += 2;
        } else {
            return Err(CliError::new(
                "E_UNSUPPORTED_COMMAND",
                format!("unsupported zornmesh tail argument '{arg}'"),
                ExitKind::UserError,
            ));
        }
    }
    Ok(options)
}

fn run_tail(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_help("tail help", TAIL_HELP, invocation.output),
        [flag] if is_help(flag) => print_help("tail help", TAIL_HELP, invocation.output),
        [pattern, rest @ ..] => {
            let pattern_raw = parse_non_empty_string("subject pattern", pattern)?;
            let pattern = SubjectPattern::new(pattern_raw.clone()).map_err(|error| {
                CliError::new(
                    "E_SUBJECT_VALIDATION",
                    format!("invalid subject pattern '{pattern_raw}': {}", error.message()),
                    ExitKind::Validation,
                )
            })?;
            let options = parse_tail_options(rest)?;
            tail_pattern(&pattern, options, invocation.output)
        }
    }
}

fn tail_pattern(
    pattern: &SubjectPattern,
    options: TailOptions,
    output: OutputFormat,
) -> Result<(), CliError> {
    let evidence_path = resolve_evidence_path(options.evidence_path.as_ref())?;
    let mut warnings = Vec::new();
    let (availability, envelopes, unavailable_reason) = match evidence_path.as_ref() {
        Some(path) => match fs::metadata(path) {
            Ok(_) => match FileEvidenceStore::open_evidence(path) {
                Ok(store) => {
                    let mut records = store.query_envelopes(EvidenceQuery::new());
                    records.retain(|record| pattern.matches(record.subject()));
                    records.sort_by(|left, right| {
                        (left.daemon_sequence(), left.message_id())
                            .cmp(&(right.daemon_sequence(), right.message_id()))
                    });
                    ("available", records, None)
                }
                Err(error) => {
                    let message = format!("evidence store is unavailable: {error}");
                    warnings.push(DiagnosticWarning::new(
                        "W_EVIDENCE_STORE_UNAVAILABLE",
                        message.clone(),
                    ));
                    ("unavailable", Vec::new(), Some(message))
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let message = "evidence store does not exist".to_owned();
                warnings.push(DiagnosticWarning::new(
                    "W_EVIDENCE_STORE_UNAVAILABLE",
                    message.clone(),
                ));
                ("unavailable", Vec::new(), Some(message))
            }
            Err(error) => {
                let message = format!("evidence store cannot be inspected: {error}");
                warnings.push(DiagnosticWarning::new(
                    "W_EVIDENCE_STORE_UNAVAILABLE",
                    message.clone(),
                ));
                ("unavailable", Vec::new(), Some(message))
            }
        },
        None => {
            let message = format!(
                "evidence store path is not configured; pass --evidence <PATH> or set {ENV_EVIDENCE_PATH}"
            );
            warnings.push(DiagnosticWarning::new(
                "W_EVIDENCE_STORE_UNAVAILABLE",
                message.clone(),
            ));
            ("unavailable", Vec::new(), Some(message))
        }
    };

    match output {
        OutputFormat::Human => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            for record in &envelopes {
                let line = format_tail_human_line(record);
                writeln!(handle, "{line}").map_err(io_error)?;
            }
            if availability != "available" {
                if let Some(reason) = unavailable_reason.as_deref() {
                    eprintln!("W_TAIL_DISCONNECTED: {reason}");
                }
            } else if envelopes.is_empty() {
                eprintln!(
                    "W_TAIL_EMPTY: no envelopes matched subject pattern '{}'",
                    pattern.as_str()
                );
            }
            Ok(())
        }
        OutputFormat::Json | OutputFormat::Ndjson => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            writeln!(
                handle,
                "{}",
                serde_json::json!({
                    "schema_version": EVENT_SCHEMA_VERSION,
                    "command": "tail",
                    "kind": "status",
                    "data": {
                        "status": if availability == "available" { "backfill" } else { "disconnected" },
                        "subject_pattern": pattern.as_str(),
                        "ordering": "daemon_sequence",
                        "reason": unavailable_reason,
                    },
                })
            )
            .map_err(io_error)?;
            for record in &envelopes {
                writeln!(handle, "{}", tail_event_json(pattern.as_str(), record))
                    .map_err(io_error)?;
            }
            writeln!(
                handle,
                "{}",
                serde_json::json!({
                    "schema_version": EVENT_SCHEMA_VERSION,
                    "command": "tail",
                    "kind": "status",
                    "data": {
                        "status": if availability == "available" { "stale" } else { "disconnected" },
                        "subject_pattern": pattern.as_str(),
                        "matched": envelopes.len(),
                        "warnings": warnings.iter().map(warning_json).collect::<Vec<_>>(),
                    },
                })
            )
            .map_err(io_error)?;
            Ok(())
        }
    }
}

fn format_tail_human_line(record: &EvidenceEnvelopeRecord) -> String {
    format!(
        "seq={seq} ts={ts} subject={subject} from={source} to={target} state={state} corr={corr}",
        seq = record.daemon_sequence(),
        ts = record.timestamp_unix_ms(),
        subject = record.subject(),
        source = record.source_agent(),
        target = record.target_or_subject(),
        state = record.delivery_state(),
        corr = record.correlation_id(),
    )
}

fn tail_event_json(pattern: &str, record: &EvidenceEnvelopeRecord) -> serde_json::Value {
    serde_json::json!({
        "schema_version": EVENT_SCHEMA_VERSION,
        "command": "tail",
        "kind": "event",
        "data": {
            "subject_pattern": pattern,
            "daemon_sequence": record.daemon_sequence(),
            "timestamp_unix_ms": record.timestamp_unix_ms(),
            "message_id": record.message_id(),
            "subject": record.subject(),
            "source_agent": record.source_agent(),
            "target_or_subject": record.target_or_subject(),
            "delivery_state": record.delivery_state(),
            "correlation_id": record.correlation_id(),
            "trace_id": record.trace_id(),
            "span_id": record.span_id(),
            "parent_message_id": record.parent_message_id(),
            "safe_payload_summary": payload_summary(
                record.payload_len(),
                record.payload_content_type(),
            ),
        },
    })
}

fn io_error(err: io::Error) -> CliError {
    CliError::new(
        "E_IO",
        format!("tail output failed: {err}"),
        ExitKind::Io,
    )
}

#[derive(Debug, Clone, Default)]
struct ReplayOptions {
    evidence_path: Option<PathBuf>,
    preview: bool,
    yes: bool,
    confirmation_token: Option<String>,
}

fn parse_replay_options(args: &[String]) -> Result<ReplayOptions, CliError> {
    let mut options = ReplayOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if let Some(value) = arg.strip_prefix("--evidence=") {
            options.evidence_path = Some(parse_non_empty_path("--evidence", value)?);
            index += 1;
        } else if let Some(value) = arg.strip_prefix("--evidence-path=") {
            options.evidence_path = Some(parse_non_empty_path("--evidence-path", value)?);
            index += 1;
        } else if matches!(arg.as_str(), "--evidence" | "--evidence-path") {
            let value = inspect_option_value(args, index, arg)?;
            options.evidence_path = Some(parse_non_empty_path(arg, value)?);
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--confirmation-token=") {
            options.confirmation_token =
                Some(parse_non_empty_string("--confirmation-token", value)?);
            index += 1;
        } else if arg == "--confirmation-token" {
            let value = inspect_option_value(args, index, arg)?;
            options.confirmation_token = Some(parse_non_empty_string(arg, value)?);
            index += 2;
        } else if arg == "--preview" {
            options.preview = true;
            index += 1;
        } else if arg == "--yes" {
            options.yes = true;
            index += 1;
        } else {
            return Err(CliError::new(
                "E_UNSUPPORTED_COMMAND",
                format!("unsupported zornmesh replay argument '{arg}'"),
                ExitKind::UserError,
            ));
        }
    }
    if options.preview && (options.yes || options.confirmation_token.is_some()) {
        return Err(CliError::new(
            "E_REPLAY_INVALID_FLAGS",
            "--preview cannot be combined with --yes or --confirmation-token".to_owned(),
            ExitKind::UserError,
        ));
    }
    if options.yes && options.confirmation_token.is_some() {
        return Err(CliError::new(
            "E_REPLAY_INVALID_FLAGS",
            "--yes cannot be combined with --confirmation-token".to_owned(),
            ExitKind::UserError,
        ));
    }
    Ok(options)
}

fn run_replay(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_help("replay help", REPLAY_HELP, invocation.output),
        [flag] if is_help(flag) => print_help("replay help", REPLAY_HELP, invocation.output),
        [message_id, rest @ ..] => {
            let message_id = parse_non_empty_string("message id", message_id)?;
            let options = parse_replay_options(rest)?;
            replay_message(&message_id, options, invocation.output)
        }
    }
}

fn replay_message(
    message_id: &str,
    options: ReplayOptions,
    output: OutputFormat,
) -> Result<(), CliError> {
    if output == OutputFormat::Ndjson {
        return Err(ndjson_not_supported("replay"));
    }
    let evidence_path = resolve_evidence_path(options.evidence_path.as_ref())?;
    let path = evidence_path.ok_or_else(|| {
        CliError::new(
            "E_EVIDENCE_STORE_UNAVAILABLE",
            format!(
                "evidence store path is not configured; pass --evidence <PATH> or set {ENV_EVIDENCE_PATH}"
            ),
            ExitKind::TemporaryUnavailable,
        )
    })?;
    let store = FileEvidenceStore::open_evidence(&path).map_err(|error| {
        CliError::new(
            "E_EVIDENCE_STORE_UNAVAILABLE",
            format!("evidence store is unavailable: {error}"),
            ExitKind::TemporaryUnavailable,
        )
    })?;
    let record = store
        .get_envelope(message_id)
        .map_err(|error| {
            CliError::new(
                "E_EVIDENCE_STORE_UNAVAILABLE",
                format!("evidence store cannot be queried: {error}"),
                ExitKind::TemporaryUnavailable,
            )
        })?
        .ok_or_else(|| {
            CliError::new(
                "E_REPLAY_NOT_FOUND",
                format!("no envelope evidence matched message id '{message_id}'"),
                ExitKind::NotFound,
            )
        })?;

    let (eligibility, ineligible_reason) = evaluate_replay_eligibility(&record);
    let token = replay_confirmation_token(&record);
    let lineage = serde_json::json!({
        "replayed_from": record.message_id(),
        "original_correlation_id": record.correlation_id(),
        "original_trace_id": record.trace_id(),
    });
    let safe_payload = payload_summary(record.payload_len(), record.payload_content_type());

    if eligibility != "eligible" {
        let data = serde_json::json!({
            "mode": if options.preview { "preview" } else { "commit" },
            "eligibility": eligibility,
            "refusal_reason": ineligible_reason,
            "side_effect": false,
            "original_message_id": record.message_id(),
            "subject": record.subject(),
            "target": record.target_or_subject(),
            "replay_lineage": lineage,
            "safe_payload_summary": safe_payload,
        });
        return emit_replay_response(
            output,
            data,
            Some(DiagnosticWarning::new(
                "W_REPLAY_INELIGIBLE",
                format!(
                    "replay refused: {}",
                    ineligible_reason.unwrap_or("ineligible")
                ),
            )),
        );
    }

    if options.preview {
        let data = serde_json::json!({
            "mode": "preview",
            "eligibility": "eligible",
            "side_effect": false,
            "confirmation_token": token,
            "original_message_id": record.message_id(),
            "subject": record.subject(),
            "target": record.target_or_subject(),
            "replay_lineage": lineage,
            "safe_payload_summary": safe_payload,
            "expected_effect": "creates a new replay audit entry linked to the original message",
            "policy_checks": ["evidence_store_available", "record_exists", "redaction_safe"],
            "required_confirmation": "rerun with --yes or --confirmation-token <TOKEN>",
        });
        return emit_replay_response(output, data, None);
    }

    if !options.yes {
        match options.confirmation_token.as_deref() {
            None => {
                return Err(CliError::new(
                    "E_REPLAY_CONFIRMATION_REQUIRED",
                    format!(
                        "replay '{message_id}' requires confirmation; rerun with --preview, --yes, or --confirmation-token <TOKEN>"
                    ),
                    ExitKind::Validation,
                ));
            }
            Some(provided) if provided != token => {
                return Err(CliError::new(
                    "E_REPLAY_STALE_CONFIRMATION",
                    format!(
                        "confirmation token does not match preview for '{message_id}'; rerun --preview to obtain a fresh token"
                    ),
                    ExitKind::Validation,
                ));
            }
            Some(_) => {}
        }
    }

    let transition = EvidenceStateTransitionInput::new(
        record.daemon_sequence(),
        record.message_id(),
        record.source_agent(),
        "replay_requested",
        record.subject(),
        record.correlation_id(),
        record.trace_id(),
        record.delivery_state(),
        "replayed",
        format!("replayed from {}", record.message_id()),
    )
    .map_err(|error| {
        CliError::new(
            "E_REPLAY_INVALID_INPUT",
            format!("replay state transition input invalid: {error}"),
            ExitKind::Validation,
        )
    })?;
    let audit = store.persist_state_transition(transition).map_err(|error| {
        CliError::new(
            "E_REPLAY_PERSIST_FAILED",
            format!("replay audit entry could not be persisted: {error}"),
            ExitKind::Io,
        )
    })?;

    let data = serde_json::json!({
        "mode": "commit",
        "eligibility": "eligible",
        "side_effect": true,
        "confirmation_source": if options.yes { "yes_flag" } else { "confirmation_token" },
        "confirmation_token": token,
        "original_message_id": record.message_id(),
        "subject": record.subject(),
        "target": record.target_or_subject(),
        "replay_lineage": lineage,
        "safe_payload_summary": safe_payload,
        "audit_daemon_sequence": audit.daemon_sequence(),
        "audit_action": audit.action(),
        "state_to": audit.state_to(),
    });
    emit_replay_response(output, data, None)
}

fn emit_replay_response(
    output: OutputFormat,
    data: serde_json::Value,
    warning: Option<DiagnosticWarning>,
) -> Result<(), CliError> {
    let warnings: Vec<DiagnosticWarning> = warning.into_iter().collect();
    match output {
        OutputFormat::Human => {
            println!(
                "replay: mode={} eligibility={} side_effect={}",
                data["mode"].as_str().unwrap_or(""),
                data["eligibility"].as_str().unwrap_or(""),
                data["side_effect"].as_bool().unwrap_or(false),
            );
            println!(
                "  original_message_id={}",
                data["original_message_id"].as_str().unwrap_or("")
            );
            println!("  subject={}", data["subject"].as_str().unwrap_or(""));
            println!("  target={}", data["target"].as_str().unwrap_or(""));
            if let Some(token) = data["confirmation_token"].as_str() {
                println!("  confirmation_token={token}");
            }
            if let Some(reason) = data["refusal_reason"].as_str() {
                println!("  refusal_reason={reason}");
            }
            for warning in &warnings {
                eprintln!("{}: {}", warning.code, warning.message);
            }
            Ok(())
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": READ_SCHEMA_VERSION,
                    "command": "replay",
                    "status": "ok",
                    "data": data,
                    "warnings": warnings.iter().map(warning_json).collect::<Vec<_>>(),
                })
            );
            Ok(())
        }
        OutputFormat::Ndjson => unreachable!("ndjson rejected before replay rendering"),
    }
}

fn evaluate_replay_eligibility(
    record: &EvidenceEnvelopeRecord,
) -> (&'static str, Option<&'static str>) {
    const MAX_REPLAY_BYTES: usize = 1_048_576;
    if record.payload_len() > MAX_REPLAY_BYTES {
        return ("ineligible", Some("payload_too_large"));
    }
    if record.source_agent() == "[REDACTED]" || record.target_or_subject() == "[REDACTED]" {
        return ("ineligible", Some("redaction_blocks_replay"));
    }
    ("eligible", None)
}

fn replay_confirmation_token(record: &EvidenceEnvelopeRecord) -> String {
    let parts = format!(
        "{}|{}|{}|{}|{}",
        record.message_id(),
        record.correlation_id(),
        record.trace_id(),
        record.subject(),
        record.daemon_sequence(),
    );
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in parts.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("zmpv-{hash:016x}")
}

#[derive(Debug, Clone, Default)]
struct TraceOptions {
    evidence_path: Option<PathBuf>,
    span_tree: bool,
}

fn parse_trace_options(args: &[String]) -> Result<TraceOptions, CliError> {
    let mut options = TraceOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if let Some(value) = arg.strip_prefix("--evidence=") {
            options.evidence_path = Some(parse_non_empty_path("--evidence", value)?);
            index += 1;
        } else if let Some(value) = arg.strip_prefix("--evidence-path=") {
            options.evidence_path = Some(parse_non_empty_path("--evidence-path", value)?);
            index += 1;
        } else if let Some(value) = arg.strip_prefix("--store=") {
            options.evidence_path = Some(parse_non_empty_path("--store", value)?);
            index += 1;
        } else if matches!(arg.as_str(), "--evidence" | "--evidence-path" | "--store") {
            let value = inspect_option_value(args, index, arg)?;
            options.evidence_path = Some(parse_non_empty_path(arg, value)?);
            index += 2;
        } else if arg == "--span-tree" {
            options.span_tree = true;
            index += 1;
        } else {
            return Err(CliError::new(
                "E_UNSUPPORTED_COMMAND",
                format!("unsupported zornmesh trace argument '{arg}'"),
                ExitKind::UserError,
            ));
        }
    }
    Ok(options)
}

#[derive(Debug, Clone)]
struct TraceEvent {
    daemon_sequence: u64,
    sort_rank: usize,
    kind: &'static str,
    message_id: String,
    timestamp_unix_ms: Option<u64>,
    source_agent: Option<String>,
    target_or_subject: Option<String>,
    subject: Option<String>,
    participating_agents: Vec<String>,
    delivery_state: String,
    correlation_id: String,
    trace_id: String,
    span_id: Option<String>,
    parent_message_id: Option<String>,
    action: Option<String>,
    relationship: &'static str,
    exceptional_state: Option<&'static str>,
    safe_payload_summary: serde_json::Value,
    failure_category: Option<&'static str>,
    attempt_count: Option<u32>,
    safe_details: Option<String>,
}

#[derive(Debug, Clone)]
struct TraceGap {
    code: &'static str,
    message_id: String,
    missing_parent_message_id: Option<String>,
    remediation: &'static str,
}

fn trace_correlation(
    correlation_id: &str,
    options: TraceOptions,
    output: OutputFormat,
) -> Result<(), CliError> {
    if output == OutputFormat::Ndjson {
        return Err(ndjson_not_supported("trace"));
    }

    let evidence_path = resolve_evidence_path(options.evidence_path.as_ref())?;
    let mut warnings = Vec::new();
    let (availability, events, span_tree, gaps, metadata) = match evidence_path.as_ref() {
        Some(path) => match fs::metadata(path) {
            Ok(_) => match FileEvidenceStore::open_evidence(path) {
                Ok(store) => {
                    let reconstruction = build_trace_reconstruction(correlation_id, &store);
                    (
                        "available",
                        reconstruction.events,
                        build_span_tree(&reconstruction.envelopes, &reconstruction.dead_letter_ids),
                        reconstruction.gaps,
                        inspect_metadata_json("available", Some(path), Some(&store), None),
                    )
                }
                Err(error) => {
                    let message = format!("evidence store is unavailable: {error}");
                    warnings.push(DiagnosticWarning::new(
                        "W_EVIDENCE_STORE_UNAVAILABLE",
                        message.clone(),
                    ));
                    (
                        "unavailable",
                        Vec::new(),
                        empty_span_tree(options.span_tree, "unavailable"),
                        Vec::new(),
                        inspect_metadata_json("unavailable", Some(path), None, Some(&message)),
                    )
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let message = "evidence store does not exist".to_owned();
                warnings.push(DiagnosticWarning::new(
                    "W_EVIDENCE_STORE_UNAVAILABLE",
                    message.clone(),
                ));
                (
                    "unavailable",
                    Vec::new(),
                    empty_span_tree(options.span_tree, "unavailable"),
                    Vec::new(),
                    inspect_metadata_json("unavailable", Some(path), None, Some(&message)),
                )
            }
            Err(error) => {
                let message = format!("evidence store cannot be inspected: {error}");
                warnings.push(DiagnosticWarning::new(
                    "W_EVIDENCE_STORE_UNAVAILABLE",
                    message.clone(),
                ));
                (
                    "unavailable",
                    Vec::new(),
                    empty_span_tree(options.span_tree, "unavailable"),
                    Vec::new(),
                    inspect_metadata_json("unavailable", Some(path), None, Some(&message)),
                )
            }
        },
        None => {
            let message = format!(
                "evidence store path is not configured; pass --evidence <PATH> or set {ENV_EVIDENCE_PATH}"
            );
            warnings.push(DiagnosticWarning::new(
                "W_EVIDENCE_STORE_UNAVAILABLE",
                message.clone(),
            ));
            (
                "unavailable",
                Vec::new(),
                empty_span_tree(options.span_tree, "unavailable"),
                Vec::new(),
                inspect_metadata_json("unavailable", None, None, Some(&message)),
            )
        }
    };

    let state = trace_state(availability, &events, &gaps, &span_tree);
    if state == "not_found" {
        warnings.push(DiagnosticWarning::new(
            "W_TRACE_NOT_FOUND",
            format!("no evidence records matched correlation ID '{correlation_id}'"),
        ));
    }
    if state == "partial" {
        warnings.push(DiagnosticWarning::new(
            "W_TRACE_GAP_DETECTED",
            "trace reconstruction is partial; inspect audit evidence, retention, and doctor output",
        ));
    }

    let next_actions = trace_next_actions(state);
    let participants = trace_participants(&events);
    let delivery_states = trace_delivery_states(&events);
    match output {
        OutputFormat::Human => print_trace_human(TraceHumanReport {
            correlation_id,
            availability,
            state,
            events: &events,
            participants: &participants,
            delivery_states: &delivery_states,
            gaps: &gaps,
            span_tree: &span_tree,
            span_tree_requested: options.span_tree,
            next_actions: &next_actions,
            warnings: &warnings,
        }),
        OutputFormat::Json => {
            let data = serde_json::json!({
                "correlation_id": correlation_id,
                "availability": availability,
                "state": state,
                "ordering": "daemon_sequence",
                "timeline": events.iter().map(trace_event_json).collect::<Vec<_>>(),
                "participants": participants,
                "delivery_states": delivery_states,
                "gaps": gaps.iter().map(trace_gap_json).collect::<Vec<_>>(),
                "span_tree": span_tree_json(&span_tree, options.span_tree),
                "metadata": metadata,
                "next_actions": next_actions,
            });
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": READ_SCHEMA_VERSION,
                    "command": "trace",
                    "status": "ok",
                    "data": data,
                    "warnings": warnings.iter().map(warning_json).collect::<Vec<_>>(),
                })
            );
            Ok(())
        }
        OutputFormat::Ndjson => unreachable!("ndjson rejected before trace rendering"),
    }
}

#[derive(Debug, Clone)]
struct TraceReconstruction {
    events: Vec<TraceEvent>,
    envelopes: Vec<EvidenceEnvelopeRecord>,
    dead_letter_ids: HashSet<String>,
    gaps: Vec<TraceGap>,
}

fn build_trace_reconstruction(
    correlation_id: &str,
    store: &FileEvidenceStore,
) -> TraceReconstruction {
    let mut envelopes = store.query_envelopes(EvidenceQuery::new().correlation_id(correlation_id));
    envelopes.sort_by(|left, right| {
        (left.daemon_sequence(), left.message_id())
            .cmp(&(right.daemon_sequence(), right.message_id()))
    });
    let envelope_by_id = envelopes
        .iter()
        .map(|record| (record.message_id().to_owned(), record.clone()))
        .collect::<HashMap<_, _>>();
    let mut dead_letters =
        store.query_dead_letters(DeadLetterQuery::new().correlation_id(correlation_id));
    dead_letters.sort_by(|left, right| {
        (left.daemon_sequence(), left.message_id())
            .cmp(&(right.daemon_sequence(), right.message_id()))
    });
    let dead_letter_ids = dead_letters
        .iter()
        .map(|record| record.message_id().to_owned())
        .collect::<HashSet<_>>();
    let mut events = Vec::new();

    for envelope in &envelopes {
        let relationship = trace_relationship(
            envelope.parent_message_id(),
            envelope.subject(),
            envelope.delivery_state(),
            None,
            None,
            dead_letter_ids.contains(envelope.message_id()),
        );
        let exceptional_state = exceptional_state(
            envelope.subject(),
            envelope.delivery_state(),
            None,
            None,
            dead_letter_ids.contains(envelope.message_id()),
        );
        events.push(TraceEvent {
            daemon_sequence: envelope.daemon_sequence(),
            sort_rank: 0,
            kind: "envelope",
            message_id: envelope.message_id().to_owned(),
            timestamp_unix_ms: Some(envelope.timestamp_unix_ms()),
            source_agent: Some(envelope.source_agent().to_owned()),
            target_or_subject: Some(envelope.target_or_subject().to_owned()),
            subject: Some(envelope.subject().to_owned()),
            participating_agents: participating_agents([
                Some(envelope.source_agent()),
                Some(envelope.target_or_subject()),
            ]),
            delivery_state: envelope.delivery_state().to_owned(),
            correlation_id: envelope.correlation_id().to_owned(),
            trace_id: envelope.trace_id().to_owned(),
            span_id: Some(envelope.span_id().to_owned()),
            parent_message_id: envelope.parent_message_id().map(ToOwned::to_owned),
            action: None,
            relationship,
            exceptional_state,
            safe_payload_summary: payload_summary(
                envelope.payload_len(),
                envelope.payload_content_type(),
            ),
            failure_category: None,
            attempt_count: None,
            safe_details: None,
        });
    }

    for dead_letter in &dead_letters {
        let envelope = envelope_by_id.get(dead_letter.message_id());
        events.push(TraceEvent {
            daemon_sequence: dead_letter.daemon_sequence(),
            sort_rank: 5,
            kind: "dead_letter",
            message_id: dead_letter.message_id().to_owned(),
            timestamp_unix_ms: Some(dead_letter.terminal_unix_ms()),
            source_agent: Some(dead_letter.source_agent().to_owned()),
            target_or_subject: dead_letter.intended_target().map(ToOwned::to_owned),
            subject: Some(dead_letter.subject().to_owned()),
            participating_agents: participating_agents([
                Some(dead_letter.source_agent()),
                dead_letter.intended_target(),
            ]),
            delivery_state: dead_letter.terminal_state().to_owned(),
            correlation_id: dead_letter.correlation_id().to_owned(),
            trace_id: dead_letter.trace_id().to_owned(),
            span_id: envelope.map(|record| record.span_id().to_owned()),
            parent_message_id: envelope
                .and_then(EvidenceEnvelopeRecord::parent_message_id)
                .map(ToOwned::to_owned),
            action: Some("dead_lettered".to_owned()),
            relationship: "dead-letter-terminal",
            exceptional_state: Some("dead_letter"),
            safe_payload_summary: payload_summary(
                dead_letter.payload_len(),
                dead_letter.payload_content_type(),
            ),
            failure_category: Some(dead_letter.failure_category().as_str()),
            attempt_count: Some(dead_letter.attempt_count()),
            safe_details: Some(dead_letter.safe_details().to_owned()),
        });
    }

    for (audit_index, audit) in store
        .audit_entries()
        .into_iter()
        .filter(|entry| entry.correlation_id() == correlation_id)
        .enumerate()
    {
        let envelope = envelope_by_id.get(audit.message_id());
        let relationship = trace_relationship(
            envelope.and_then(EvidenceEnvelopeRecord::parent_message_id),
            audit.capability_or_subject(),
            audit.state_to(),
            Some(audit.action()),
            Some(audit.outcome_details()),
            audit.action() == "dead_lettered" || audit.state_to() == "dead_lettered",
        );
        let exceptional_state = exceptional_state(
            audit.capability_or_subject(),
            audit.state_to(),
            Some(audit.action()),
            Some(audit.outcome_details()),
            audit.action() == "dead_lettered" || audit.state_to() == "dead_lettered",
        );
        events.push(TraceEvent {
            daemon_sequence: audit.daemon_sequence(),
            sort_rank: 10 + audit_index,
            kind: "audit",
            message_id: audit.message_id().to_owned(),
            timestamp_unix_ms: envelope.map(EvidenceEnvelopeRecord::timestamp_unix_ms),
            source_agent: Some(audit.actor().to_owned()),
            target_or_subject: None,
            subject: Some(audit.capability_or_subject().to_owned()),
            participating_agents: participating_agents([Some(audit.actor()), None]),
            delivery_state: audit.state_to().to_owned(),
            correlation_id: audit.correlation_id().to_owned(),
            trace_id: audit.trace_id().to_owned(),
            span_id: envelope.map(|record| record.span_id().to_owned()),
            parent_message_id: envelope
                .and_then(EvidenceEnvelopeRecord::parent_message_id)
                .map(ToOwned::to_owned),
            action: Some(audit.action().to_owned()),
            relationship,
            exceptional_state,
            safe_payload_summary: serde_json::json!({
                "outcome_details": audit.outcome_details(),
            }),
            failure_category: None,
            attempt_count: None,
            safe_details: Some(audit.outcome_details().to_owned()),
        });
    }

    events.sort_by(|left, right| {
        (
            left.daemon_sequence,
            left.sort_rank,
            left.kind,
            left.message_id.as_str(),
        )
            .cmp(&(
                right.daemon_sequence,
                right.sort_rank,
                right.kind,
                right.message_id.as_str(),
            ))
    });
    let gaps = trace_gaps(&envelopes);

    TraceReconstruction {
        events,
        envelopes,
        dead_letter_ids,
        gaps,
    }
}

#[derive(Debug, Clone)]
struct TraceSpanTree {
    reconstruction: &'static str,
    nodes: Vec<TraceSpanNode>,
}

#[derive(Debug, Clone)]
struct TraceSpanNode {
    message_id: String,
    trace_id: String,
    span_id: String,
    parent_message_id: Option<String>,
    child_message_ids: Vec<String>,
    depth: Option<usize>,
    relationship: &'static str,
    source_agent: String,
    target_or_subject: String,
    subject: String,
    delivery_state: String,
    stream_sequence: Option<usize>,
    stream_state: Option<&'static str>,
    status: &'static str,
    invalid_reasons: Vec<&'static str>,
}

fn build_span_tree(
    envelopes: &[EvidenceEnvelopeRecord],
    dead_letter_ids: &HashSet<String>,
) -> TraceSpanTree {
    let parent_by_message = envelopes
        .iter()
        .map(|record| {
            (
                record.message_id().to_owned(),
                record.parent_message_id().map(ToOwned::to_owned),
            )
        })
        .collect::<HashMap<_, _>>();
    let record_by_message = envelopes
        .iter()
        .map(|record| (record.message_id().to_owned(), record))
        .collect::<HashMap<_, _>>();
    let mut child_by_parent = envelopes.iter().fold(
        HashMap::<String, Vec<String>>::new(),
        |mut children_by_parent, record| {
            if let Some(parent) = record.parent_message_id() {
                children_by_parent
                    .entry(parent.to_owned())
                    .or_default()
                    .push(record.message_id().to_owned());
            }
            children_by_parent
        },
    );
    for child_ids in child_by_parent.values_mut() {
        child_ids.sort_by(|left, right| {
            span_sort_key(&record_by_message, left).cmp(&span_sort_key(&record_by_message, right))
        });
    }
    let duplicate_edges = duplicate_span_edges(envelopes);
    let cycle_nodes = cycle_nodes(&parent_by_message);
    let stream_sequence_by_message = stream_sequences(&child_by_parent, &record_by_message);
    let mut nodes = envelopes
        .iter()
        .map(|record| {
            let mut invalid_reasons = Vec::new();
            if record.parent_message_id() == Some(record.message_id()) {
                invalid_reasons.push("self_parent");
            } else if record
                .parent_message_id()
                .is_some_and(|parent| !parent_by_message.contains_key(parent))
            {
                invalid_reasons.push("missing_parent");
            }
            if record.parent_message_id().is_some_and(|parent| {
                duplicate_edges.contains(&(parent.to_owned(), record.span_id().to_owned()))
            }) {
                invalid_reasons.push("duplicate_edge");
            }
            if cycle_nodes.contains(record.message_id()) {
                invalid_reasons.push("cycle");
            }
            TraceSpanNode {
                message_id: record.message_id().to_owned(),
                trace_id: record.trace_id().to_owned(),
                span_id: record.span_id().to_owned(),
                parent_message_id: record.parent_message_id().map(ToOwned::to_owned),
                child_message_ids: child_by_parent
                    .get(record.message_id())
                    .cloned()
                    .unwrap_or_default(),
                depth: span_depth(record.message_id(), &parent_by_message, &cycle_nodes),
                relationship: trace_relationship(
                    record.parent_message_id(),
                    record.subject(),
                    record.delivery_state(),
                    None,
                    None,
                    dead_letter_ids.contains(record.message_id()),
                ),
                source_agent: record.source_agent().to_owned(),
                target_or_subject: record.target_or_subject().to_owned(),
                subject: record.subject().to_owned(),
                delivery_state: record.delivery_state().to_owned(),
                stream_sequence: stream_sequence_by_message.get(record.message_id()).copied(),
                stream_state: stream_state(record.subject(), record.delivery_state()),
                status: if invalid_reasons.is_empty() {
                    "valid"
                } else {
                    "partial"
                },
                invalid_reasons,
            }
        })
        .collect::<Vec<_>>();
    let span_order = ordered_span_message_ids(envelopes, &child_by_parent, &record_by_message)
        .into_iter()
        .enumerate()
        .map(|(index, message_id)| (message_id, index))
        .collect::<HashMap<_, _>>();
    nodes.sort_by(|left, right| {
        span_order
            .get(&left.message_id)
            .cmp(&span_order.get(&right.message_id))
            .then_with(|| left.message_id.cmp(&right.message_id))
    });
    let reconstruction = if nodes.iter().any(|node| !node.invalid_reasons.is_empty()) {
        "partial"
    } else if nodes.is_empty() {
        "empty"
    } else {
        "complete"
    };
    TraceSpanTree {
        reconstruction,
        nodes,
    }
}

fn span_sort_key(
    record_by_message: &HashMap<String, &EvidenceEnvelopeRecord>,
    message_id: &str,
) -> (u64, String) {
    record_by_message
        .get(message_id)
        .map(|record| (record.daemon_sequence(), record.message_id().to_owned()))
        .unwrap_or((u64::MAX, message_id.to_owned()))
}

fn duplicate_span_edges(envelopes: &[EvidenceEnvelopeRecord]) -> HashSet<(String, String)> {
    let mut edge_counts = HashMap::<(String, String), usize>::new();
    for record in envelopes {
        if let Some(parent) = record.parent_message_id() {
            *edge_counts
                .entry((parent.to_owned(), record.span_id().to_owned()))
                .or_default() += 1;
        }
    }
    edge_counts
        .into_iter()
        .filter_map(|(edge, count)| (count > 1).then_some(edge))
        .collect()
}

fn stream_sequences(
    child_by_parent: &HashMap<String, Vec<String>>,
    record_by_message: &HashMap<String, &EvidenceEnvelopeRecord>,
) -> HashMap<String, usize> {
    let mut sequences = HashMap::new();
    for child_ids in child_by_parent.values() {
        let mut sequence = 0;
        for child_id in child_ids {
            let Some(record) = record_by_message.get(child_id) else {
                continue;
            };
            if stream_state(record.subject(), record.delivery_state()).is_some() {
                sequence += 1;
                sequences.insert(child_id.clone(), sequence);
            }
        }
    }
    sequences
}

fn span_depth(
    message_id: &str,
    parent_by_message: &HashMap<String, Option<String>>,
    cycle_nodes: &HashSet<String>,
) -> Option<usize> {
    let mut depth = 0;
    let mut seen = HashSet::new();
    let mut cursor = message_id;
    loop {
        if cycle_nodes.contains(cursor) || !seen.insert(cursor.to_owned()) {
            return None;
        }
        match parent_by_message.get(cursor) {
            Some(None) => return Some(depth),
            Some(Some(parent)) if !parent_by_message.contains_key(parent) => return None,
            Some(Some(parent)) => {
                depth += 1;
                cursor = parent;
            }
            None => return None,
        }
    }
}

fn ordered_span_message_ids(
    envelopes: &[EvidenceEnvelopeRecord],
    child_by_parent: &HashMap<String, Vec<String>>,
    record_by_message: &HashMap<String, &EvidenceEnvelopeRecord>,
) -> Vec<String> {
    let mut all_ids = envelopes
        .iter()
        .map(|record| record.message_id().to_owned())
        .collect::<Vec<_>>();
    all_ids.sort_by(|left, right| {
        span_sort_key(record_by_message, left).cmp(&span_sort_key(record_by_message, right))
    });

    let mut ordered = Vec::new();
    let mut visited = HashSet::new();
    for message_id in all_ids.iter().filter(|message_id| {
        record_by_message
            .get(message_id.as_str())
            .is_some_and(|record| record.parent_message_id().is_none())
    }) {
        append_span_subtree(message_id, child_by_parent, &mut visited, &mut ordered);
    }
    for message_id in &all_ids {
        append_span_subtree(message_id, child_by_parent, &mut visited, &mut ordered);
    }
    ordered
}

fn append_span_subtree(
    message_id: &str,
    child_by_parent: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    ordered: &mut Vec<String>,
) {
    if !visited.insert(message_id.to_owned()) {
        return;
    }
    ordered.push(message_id.to_owned());
    if let Some(child_ids) = child_by_parent.get(message_id) {
        for child_id in child_ids {
            append_span_subtree(child_id, child_by_parent, visited, ordered);
        }
    }
}

fn cycle_nodes(parent_by_message: &HashMap<String, Option<String>>) -> HashSet<String> {
    let mut cyclic = HashSet::new();
    for message_id in parent_by_message.keys() {
        let mut seen = HashSet::new();
        let mut cursor = Some(message_id.as_str());
        while let Some(current) = cursor {
            if !seen.insert(current.to_owned()) {
                cyclic.extend(seen);
                break;
            }
            cursor = parent_by_message
                .get(current)
                .and_then(|parent| parent.as_deref());
        }
    }
    cyclic
}

fn stream_state(subject: &str, state: &str) -> Option<&'static str> {
    let text = format!("{subject} {state}").to_ascii_lowercase();
    if !text.contains("stream") {
        return None;
    }
    if text.contains("cancel") {
        Some("cancelled")
    } else if text.contains("fail") || text.contains("error") {
        Some("failed")
    } else if text.contains("gap") {
        Some("gap")
    } else if text.contains("final") || text.contains("complete") {
        Some("final")
    } else {
        Some("continue")
    }
}

fn empty_span_tree(_requested: bool, reconstruction: &'static str) -> TraceSpanTree {
    TraceSpanTree {
        reconstruction,
        nodes: Vec::new(),
    }
}

fn trace_gaps(envelopes: &[EvidenceEnvelopeRecord]) -> Vec<TraceGap> {
    let message_ids = envelopes
        .iter()
        .map(|record| record.message_id().to_owned())
        .collect::<HashSet<_>>();
    envelopes
        .iter()
        .filter_map(|record| {
            let parent = record.parent_message_id()?;
            (!message_ids.contains(parent)).then(|| TraceGap {
                code: "missing_parent",
                message_id: record.message_id().to_owned(),
                missing_parent_message_id: Some(parent.to_owned()),
                remediation: "run inspect audit, check retention policy, and verify the audit chain",
            })
        })
        .collect()
}

fn trace_state(
    availability: &str,
    events: &[TraceEvent],
    gaps: &[TraceGap],
    span_tree: &TraceSpanTree,
) -> &'static str {
    if availability != "available" {
        "unavailable"
    } else if events.is_empty() {
        "not_found"
    } else if !gaps.is_empty() || span_tree.reconstruction == "partial" {
        "partial"
    } else {
        "complete"
    }
}

fn trace_next_actions(state: &str) -> Vec<&'static str> {
    match state {
        "not_found" | "partial" | "unavailable" => {
            vec![
                "inspect",
                "doctor",
                "retention checks",
                "audit verification",
            ]
        }
        _ => Vec::new(),
    }
}

fn trace_participants(events: &[TraceEvent]) -> Vec<String> {
    events
        .iter()
        .flat_map(|event| event.participating_agents.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn trace_delivery_states(events: &[TraceEvent]) -> Vec<String> {
    events
        .iter()
        .map(|event| event.delivery_state.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn participating_agents<const N: usize>(agents: [Option<&str>; N]) -> Vec<String> {
    agents
        .into_iter()
        .flatten()
        .filter(|agent| agent.starts_with("agent."))
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn payload_summary(payload_len: usize, payload_content_type: &str) -> serde_json::Value {
    serde_json::json!({
        "payload_len": payload_len,
        "payload_content_type": payload_content_type,
    })
}

fn exceptional_state(
    subject: &str,
    state: &str,
    action: Option<&str>,
    details: Option<&str>,
    dead_letter: bool,
) -> Option<&'static str> {
    let text = format!(
        "{} {} {} {}",
        subject,
        state,
        action.unwrap_or_default(),
        details.unwrap_or_default()
    )
    .to_ascii_lowercase();
    if dead_letter || text.contains("dead_letter") || text.contains("dead-letter") {
        Some("dead_letter")
    } else if text.contains("cancel") {
        Some("cancellation")
    } else if text.contains("late") {
        Some("late_arrival")
    } else if text.contains("replay") {
        Some("replay")
    } else if text.contains("retry") {
        Some("retry")
    } else {
        None
    }
}

fn trace_relationship(
    parent_message_id: Option<&str>,
    subject: &str,
    state: &str,
    action: Option<&str>,
    details: Option<&str>,
    dead_letter: bool,
) -> &'static str {
    let text = format!(
        "{} {} {} {}",
        subject,
        state,
        action.unwrap_or_default(),
        details.unwrap_or_default()
    )
    .to_ascii_lowercase();
    if dead_letter || text.contains("dead_letter") || text.contains("dead-letter") {
        "dead-letter-terminal"
    } else if text.contains("replay") {
        "replayed-from"
    } else if text.contains("retry") {
        "retry-of"
    } else if text.contains("reply") || text.contains("response") {
        "responds-to"
    } else if parent_message_id.is_some() {
        "caused-by"
    } else {
        "root"
    }
}

fn trace_event_json(event: &TraceEvent) -> serde_json::Value {
    serde_json::json!({
        "daemon_sequence": event.daemon_sequence,
        "kind": event.kind,
        "message_id": event.message_id,
        "timestamp_unix_ms": event.timestamp_unix_ms,
        "source_agent": event.source_agent,
        "target_or_subject": event.target_or_subject,
        "subject": event.subject,
        "participating_agents": event.participating_agents,
        "delivery_state": event.delivery_state,
        "correlation_id": event.correlation_id,
        "trace_id": event.trace_id,
        "span_id": event.span_id,
        "parent_message_id": event.parent_message_id,
        "action": event.action,
        "relationship": event.relationship,
        "exceptional": event.exceptional_state.is_some(),
        "exceptional_state": event.exceptional_state,
        "safe_payload_summary": event.safe_payload_summary,
        "failure_category": event.failure_category,
        "attempt_count": event.attempt_count,
        "safe_details": event.safe_details,
    })
}

fn trace_gap_json(gap: &TraceGap) -> serde_json::Value {
    serde_json::json!({
        "code": gap.code,
        "message_id": gap.message_id,
        "missing_parent_message_id": gap.missing_parent_message_id,
        "remediation": gap.remediation,
    })
}

fn span_tree_json(span_tree: &TraceSpanTree, requested: bool) -> serde_json::Value {
    serde_json::json!({
        "requested": requested,
        "reconstruction": span_tree.reconstruction,
        "nodes": span_tree.nodes.iter().map(span_node_json).collect::<Vec<_>>(),
    })
}

fn span_node_json(node: &TraceSpanNode) -> serde_json::Value {
    serde_json::json!({
        "message_id": node.message_id,
        "trace_id": node.trace_id,
        "span_id": node.span_id,
        "parent_message_id": node.parent_message_id,
        "child_message_ids": node.child_message_ids,
        "depth": node.depth,
        "relationship": node.relationship,
        "source_agent": node.source_agent,
        "target_or_subject": node.target_or_subject,
        "subject": node.subject,
        "delivery_state": node.delivery_state,
        "stream_sequence": node.stream_sequence,
        "stream_state": node.stream_state,
        "status": node.status,
        "invalid_reasons": node.invalid_reasons,
    })
}

struct TraceHumanReport<'a> {
    correlation_id: &'a str,
    availability: &'static str,
    state: &'static str,
    events: &'a [TraceEvent],
    participants: &'a [String],
    delivery_states: &'a [String],
    gaps: &'a [TraceGap],
    span_tree: &'a TraceSpanTree,
    span_tree_requested: bool,
    next_actions: &'a [&'static str],
    warnings: &'a [DiagnosticWarning],
}

fn print_trace_human(report: TraceHumanReport<'_>) -> Result<(), CliError> {
    println!("zornmesh trace {}", report.correlation_id);
    println!("status: {}", report.availability);
    println!("state: {}", report.state);
    println!("ordering: daemon_sequence");
    println!("events: {}", report.events.len());
    if !report.participants.is_empty() {
        println!("participants: {}", report.participants.join(", "));
    }
    if !report.delivery_states.is_empty() {
        println!("delivery_states: {}", report.delivery_states.join(", "));
    }
    for warning in report.warnings {
        println!("warning: {}", warning.message);
    }
    if report.state == "not_found" {
        println!("empty: no evidence records matched the correlation ID");
    }
    for event in report.events {
        println!(
            "event: sequence={} kind={} message_id={} state={} exceptional={} relationship={}",
            event.daemon_sequence,
            event.kind,
            event.message_id,
            event.delivery_state,
            event.exceptional_state.unwrap_or("none"),
            event.relationship
        );
    }
    for gap in report.gaps {
        println!(
            "gap: {} message_id={} missing_parent_message_id={}",
            gap.code,
            gap.message_id,
            gap.missing_parent_message_id
                .as_deref()
                .unwrap_or("unavailable")
        );
    }
    if report.span_tree_requested {
        println!("span_tree: {}", report.span_tree.reconstruction);
        for node in &report.span_tree.nodes {
            let children = if node.child_message_ids.is_empty() {
                "none".to_owned()
            } else {
                node.child_message_ids.join(",")
            };
            let invalid_reasons = if node.invalid_reasons.is_empty() {
                "none".to_owned()
            } else {
                node.invalid_reasons.join(",")
            };
            println!(
                "span: message_id={} span_id={} parent={} relationship={} status={} depth={} children={} stream_sequence={} stream_state={} invalid_reasons={}",
                node.message_id,
                node.span_id,
                node.parent_message_id.as_deref().unwrap_or("none"),
                node.relationship,
                node.status,
                node.depth
                    .map(|depth| depth.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                children,
                node.stream_sequence
                    .map(|sequence| sequence.to_string())
                    .unwrap_or_else(|| "none".to_owned()),
                node.stream_state.unwrap_or("none"),
                invalid_reasons
            );
        }
    }
    if !report.next_actions.is_empty() {
        println!("next_actions: {}", report.next_actions.join(", "));
    }
    Ok(())
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
        zornmesh_daemon::DaemonErrorCode::PersistenceUnavailable => ExitKind::TemporaryUnavailable,
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

fn json_optional_string(value: Option<&str>) -> String {
    match value {
        Some(value) => json_string(value),
        None => "null".to_owned(),
    }
}

fn warnings_json(warnings: &[DiagnosticWarning]) -> String {
    let mut json = String::from("[");
    for (index, warning) in warnings.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str("{\"code\":");
        json.push_str(&json_string(warning.code));
        json.push_str(",\"message\":");
        json.push_str(&json_string(&warning.message));
        json.push('}');
    }
    json.push(']');
    json
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeMessage {
    Initialize { protocol_version: String },
    Request { method: String, params: String },
    HostClosed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineMcpTool {
    name: String,
    description: String,
}

impl BaselineMcpTool {
    fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeResponse {
    InitializeAck {
        protocol_version: String,
    },
    ToolList {
        tools: Vec<BaselineMcpTool>,
    },
    Mapped {
        correlation_id: String,
        trace_id: Option<String>,
        capability_id: Option<String>,
        capability_version: Option<String>,
        internal_operation: String,
        safe_params: String,
    },
    UnsupportedCapability {
        code: String,
        capability_id: Option<String>,
        capability_version: Option<String>,
        reason: String,
        remediation: String,
        safe_params: String,
    },
    Error(StdioBridgeError),
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeState {
    Pending,
    Initialized {
        agent_id: String,
        session_id: String,
    },
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdioBridgeErrorCode {
    OutOfSequence,
    AlreadyInitialized,
    UnsupportedProtocolVersion,
    MalformedInitialize,
    Closed,
    MalformedMessage,
    UnsupportedMapping,
    UnsupportedCapability,
    PolicyDenied,
    RegistrationFailed,
    SocketPermissionDenied,
    DaemonUnavailable,
}

impl StdioBridgeErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OutOfSequence => "E_BRIDGE_OUT_OF_SEQUENCE",
            Self::AlreadyInitialized => "E_BRIDGE_ALREADY_INITIALIZED",
            Self::UnsupportedProtocolVersion => "E_BRIDGE_UNSUPPORTED_PROTOCOL",
            Self::MalformedInitialize => "E_BRIDGE_MALFORMED_INITIALIZE",
            Self::Closed => "E_BRIDGE_CLOSED",
            Self::MalformedMessage => "E_BRIDGE_MALFORMED_MESSAGE",
            Self::UnsupportedMapping => "E_BRIDGE_UNSUPPORTED_MAPPING",
            Self::UnsupportedCapability => "E_BRIDGE_UNSUPPORTED_CAPABILITY",
            Self::PolicyDenied => "E_BRIDGE_POLICY_DENIED",
            Self::RegistrationFailed => "E_BRIDGE_REGISTRATION_FAILED",
            Self::SocketPermissionDenied => "E_BRIDGE_SOCKET_PERMISSION_DENIED",
            Self::DaemonUnavailable => "E_DAEMON_UNREACHABLE",
        }
    }

    pub const fn jsonrpc_code(self) -> i32 {
        match self {
            Self::MalformedMessage | Self::MalformedInitialize => -32602,
            Self::UnsupportedMapping => -32601,
            Self::OutOfSequence
            | Self::AlreadyInitialized
            | Self::UnsupportedProtocolVersion
            | Self::Closed
            | Self::UnsupportedCapability
            | Self::PolicyDenied
            | Self::RegistrationFailed
            | Self::SocketPermissionDenied
            | Self::DaemonUnavailable => -32000,
        }
    }

    pub const fn retryable(self) -> bool {
        matches!(self, Self::DaemonUnavailable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdioBridgeError {
    code: StdioBridgeErrorCode,
    safe_message: String,
}

impl StdioBridgeError {
    pub fn new(code: StdioBridgeErrorCode, safe_message: impl Into<String>) -> Self {
        Self {
            code,
            safe_message: safe_message.into(),
        }
    }

    pub const fn code(&self) -> StdioBridgeErrorCode {
        self.code
    }

    pub fn safe_message(&self) -> &str {
        &self.safe_message
    }
}

#[derive(Debug, Clone)]
pub struct StdioBridge {
    broker: zornmesh_broker::Broker,
    agent_id: String,
    display_name: String,
    credentials: zornmesh_broker::PeerCredentials,
    trust_policy: zornmesh_broker::SocketTrustPolicy,
    state: BridgeState,
}

impl StdioBridge {
    pub fn new(
        broker: zornmesh_broker::Broker,
        agent_id: impl Into<String>,
        display_name: impl Into<String>,
        credentials: zornmesh_broker::PeerCredentials,
        trust_policy: zornmesh_broker::SocketTrustPolicy,
    ) -> Self {
        Self {
            broker,
            agent_id: agent_id.into(),
            display_name: display_name.into(),
            credentials,
            trust_policy,
            state: BridgeState::Pending,
        }
    }

    pub fn state(&self) -> BridgeState {
        self.state.clone()
    }

    pub fn handle_message(&mut self, message: BridgeMessage) -> BridgeResponse {
        match (&self.state, message) {
            (BridgeState::Closed, BridgeMessage::HostClosed) => BridgeResponse::Closed,
            (BridgeState::Closed, _) => BridgeResponse::Error(StdioBridgeError::new(
                StdioBridgeErrorCode::Closed,
                "stdio bridge is closed",
            )),
            (BridgeState::Pending, BridgeMessage::HostClosed) => {
                self.state = BridgeState::Closed;
                BridgeResponse::Closed
            }
            (BridgeState::Pending, BridgeMessage::Request { .. }) => {
                BridgeResponse::Error(StdioBridgeError::new(
                    StdioBridgeErrorCode::OutOfSequence,
                    "MCP initialize must complete before mesh requests are dispatched",
                ))
            }
            (BridgeState::Pending, BridgeMessage::Initialize { protocol_version }) => {
                self.initialize(protocol_version)
            }
            (BridgeState::Initialized { .. }, BridgeMessage::Initialize { .. }) => {
                BridgeResponse::Error(StdioBridgeError::new(
                    StdioBridgeErrorCode::AlreadyInitialized,
                    "MCP initialize has already completed for this stdio bridge",
                ))
            }
            (
                BridgeState::Initialized {
                    agent_id,
                    session_id,
                },
                BridgeMessage::HostClosed,
            ) => {
                self.broker.record_session_disconnect(agent_id, session_id);
                self.state = BridgeState::Closed;
                BridgeResponse::Closed
            }
            (
                BridgeState::Initialized { agent_id, .. },
                BridgeMessage::Request { method, params },
            ) => {
                let agent_id = agent_id.clone();
                self.map_request(&agent_id, method, params)
            }
        }
    }

    fn initialize(&mut self, protocol_version: String) -> BridgeResponse {
        if protocol_version.trim().is_empty() {
            return BridgeResponse::Error(StdioBridgeError::new(
                StdioBridgeErrorCode::MalformedInitialize,
                "MCP initialize requires protocolVersion",
            ));
        }
        if protocol_version != MCP_BRIDGE_PROTOCOL_VERSION {
            return BridgeResponse::Error(StdioBridgeError::new(
                StdioBridgeErrorCode::UnsupportedProtocolVersion,
                format!("unsupported MCP protocolVersion '{protocol_version}'"),
            ));
        }

        match self.register_identity_and_session() {
            Ok((agent_id, session_id)) => {
                self.state = BridgeState::Initialized {
                    agent_id,
                    session_id,
                };
                BridgeResponse::InitializeAck { protocol_version }
            }
            Err(error) => BridgeResponse::Error(error),
        }
    }

    fn register_identity_and_session(&self) -> Result<(String, String), StdioBridgeError> {
        let card = zornmesh_core::AgentCard::from_input(zornmesh_core::AgentCardInput {
            profile_version: zornmesh_core::AGENT_CARD_PROFILE_VERSION.to_owned(),
            stable_id: self.agent_id.clone(),
            display_name: self.display_name.clone(),
            transport: "unix".to_owned(),
            source: "zornmesh stdio --as-agent".to_owned(),
        })
        .map_err(|error| {
            StdioBridgeError::new(
                StdioBridgeErrorCode::RegistrationFailed,
                format!(
                    "AgentCard registration failed with {}",
                    error.code().as_str()
                ),
            )
        })?;

        let canonical = match self.broker.register_agent_card(card).map_err(|error| {
            StdioBridgeError::new(
                StdioBridgeErrorCode::RegistrationFailed,
                format!("AgentCard registration failed: {error}"),
            )
        })? {
            zornmesh_broker::AgentRegistrationOutcome::Registered { canonical }
            | zornmesh_broker::AgentRegistrationOutcome::Compatible { canonical } => canonical,
            zornmesh_broker::AgentRegistrationOutcome::Conflict { .. } => {
                return Err(StdioBridgeError::new(
                    StdioBridgeErrorCode::RegistrationFailed,
                    "AgentCard conflicts with an existing mesh identity",
                ));
            }
        };
        let canonical_id = canonical.canonical_stable_id().to_owned();

        match self.broker.accept_connection(
            &canonical_id,
            self.credentials.clone(),
            self.trust_policy,
            self.trust_policy.expected_mode(),
        ) {
            Ok(zornmesh_broker::ConnectionAcceptanceOutcome::Accepted { .. }) => {}
            Ok(zornmesh_broker::ConnectionAcceptanceOutcome::Rejected { code, remediation }) => {
                return Err(StdioBridgeError::new(
                    StdioBridgeErrorCode::SocketPermissionDenied,
                    format!("{}: {remediation}", code.as_str()),
                ));
            }
            Err(error) => {
                return Err(StdioBridgeError::new(
                    StdioBridgeErrorCode::SocketPermissionDenied,
                    format!("socket permission validation failed: {error}"),
                ));
            }
        }

        self.declare_bridge_capabilities(&canonical_id)?;
        let session_id = self
            .broker
            .routing_session(&canonical_id)
            .map(|session| session.session_id().to_owned())
            .ok_or_else(|| {
                StdioBridgeError::new(
                    StdioBridgeErrorCode::RegistrationFailed,
                    "mesh session was not recorded after bridge initialization",
                )
            })?;
        Ok((canonical_id, session_id))
    }

    fn declare_bridge_capabilities(&self, agent_id: &str) -> Result<(), StdioBridgeError> {
        let descriptors = [
            ("mcp.ping", "MCP ping bridge operation"),
            ("mcp.tools.list", "MCP tools/list bridge operation"),
            ("mcp.tools.call", "MCP tools/call bridge operation"),
        ]
        .into_iter()
        .map(|(capability_id, summary)| {
            zornmesh_core::CapabilityDescriptor::builder(
                capability_id,
                "v1",
                zornmesh_core::CapabilityDirection::Both,
            )
            .with_summary(summary)
            .with_schema_ref(
                zornmesh_core::CapabilitySchemaDialect::JsonSchema,
                format!("{capability_id}.v1.schema"),
            )
            .build()
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            StdioBridgeError::new(
                StdioBridgeErrorCode::RegistrationFailed,
                format!(
                    "bridge capability declaration failed with {}",
                    error.code().as_str()
                ),
            )
        })?;

        self.broker
            .declare_capabilities(agent_id, descriptors)
            .map(|_| ())
            .map_err(|error| {
                StdioBridgeError::new(
                    StdioBridgeErrorCode::RegistrationFailed,
                    format!(
                        "bridge capability declaration failed with {}",
                        error.code().as_str()
                    ),
                )
            })
    }

    fn map_request(&self, agent_id: &str, method: String, params: String) -> BridgeResponse {
        if !matches!(method.as_str(), "ping" | "tools/list" | "tools/call") {
            return BridgeResponse::Error(StdioBridgeError::new(
                StdioBridgeErrorCode::UnsupportedMapping,
                format!("MCP method '{method}' cannot be mapped to a mesh operation"),
            ));
        }
        let params_value = match parse_params(&params) {
            Ok(value) => value,
            Err(error) => return BridgeResponse::Error(error),
        };
        let correlation_id = string_field(&params_value, "correlation_id")
            .or_else(|| string_field(&params_value, "correlationId"))
            .unwrap_or_else(next_bridge_correlation_id);
        let trace_id = string_field(&params_value, "trace_id")
            .or_else(|| string_field(&params_value, "traceId"));
        let capability_id = string_field(&params_value, "capability_id")
            .or_else(|| string_field(&params_value, "capabilityId"));
        let capability_version = string_field(&params_value, "capability_version")
            .or_else(|| string_field(&params_value, "capabilityVersion"));

        if method == "tools/list" {
            return BridgeResponse::ToolList {
                tools: baseline_mcp_tools(),
            };
        }

        if method == "tools/call"
            && let Some(capability_id) = capability_id.as_deref()
        {
            let version = capability_version.as_deref().unwrap_or("v1");
            if let zornmesh_broker::AuthorizationDecision::Denied { reason } = self
                .broker
                .authorize_invocation(agent_id, capability_id, version)
            {
                return BridgeResponse::Error(StdioBridgeError::new(
                    StdioBridgeErrorCode::PolicyDenied,
                    format!(
                        "capability invocation denied by local policy: {}",
                        reason.as_str()
                    ),
                ));
            }
        }

        if method == "tools/call"
            && let Some(limitation) = baseline_mcp_limitation(&params_value)
        {
            return BridgeResponse::UnsupportedCapability {
                code: StdioBridgeErrorCode::UnsupportedCapability
                    .as_str()
                    .to_owned(),
                capability_id,
                capability_version,
                reason: limitation.reason().to_owned(),
                remediation: limitation.remediation().to_owned(),
                safe_params: redacted_json_string(params_value),
            };
        }

        BridgeResponse::Mapped {
            correlation_id,
            trace_id,
            capability_id,
            capability_version,
            internal_operation: method,
            safe_params: redacted_json_string(params_value),
        }
    }
}

fn baseline_mcp_tools() -> Vec<BaselineMcpTool> {
    vec![BaselineMcpTool::new(
        "zornmesh.call_capability",
        "Invoke mesh capabilities that fit baseline MCP unary JSON tools/call semantics; streaming, delivery ACK, and required trace-context semantics return unsupported_capability.",
    )]
}

fn baseline_mcp_tool_json(tool: &BaselineMcpTool) -> serde_json::Value {
    serde_json::json!({
        "name": tool.name(),
        "description": tool.description(),
        "inputSchema": {
            "type": "object",
            "additionalProperties": true,
            "properties": {
                "capability_id": {
                    "type": "string",
                    "description": "Mesh capability identifier to invoke."
                },
                "capability_version": {
                    "type": "string",
                    "description": "Mesh capability version; defaults to v1 when omitted."
                },
                "correlation_id": {
                    "type": "string",
                    "description": "Optional caller-supplied correlation identifier."
                },
                "trace_id": {
                    "type": "string",
                    "description": "Optional trace identifier carried as best-effort metadata."
                }
            }
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BaselineMcpLimitation {
    Streaming,
    DeliveryAck,
    TraceContext,
}

impl BaselineMcpLimitation {
    const fn reason(self) -> &'static str {
        match self {
            Self::Streaming => {
                "streaming semantics are not representable on baseline MCP tools/call"
            }
            Self::DeliveryAck => {
                "delivery_ack semantics are not representable on baseline MCP tools/call"
            }
            Self::TraceContext => "trace_context propagation is limited on baseline MCP tools/call",
        }
    }

    const fn remediation(self) -> &'static str {
        match self {
            Self::Streaming => {
                "Use the zornmesh CLI or Rust/TypeScript SDK for streaming mesh capabilities; baseline MCP tools/call supports only unary JSON calls."
            }
            Self::DeliveryAck => {
                "Use the zornmesh CLI or Rust/TypeScript SDK when delivery ACK semantics are required; baseline MCP tools/call cannot confirm mesh delivery state."
            }
            Self::TraceContext => {
                "Use the zornmesh CLI or Rust/TypeScript SDK when W3C tracecontext continuity is required; baseline MCP tools/call carries trace metadata only as best-effort fields."
            }
        }
    }
}

fn baseline_mcp_limitation(params: &serde_json::Value) -> Option<BaselineMcpLimitation> {
    if bool_field(params, "requires_streaming")
        || bool_field(params, "requiresStreaming")
        || semantic_requirement_present(params, &["streaming", "stream"])
    {
        return Some(BaselineMcpLimitation::Streaming);
    }
    if bool_field(params, "requires_delivery_ack")
        || bool_field(params, "requiresDeliveryAck")
        || semantic_requirement_present(params, &["delivery_ack", "deliveryack", "ack"])
    {
        return Some(BaselineMcpLimitation::DeliveryAck);
    }
    if bool_field(params, "requires_trace_context")
        || bool_field(params, "requiresTraceContext")
        || semantic_requirement_present(params, &["trace_context", "tracecontext"])
    {
        return Some(BaselineMcpLimitation::TraceContext);
    }
    None
}

fn bool_field(value: &serde_json::Value, field: &str) -> bool {
    value
        .get(field)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn semantic_requirement_present(value: &serde_json::Value, needles: &[&str]) -> bool {
    [
        "semantic_requirements",
        "semanticRequirements",
        "required_semantics",
        "requiredSemantics",
    ]
    .iter()
    .filter_map(|field| value.get(field))
    .any(|requirements| semantic_value_matches(requirements, needles))
}

fn semantic_value_matches(value: &serde_json::Value, needles: &[&str]) -> bool {
    match value {
        serde_json::Value::Array(values) => values
            .iter()
            .any(|value| semantic_value_matches(value, needles)),
        serde_json::Value::String(value) => {
            let normalized = normalize_semantic_requirement(value);
            needles.iter().any(|needle| normalized == *needle)
        }
        _ => false,
    }
}

fn normalize_semantic_requirement(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '.', ' '], "_")
}

fn parse_params(params: &str) -> Result<serde_json::Value, StdioBridgeError> {
    if params.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(params).map_err(|_| {
        StdioBridgeError::new(
            StdioBridgeErrorCode::MalformedMessage,
            "MCP request params must be valid JSON",
        )
    })
}

fn string_field(value: &serde_json::Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
}

fn next_bridge_correlation_id() -> String {
    format!(
        "bridge-corr-{}",
        NEXT_BRIDGE_CORRELATION_ID.fetch_add(1, Ordering::Relaxed)
    )
}

fn redacted_json_string(mut value: serde_json::Value) -> String {
    redact_value(&mut value);
    value.to_string()
}

fn redact_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if is_secret_field(key) {
                    *value = serde_json::Value::String(zornmesh_core::REDACTION_MARKER.to_owned());
                } else {
                    redact_value(value);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_value(item);
            }
        }
        _ => {}
    }
}

fn is_secret_field(field: &str) -> bool {
    let normalized = field.to_ascii_lowercase();
    normalized == "secret"
        || normalized == "password"
        || normalized == "token"
        || normalized == "api_key"
        || normalized == "apikey"
        || normalized.ends_with("_secret")
        || normalized.ends_with("_password")
        || normalized.ends_with("_token")
}
