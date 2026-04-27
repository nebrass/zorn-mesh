#![doc = "Command skeleton for the public zornmesh CLI."]

use std::{
    fs,
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

pub const ROOT_HELP: &str = include_str!("../../../fixtures/cli/root-help.stdout");
pub const DAEMON_HELP: &str = include_str!("../../../fixtures/cli/daemon-help.stdout");
pub const TRACE_HELP: &str = include_str!("../../../fixtures/cli/trace-help.stdout");
pub const VERSION: &str = "zornmesh 0.1.0\n";
const READ_SCHEMA_VERSION: &str = "zornmesh.cli.read.v1";
const EVENT_SCHEMA_VERSION: &str = "zornmesh.cli.event.v1";
const DOCTOR_SCHEMA_VERSION: &str = "zornmesh.cli.doctor.v1";
const CLI_VERSION: &str = "0.1.0";
const DEFAULT_SHUTDOWN_BUDGET_MS: u64 = 10_000;
const MAX_SHUTDOWN_BUDGET_MS: u64 = 60_000;
const SUPPORTED_SHELLS: &str = "bash, zsh, fish";
const BASH_COMPLETION: &str = r#"_zornmesh()
{
    local cur prev commands global_opts daemon_commands shells formats
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    commands="daemon doctor agents trace completion help"
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
  commands=(daemon doctor agents trace completion help)
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
const FISH_COMPLETION: &str = r#"complete -c zornmesh -f -a "daemon doctor agents trace completion help"
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
        [command] if command == "doctor" => run_doctor(&[], &invocation),
        [command, rest @ ..] if command == "doctor" => run_doctor(rest, &invocation),
        [command] if command == "completion" => print_completion_help(invocation.output),
        [command, rest @ ..] if command == "completion" => run_completion(rest, &invocation),
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
