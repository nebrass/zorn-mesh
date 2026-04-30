#![doc = "Command skeleton for the public zornmesh CLI."]

pub mod core;
pub mod proto;
pub mod store;
pub mod rpc;
pub mod broker;
pub mod daemon;
pub mod sdk;
pub mod debate;

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    io::{self, BufRead, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::UnixStream,
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use crate::broker::SubjectPattern;
use crate::store::{
    DeadLetterFailureCategory, DeadLetterQuery, EvidenceAuditEntry, EvidenceEnvelopeRecord,
    EvidenceQuery, EvidenceStateTransitionInput, EvidenceStore, EvidenceStoreErrorCode,
    FileEvidenceStore, RetentionPolicy, RetentionReport,
};

pub const ROOT_HELP: &str = include_str!("../fixtures/root-help.stdout");
pub const DAEMON_HELP: &str = include_str!("../fixtures/daemon-help.stdout");
pub const TRACE_HELP: &str = include_str!("../fixtures/trace-help.stdout");
pub const TAIL_HELP: &str = include_str!("../fixtures/tail-help.stdout");
pub const REPLAY_HELP: &str = "zornmesh replay\nRedeliver a previously sent envelope by message ID.\n\nUsage: zornmesh replay <MESSAGE_ID> [OPTIONS]\n\nOptions:\n      --evidence <PATH>            Read this evidence log\n      --preview                    Emit a preview without delivery side effect\n      --yes                        Confirm replay without preview\n      --confirmation-token <TOKEN> Confirm a previously previewed replay\n      --output <FORMAT>            Select human or json output\n  -h, --help                       Print help\n";
pub const UI_HELP: &str = "zornmesh ui\nLaunch the protected loopback local UI session.\n\nUsage: zornmesh ui [OPTIONS]\n\nOptions:\n      --port <PORT>     Preferred loopback port (default 7878)\n      --no-open         Do not open a browser; only print the URL\n      --output <FORMAT> Select human or json output\n  -h, --help            Print help\n\nThe local UI server binds loopback only, mints a per-launch high-entropy\nsession token, enforces CORS to the exact loopback origin, requires\nCSRF protection on state-changing requests, and serves only bundled\nlocal assets. Token-bearing material never appears in logs, audit\npayloads, or CLI handoff text.\n";
pub const UI_PREFERRED_PORT: u16 = 7878;
pub const UI_FALLBACK_PORTS: &[u16] = &[7879, 7880];
pub const UI_SCHEMA_VERSION: &str = "zornmesh.cli.ui.v1";
pub const UI_REFERRER_POLICY: &str = "no-referrer";
pub const UI_TOKEN_HEX_LEN: usize = 64;
pub const AIRMF_HELP: &str = "zornmesh airmf\nMap evidence records to NIST AI RMF functions and categories.\n\nUsage: zornmesh airmf map [OPTIONS]\n\nOptions:\n      --evidence <PATH>          Read this evidence log\n      --correlation-id <ID>      Restrict the report to one correlation ID\n      --output <FORMAT>          Select human or json output\n  -h, --help                     Print help\n\nThe report classifies each evidence record under one of the AI RMF\nGovern, Map, Measure, or Manage functions, with a subcategory and\ncontrol reference where the mapping definition supports one. Records\nthat the mapping table does not cover are included with an explicit\nautomatic_mapping=unmapped flag rather than silently omitted; records\nlacking required metadata carry an evidence-gap reason. Every report\npins the mapping-definition version so prior fixtures stay\nreproducible across mapping updates.\n";
const AIRMF_MAPPING_DEFINITION_VERSION: &str = "nist.ai-rmf.v1.0";
const AIRMF_REPORT_SCHEMA_VERSION: &str = "zornmesh.airmf.report.v1";
pub const REDACT_HELP: &str = "zornmesh redact\nApply append-only redaction proofs while preserving audit integrity.\n\nUsage: zornmesh redact apply --message-id <ID> --actor <ID> --policy-version <VERSION> --reason <TEXT> [OPTIONS]\n\nOptions:\n      --evidence <PATH>             Read this evidence log\n      --message-id <ID>             Scope redaction to this message\n      --actor <ID>                  Operator/actor identity authorising the redaction\n      --policy-version <VERSION>    Privacy/redaction policy version reference\n      --reason <TEXT>               Documented reason for the redaction\n      --preview                     Emit a preview without persisting evidence\n      --yes                         Confirm redaction without preview\n      --confirmation-token <TOKEN>  Confirm a previously previewed redaction\n      --output <FORMAT>             Select human or json output\n  -h, --help                        Print help\n\nA committed redaction appends a `redaction_applied` audit transition\ncarrying actor, policy version, reason, redaction scope, original\naudit hash anchors, and the daemon-sequence checkpoint. Existing\naudit-chain entries and prior hashes are never rewritten, deleted,\nor re-linked.\n";
pub const EVIDENCE_HELP: &str = "zornmesh evidence\nExport self-contained evidence bundles for a time window.\n\nUsage: zornmesh evidence export [OPTIONS]\n\nOptions:\n      --evidence <PATH>          Read this evidence log\n      --release-manifest <PATH>  Include this release manifest in the bundle\n      --since <UNIX_MS>          Lower bound (inclusive) of the time window\n      --until <UNIX_MS>          Upper bound (inclusive) of the time window\n      --output <FORMAT>          Select human or json output\n  -h, --help                     Print help\n\nExports a single self-contained JSON bundle containing the audit-log\nslice, envelope and dead-letter records, release evidence (where\navailable), and a manifest enumerating included sections plus any\nevidence gaps. Raw secrets are never emitted; redaction markers remain\nvisible. Stable structured errors prevent partial bundles from being\nreported as complete.\n";
pub const COMPLIANCE_HELP: &str = "zornmesh compliance\nAudit compliance traceability fields on evidence records.\n\nUsage: zornmesh compliance traceability [OPTIONS]\n\nOptions:\n      --evidence <PATH>           Read this evidence log\n      --correlation-id <ID>       Restrict to one correlation ID\n      --output <FORMAT>           Select human or json output\n  -h, --help                      Print help\n\nClassifies each persisted record as complete, partial, or evidence_gap\nbased on the AC-required traceability fields (agent identity, capability\nor subject, timestamp, correlation ID, trace ID, prior-message lineage).\nRedaction markers preserve compliance status; missing required fields\nproduce explicit evidence-gap reasons rather than silent completeness.\n";
pub const RELEASE_HELP: &str = "zornmesh release\nVerify release signatures and inspect SBOM evidence.\n\nUsage: zornmesh release verify [OPTIONS]\n       zornmesh release sbom   [OPTIONS]\n\nOptions:\n      --manifest <PATH>  Read this release evidence manifest\n      --output <FORMAT>  Select human or json output\n  -h, --help            Print help\n\nVerification reads only local manifest evidence; no network or remote\ntrust decision is performed unless an operator explicitly configures\none. The manifest path may also be set via ZORN_RELEASE_MANIFEST.\n";
const ENV_RELEASE_MANIFEST: &str = "ZORN_RELEASE_MANIFEST";
pub const AUDIT_HELP: &str = "zornmesh audit\nVerify the audit log hash chain offline.\n\nUsage: zornmesh audit verify [OPTIONS]\n\nOptions:\n      --evidence <PATH>  Read this evidence log\n      --output <FORMAT>  Select human or json output\n  -h, --help            Print help\n\nVerification walks the on-disk hash chain without requiring a running\ndaemon, distinguishing valid, tampered, missing, unreadable, and\nunsupported-schema results with stable exit codes.\n";
pub const RETENTION_HELP: &str = "zornmesh retention\nPlan retention purges and surface retention gaps.\n\nUsage: zornmesh retention plan [OPTIONS]\n\nOptions:\n      --evidence <PATH>     Read this evidence log\n      --max-age-ms <MS>     Mark records older than this age as purgeable\n      --max-count <N>       Mark all but the most recent N envelopes as purgeable\n      --now-unix-ms <MS>    Override the current time (defaults to wall clock)\n      --output <FORMAT>     Select human or json output\n  -h, --help                Print help\n\nThe plan subcommand never mutates the evidence log; it computes the records\nthat would be purged under the configured policy and reports retention\ncheckpoint metadata that downstream tooling can verify offline.\n";
pub const VERSION: &str = concat!("zornmesh ", env!("CARGO_PKG_VERSION"), "\n");
const READ_SCHEMA_VERSION: &str = "zornmesh.cli.read.v1";
const EVENT_SCHEMA_VERSION: &str = "zornmesh.cli.event.v1";
const DOCTOR_SCHEMA_VERSION: &str = "zornmesh.cli.doctor.v1";
const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");
/// MCP protocol versions this bridge accepts in `initialize`. The first entry
/// is the canonical / preferred version; additional entries support older or
/// newer clients negotiating the same wire shape. Add new versions to the
/// front as MCP hosts (Claude Code, Cursor, Windsurf, …) roll forward.
pub const MCP_BRIDGE_PROTOCOL_VERSIONS: &[&str] =
    &["2025-11-25", "2025-06-18", "2025-03-26"];

/// Preferred protocol version for any caller that needs one canonical value
/// (tests, fixtures). Always equal to `MCP_BRIDGE_PROTOCOL_VERSIONS[0]`.
pub const MCP_BRIDGE_PROTOCOL_VERSION: &str = MCP_BRIDGE_PROTOCOL_VERSIONS[0];
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
    commands="daemon doctor agents stdio inspect trace tail replay retention audit release compliance evidence redact airmf ui service worker debate completion help"
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
  commands=(daemon doctor agents stdio inspect trace tail replay retention audit release compliance evidence redact airmf ui service worker debate completion help)
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
const FISH_COMPLETION: &str = r#"complete -c zornmesh -f -a "daemon doctor agents stdio inspect trace tail replay retention audit release compliance evidence redact airmf ui service worker debate completion help"
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
        crate::rpc::local::default_socket_path().map_err(cli_error_from_local)?;
    let mut socket_source = ValueSource::Default;

    if let Some(path) = config_path
        && let Some(config_socket) = read_config_socket_path(path)?
    {
        socket_path = config_socket;
        socket_source = ValueSource::Config;
    }

    if let Some(env_socket) = std::env::var_os(crate::rpc::local::ENV_SOCKET_PATH) {
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

fn cli_error_from_local(error: crate::rpc::local::LocalError) -> CliError {
    let kind = match error.code() {
        crate::rpc::local::LocalErrorCode::ExistingOwner => ExitKind::TemporaryUnavailable,
        crate::rpc::local::LocalErrorCode::LocalTrustUnsafe
        | crate::rpc::local::LocalErrorCode::ElevatedPrivilege => ExitKind::PermissionDenied,
        crate::rpc::local::LocalErrorCode::DaemonUnreachable => ExitKind::DaemonUnreachable,
        crate::rpc::local::LocalErrorCode::InvalidConfig => ExitKind::UserError,
        crate::rpc::local::LocalErrorCode::Io => ExitKind::Io,
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
        [command] if command == "retention" => {
            print_help("retention help", RETENTION_HELP, invocation.output)
        }
        [command, rest @ ..] if command == "retention" => run_retention(rest, &invocation),
        [command] if command == "audit" => print_help("audit help", AUDIT_HELP, invocation.output),
        [command, rest @ ..] if command == "audit" => run_audit(rest, &invocation),
        [command] if command == "release" => {
            print_help("release help", RELEASE_HELP, invocation.output)
        }
        [command, rest @ ..] if command == "release" => run_release(rest, &invocation),
        [command] if command == "compliance" => {
            print_help("compliance help", COMPLIANCE_HELP, invocation.output)
        }
        [command, rest @ ..] if command == "compliance" => run_compliance(rest, &invocation),
        [command] if command == "evidence" => {
            print_help("evidence help", EVIDENCE_HELP, invocation.output)
        }
        [command, rest @ ..] if command == "evidence" => run_evidence(rest, &invocation),
        [command] if command == "redact" => {
            print_help("redact help", REDACT_HELP, invocation.output)
        }
        [command, rest @ ..] if command == "redact" => run_redact(rest, &invocation),
        [command] if command == "airmf" => print_help("airmf help", AIRMF_HELP, invocation.output),
        [command, rest @ ..] if command == "airmf" => run_airmf(rest, &invocation),
        [command] if command == "ui" => run_ui(&[], &invocation),
        [command, rest @ ..] if command == "ui" => run_ui(rest, &invocation),
        [command] if command == "service" => run_service(&[], &invocation),
        [command, rest @ ..] if command == "service" => run_service(rest, &invocation),
        [command] if command == "worker" => run_worker(&[], &invocation),
        [command, rest @ ..] if command == "worker" => run_worker(rest, &invocation),
        [command] if command == "debate" => run_debate(&[], &invocation),
        [command, rest @ ..] if command == "debate" => run_debate(rest, &invocation),
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
            let config = crate::daemon::DaemonConfig::for_socket_path(
                invocation.config.socket_path.clone(),
            );
            match crate::daemon::run_foreground(config) {
                Ok(report) => match report.outcome {
                    crate::daemon::ShutdownOutcome::Clean => Ok(()),
                    crate::daemon::ShutdownOutcome::BudgetExceeded => Err(CliError::new(
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
    match std::env::var(crate::rpc::local::ENV_SHUTDOWN_BUDGET_MS) {
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
            "schema_version": crate::store::EVIDENCE_STORE_SCHEMA_VERSION,
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
            "schema_version": store.map(|_| crate::store::EVIDENCE_STORE_SCHEMA_VERSION),
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

    let broker = crate::broker::Broker::new();
    let credentials = crate::broker::PeerCredentials::new(uid, uid, std::process::id());
    let policy = crate::broker::SocketTrustPolicy::new(uid, uid, 0o600);
    let mut bridge = StdioBridge::new(broker, agent_id, "MCP Host", credentials, policy);
    let stdin = io::stdin();
    run_stdio_loop(stdin.lock(), stdout, &mut bridge).map_err(stdio_io_error)
}

/// Default budget for the autospawned daemon to publish its readiness line.
/// Override with `ZORN_AUTOSPAWN_TIMEOUT_MS`.
const DEFAULT_AUTOSPAWN_TIMEOUT_MS: u64 = 5_000;
const ENV_AUTOSPAWN_TIMEOUT: &str = "ZORN_AUTOSPAWN_TIMEOUT_MS";
const ENV_NO_AUTOSPAWN: &str = "ZORN_NO_AUTOSPAWN";

fn connect_stdio_daemon(socket_path: &Path) -> Result<(u32, UnixStream), StdioBridgeError> {
    let uid = crate::rpc::local::effective_uid().map_err(stdio_daemon_error_from_local)?;

    match crate::rpc::local::connect_trusted_socket(socket_path, uid) {
        Ok(stream) => return Ok((uid, stream)),
        Err(error)
            if error.code() == crate::rpc::local::LocalErrorCode::DaemonUnreachable =>
        {
            if autospawn_disabled() {
                return Err(stdio_daemon_error_from_local(error));
            }
        }
        Err(error) => return Err(stdio_daemon_error_from_local(error)),
    }

    spawn_daemon_subprocess(socket_path)?;
    wait_for_daemon_socket(socket_path, uid, autospawn_timeout())
}

fn autospawn_disabled() -> bool {
    std::env::var(ENV_NO_AUTOSPAWN)
        .map(|value| {
            matches!(
                value.trim(),
                "1" | "true" | "TRUE" | "True" | "yes" | "YES" | "Yes"
            )
        })
        .unwrap_or(false)
}

fn autospawn_timeout() -> Duration {
    let millis = std::env::var(ENV_AUTOSPAWN_TIMEOUT)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_AUTOSPAWN_TIMEOUT_MS);
    Duration::from_millis(millis)
}

fn spawn_daemon_subprocess(socket_path: &Path) -> Result<(), StdioBridgeError> {
    let exe = std::env::current_exe().map_err(|error| {
        StdioBridgeError::new(
            StdioBridgeErrorCode::DaemonUnavailable,
            format!("cannot resolve zornmesh executable for autospawn: {error}"),
        )
    })?;

    // process_group(0) detaches the daemon from the bridge's process group so
    // the daemon survives when the MCP host terminates the stdio process.
    let mut command = Command::new(exe);
    command
        .arg("daemon")
        .arg("--socket")
        .arg(socket_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);

    command.spawn().map(|_child| ()).map_err(|error| {
        StdioBridgeError::new(
            StdioBridgeErrorCode::DaemonUnavailable,
            format!("autospawn failed to launch zornmesh daemon: {error}"),
        )
    })
}

fn wait_for_daemon_socket(
    socket_path: &Path,
    uid: u32,
    timeout: Duration,
) -> Result<(u32, UnixStream), StdioBridgeError> {
    let deadline = Instant::now() + timeout;
    let poll_interval = Duration::from_millis(50);
    loop {
        match crate::rpc::local::connect_trusted_socket(socket_path, uid) {
            Ok(stream) => return Ok((uid, stream)),
            Err(error) => {
                if Instant::now() >= deadline {
                    return Err(stdio_daemon_readiness_timeout(error, timeout));
                }
            }
        }
        thread::sleep(poll_interval);
    }
}

fn stdio_daemon_error_from_local(error: crate::rpc::local::LocalError) -> StdioBridgeError {
    StdioBridgeError::new(
        StdioBridgeErrorCode::DaemonUnavailable,
        format!("daemon connection failed with {}", error.code().as_str()),
    )
}

fn stdio_daemon_readiness_timeout(
    error: crate::rpc::local::LocalError,
    timeout: Duration,
) -> StdioBridgeError {
    StdioBridgeError::new(
        StdioBridgeErrorCode::DaemonUnavailable,
        format!(
            "autospawned daemon did not become ready within {}ms ({}); set ZORN_NO_AUTOSPAWN=1 and start `zornmesh daemon` manually, or run `zornmesh service install`",
            timeout.as_millis(),
            error.code().as_str()
        ),
    )
}

fn stdio_io_error(error: io::Error) -> CliError {
    CliError::new(
        "E_DAEMON_IO",
        format!("stdio bridge I/O failed: {error}"),
        ExitKind::Io,
    )
}

pub const SERVICE_HELP: &str = "zornmesh service\nManage the per-user zornmesh daemon as a supervised background service.\n\nUsage: zornmesh service <install|uninstall|status>\n\nSubcommands:\n      install     Write the launchd plist (macOS) or systemd user unit (Linux)\n      uninstall   Remove the previously installed unit file\n      status      Report install state and daemon reachability\n  -h, --help     Print help\n\nThe install subcommand writes only the unit file. It prints the exact\nactivation command (launchctl bootstrap or systemctl --user enable) so\noperators can audit the change before it takes effect. Running as root\nis refused; the unit is per-user and points at the current binary.\n";

const SERVICE_LABEL: &str = "dev.zornmesh.daemon";

fn run_service(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_service_help(invocation.output),
        [flag] if is_help(flag) => print_service_help(invocation.output),
        [sub] if sub == "install" => service_install(invocation),
        [sub] if sub == "uninstall" => service_uninstall(invocation),
        [sub] if sub == "status" => service_status(invocation),
        [sub, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh service subcommand '{sub}'"),
            ExitKind::UserError,
        )),
    }
}

fn print_service_help(output: OutputFormat) -> Result<(), CliError> {
    print_help("service help", SERVICE_HELP, output)
}

fn service_refuse_root() -> Result<(), CliError> {
    if crate::rpc::local::effective_uid()
        .map(|uid| uid == 0)
        .unwrap_or(false)
    {
        return Err(CliError::new(
            "E_ELEVATED_PRIVILEGE",
            "zornmesh service must be installed as a regular user, not root",
            ExitKind::PermissionDenied,
        ));
    }
    Ok(())
}

fn service_unit_path() -> Result<PathBuf, CliError> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        CliError::new(
            "E_VALIDATION_FAILED",
            "HOME is unset; cannot resolve per-user service unit path",
            ExitKind::Validation,
        )
    })?;
    let home = PathBuf::from(home);
    if cfg!(target_os = "macos") {
        Ok(home
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{SERVICE_LABEL}.plist")))
    } else if cfg!(target_os = "linux") {
        Ok(home
            .join(".config")
            .join("systemd")
            .join("user")
            .join("zornmesh.service"))
    } else {
        Err(CliError::new(
            "E_UNSUPPORTED_PLATFORM",
            "zornmesh service is only supported on macOS and Linux",
            ExitKind::UserError,
        ))
    }
}

fn render_service_unit(exe: &Path, socket_path: &Path) -> String {
    if cfg!(target_os = "macos") {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>daemon</string>
        <string>--socket</string>
        <string>{socket}</string>
    </array>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><true/>
    <key>ProcessType</key><string>Background</string>
    <key>StandardOutPath</key><string>/tmp/{label}.out.log</string>
    <key>StandardErrorPath</key><string>/tmp/{label}.err.log</string>
</dict>
</plist>
"#,
            label = SERVICE_LABEL,
            exe = exe.display(),
            socket = socket_path.display()
        )
    } else {
        format!(
            "[Unit]
Description=zornmesh local-first agent mesh daemon
After=default.target

[Service]
Type=simple
ExecStart={exe} daemon --socket {socket}
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
",
            exe = exe.display(),
            socket = socket_path.display()
        )
    }
}

fn service_activation_hint(unit_path: &Path) -> String {
    if cfg!(target_os = "macos") {
        format!(
            "next: launchctl bootstrap gui/$(id -u) {}",
            unit_path.display()
        )
    } else {
        "next: systemctl --user daemon-reload && systemctl --user enable --now zornmesh.service"
            .to_owned()
    }
}

fn service_deactivation_hint(unit_path: &Path) -> String {
    if cfg!(target_os = "macos") {
        format!(
            "next: launchctl bootout gui/$(id -u) {}",
            unit_path.display()
        )
    } else {
        "next: systemctl --user disable --now zornmesh.service".to_owned()
    }
}

fn service_install(invocation: &Invocation) -> Result<(), CliError> {
    service_refuse_root()?;
    let unit_path = service_unit_path()?;
    let exe = std::env::current_exe().map_err(|error| {
        CliError::new(
            "E_VALIDATION_FAILED",
            format!("cannot resolve zornmesh executable: {error}"),
            ExitKind::Validation,
        )
    })?;
    let socket_path = &invocation.config.socket_path;
    let unit = render_service_unit(&exe, socket_path);

    if let Some(parent) = unit_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::new(
                "E_DAEMON_IO",
                format!("cannot create {}: {error}", parent.display()),
                ExitKind::Io,
            )
        })?;
    }

    let already_present = match fs::read_to_string(&unit_path) {
        Ok(existing) if existing == unit => true,
        Ok(_) | Err(_) => false,
    };

    if !already_present {
        fs::write(&unit_path, &unit).map_err(|error| {
            CliError::new(
                "E_DAEMON_IO",
                format!("cannot write {}: {error}", unit_path.display()),
                ExitKind::Io,
            )
        })?;
    }

    let action = if already_present {
        "unchanged"
    } else {
        "installed"
    };
    let hint = service_activation_hint(&unit_path);
    match invocation.output {
        OutputFormat::Human => {
            println!("zornmesh service {action} at {}", unit_path.display());
            println!("{hint}");
        }
        OutputFormat::Json | OutputFormat::Ndjson => {
            println!(
                r#"{{"schema":"zornmesh.cli.service.v1","action":"{action}","unit_path":{unit},"hint":{hint}}}"#,
                unit = json_string(&unit_path.display().to_string()),
                hint = json_string(&hint)
            );
        }
    }
    Ok(())
}

fn service_uninstall(invocation: &Invocation) -> Result<(), CliError> {
    service_refuse_root()?;
    let unit_path = service_unit_path()?;
    let existed = unit_path.exists();
    if existed {
        fs::remove_file(&unit_path).map_err(|error| {
            CliError::new(
                "E_DAEMON_IO",
                format!("cannot remove {}: {error}", unit_path.display()),
                ExitKind::Io,
            )
        })?;
    }
    let action = if existed { "removed" } else { "absent" };
    let hint = service_deactivation_hint(&unit_path);
    match invocation.output {
        OutputFormat::Human => {
            println!("zornmesh service {action} at {}", unit_path.display());
            if existed {
                println!("{hint}");
            }
        }
        OutputFormat::Json | OutputFormat::Ndjson => {
            println!(
                r#"{{"schema":"zornmesh.cli.service.v1","action":"{action}","unit_path":{unit},"hint":{hint}}}"#,
                unit = json_string(&unit_path.display().to_string()),
                hint = json_string(&hint)
            );
        }
    }
    Ok(())
}

fn service_status(invocation: &Invocation) -> Result<(), CliError> {
    let unit_path = service_unit_path()?;
    let installed = unit_path.exists();
    let socket_path = &invocation.config.socket_path;
    let reachable = crate::rpc::local::effective_uid()
        .ok()
        .map(|uid| crate::rpc::local::connect_trusted_socket(socket_path, uid).is_ok())
        .unwrap_or(false);

    match invocation.output {
        OutputFormat::Human => {
            println!(
                "zornmesh service: installed={installed} reachable={reachable} unit={}",
                unit_path.display()
            );
        }
        OutputFormat::Json | OutputFormat::Ndjson => {
            println!(
                r#"{{"schema":"zornmesh.cli.service.v1","installed":{installed},"reachable":{reachable},"unit_path":{unit},"socket_path":{socket}}}"#,
                unit = json_string(&unit_path.display().to_string()),
                socket = json_string(&socket_path.display().to_string())
            );
        }
    }
    Ok(())
}

pub const WORKER_HELP: &str = "zornmesh worker\nRun a long-lived worker daemon that subscribes to debate plans and drives an underlying coding-agent CLI.\n\nUsage: zornmesh worker --platform <claude|copilot|gemini|opencode> [OPTIONS]\n\nOptions:\n      --platform <NAME>            Coding-agent CLI to drive (required)\n      --invocation-timeout <MS>   Per-call subprocess deadline (default 120000)\n  -h, --help                      Print help\n\nThe worker connects to the local broker, subscribes to `debate.>.plan`, and\non each delivery shells out to the platform's non-interactive mode\n(`claude --print`, `copilot -p`, `gemini --print`, `opencode run`). The\nresulting critique is published to `debate.<id>.critique.<platform>` for the\norchestrator to aggregate.\n\nWorkers are per-platform. Run one per coding-agent CLI you want to\nparticipate in debates. Each worker registers as `agent.worker.<platform>`\nin the mesh audit trail.\n";

pub const DEBATE_HELP: &str = "zornmesh debate\nDrive multi-agent debates over the local mesh.\n\nUsage: zornmesh debate run <PLAN> [OPTIONS]\n       zornmesh debate run --plan-stdin [OPTIONS]\n\nOptions for `run`:\n      --plan-stdin              Read the plan from stdin instead of an arg\n      --repo <PATH>             Repo path workers should cwd into when invoking\n      --timeout <SECS>          Total wall-clock budget (default 30)\n      --quorum <N>              Minimum critiques before early-completion (default 1)\n      --as-agent <ID>           Originator identity (default agent.driver.cli)\n      --output <FORMAT>         Select human or json output\n  -h, --help                    Print help\n\nThe debate command publishes a plan envelope to `debate.<id>.plan`, blocks\nuntil the quorum is met or the timeout fires, then emits a synthesized\nconsensus that explicitly preserves dissent. Worker daemons (run with\n`zornmesh worker --platform <name>`) must already be subscribed for any\ncritiques to arrive.\n";

fn run_worker(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_help("worker", WORKER_HELP, invocation.output),
        [flag] if is_help(flag) => print_help("worker", WORKER_HELP, invocation.output),
        rest => worker_listen(rest, invocation),
    }
}

fn worker_listen(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    let mut platform: Option<crate::debate::Platform> = None;
    let mut invocation_timeout_ms: u64 = 120_000;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--platform" => {
                let value = iter.next().ok_or_else(|| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        "worker --platform requires a value",
                        ExitKind::Validation,
                    )
                })?;
                platform = Some(crate::debate::Platform::parse(value).ok_or_else(|| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        format!(
                            "unknown worker platform '{value}'; expected claude|copilot|gemini|opencode"
                        ),
                        ExitKind::Validation,
                    )
                })?);
            }
            "--invocation-timeout" => {
                let value = iter.next().ok_or_else(|| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        "worker --invocation-timeout requires a millisecond value",
                        ExitKind::Validation,
                    )
                })?;
                invocation_timeout_ms = value.parse::<u64>().map_err(|_| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        format!("worker --invocation-timeout: '{value}' is not a u64"),
                        ExitKind::Validation,
                    )
                })?;
            }
            other => {
                return Err(CliError::new(
                    "E_UNSUPPORTED_COMMAND",
                    format!("unsupported zornmesh worker argument '{other}'"),
                    ExitKind::UserError,
                ));
            }
        }
    }
    let platform = platform.ok_or_else(|| {
        CliError::new(
            "E_VALIDATION_FAILED",
            "worker requires --platform <claude|copilot|gemini|opencode>",
            ExitKind::Validation,
        )
    })?;

    let _ = invocation; // unused in v0.2: workers are stateless of CLI invocation context
    let broker = crate::broker::Broker::new();
    let daemon = crate::debate::WorkerDaemon::new(&broker, platform)
        .with_invocation_timeout(Duration::from_millis(invocation_timeout_ms));
    eprintln!(
        "zornmesh worker: platform={} agent_id={} subscription={}",
        platform.name(),
        daemon.agent_id(),
        crate::debate::WORKER_PLAN_SUBSCRIPTION
    );
    daemon
        .listen(None)
        .map_err(|err| CliError::new("E_DAEMON_IO", err, ExitKind::Io))
}

fn run_debate(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_help("debate", DEBATE_HELP, invocation.output),
        [flag] if is_help(flag) => print_help("debate", DEBATE_HELP, invocation.output),
        [sub, rest @ ..] if sub == "run" => debate_run(rest, invocation),
        [sub, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh debate subcommand '{sub}'"),
            ExitKind::UserError,
        )),
    }
}

fn debate_run(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    let mut plan: Option<String> = None;
    let mut plan_stdin = false;
    let mut repo: Option<String> = None;
    let mut timeout_secs: u64 = 30;
    let mut quorum: u32 = 1;
    let mut originator: String = "agent.driver.cli".to_owned();

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--plan-stdin" => plan_stdin = true,
            "--repo" => {
                repo = Some(
                    iter.next()
                        .ok_or_else(|| {
                            CliError::new(
                                "E_VALIDATION_FAILED",
                                "debate run --repo requires a value",
                                ExitKind::Validation,
                            )
                        })?
                        .to_owned(),
                );
            }
            "--timeout" => {
                let value = iter.next().ok_or_else(|| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        "debate run --timeout requires a value",
                        ExitKind::Validation,
                    )
                })?;
                timeout_secs = value.parse::<u64>().map_err(|_| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        format!("debate run --timeout: '{value}' is not a u64"),
                        ExitKind::Validation,
                    )
                })?;
            }
            "--quorum" => {
                let value = iter.next().ok_or_else(|| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        "debate run --quorum requires a value",
                        ExitKind::Validation,
                    )
                })?;
                quorum = value.parse::<u32>().map_err(|_| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        format!("debate run --quorum: '{value}' is not a u32"),
                        ExitKind::Validation,
                    )
                })?;
            }
            "--as-agent" => {
                originator = iter
                    .next()
                    .ok_or_else(|| {
                        CliError::new(
                            "E_VALIDATION_FAILED",
                            "debate run --as-agent requires a value",
                            ExitKind::Validation,
                        )
                    })?
                    .to_owned();
            }
            other if !other.starts_with("--") && plan.is_none() => {
                plan = Some(other.to_owned());
            }
            other => {
                return Err(CliError::new(
                    "E_UNSUPPORTED_COMMAND",
                    format!("unsupported zornmesh debate run argument '{other}'"),
                    ExitKind::UserError,
                ));
            }
        }
    }

    let plan = if plan_stdin {
        use std::io::Read as _;
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).map_err(|err| {
            CliError::new(
                "E_DAEMON_IO",
                format!("debate run --plan-stdin: read failure: {err}"),
                ExitKind::Io,
            )
        })?;
        buf
    } else {
        plan.ok_or_else(|| {
            CliError::new(
                "E_VALIDATION_FAILED",
                "debate run requires a plan argument or --plan-stdin",
                ExitKind::Validation,
            )
        })?
    };

    // For v0.2 the CLI runs the orchestrator against an in-process broker.
    // Workers must already be running against this same broker (this means
    // the v0.2 CLI flow currently exercises the in-process path; real-world
    // multi-process workers wait on v0.3's daemon-mediated broker access).
    let broker = crate::broker::Broker::new();
    let credentials =
        crate::broker::PeerCredentials::new(0, 0, std::process::id());
    let trust_policy = crate::broker::SocketTrustPolicy::new(0, 0, 0o600);
    let orchestrator = crate::debate::DebateOrchestrator::new(&broker, credentials, trust_policy);
    let options = crate::debate::DebateOptions::new(originator, plan)
        .with_timeout(Duration::from_secs(timeout_secs))
        .with_quorum(quorum.max(1));
    let options = if let Some(r) = repo {
        options.with_repo(r)
    } else {
        options
    };

    let outcome = orchestrator
        .run(options)
        .map_err(|err| CliError::new(error_code_to_static(err.code()), err.message(), ExitKind::Validation))?;

    match invocation.output {
        OutputFormat::Human => {
            println!("debate_id: {}", outcome.debate_id);
            println!("participants: {}", outcome.consensus.participants.len());
            println!("---");
            println!("{}", outcome.consensus.consensus);
        }
        OutputFormat::Json | OutputFormat::Ndjson => {
            let critiques: Vec<serde_json::Value> = outcome
                .critiques
                .iter()
                .map(crate::debate::CritiqueEnvelope::to_json)
                .collect();
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": crate::debate::DEBATE_SCHEMA_VERSION,
                    "debate_id": outcome.debate_id,
                    "consensus": outcome.consensus.to_json(),
                    "critiques": critiques,
                })
            );
        }
    }
    Ok(())
}

/// Map a runtime-known error code (a borrowed &str) to a static lifetime so
/// it can be embedded in CliError without leaking. The `code` field of
/// CliError is `&'static str` for cheap formatting; for debate errors we
/// know the universe of codes ahead of time.
fn error_code_to_static(code: &str) -> &'static str {
    match code {
        "E_DEBATE_INVALID_PLAN" => "E_DEBATE_INVALID_PLAN",
        "E_DEBATE_BROKER_FAILURE" => "E_DEBATE_BROKER_FAILURE",
        "E_DEBATE_PUBLISH_FAILURE" => "E_DEBATE_PUBLISH_FAILURE",
        _ => "E_DEBATE_UNKNOWN",
    }
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
    let current_uid = crate::rpc::local::effective_uid().map_err(cli_error_from_local)?;
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

#[derive(Debug, Clone, Default)]
struct AirmfOptions {
    evidence_path: Option<PathBuf>,
    correlation_id: Option<String>,
}

fn parse_airmf_options(args: &[String]) -> Result<AirmfOptions, CliError> {
    let mut options = AirmfOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let (key, value, advance) = if let Some((k, v)) = arg.split_once('=') {
            (k.to_owned(), v.to_owned(), 1)
        } else {
            let value = inspect_option_value(args, index, arg)?.to_owned();
            (arg.clone(), value, 2)
        };
        match key.as_str() {
            "--evidence" | "--evidence-path" => {
                options.evidence_path = Some(parse_non_empty_path(&key, &value)?);
            }
            "--correlation-id" => {
                options.correlation_id = Some(parse_non_empty_string(&key, &value)?);
            }
            other => {
                return Err(CliError::new(
                    "E_UNSUPPORTED_COMMAND",
                    format!("unsupported zornmesh airmf argument '{other}'"),
                    ExitKind::UserError,
                ));
            }
        }
        index += advance;
    }
    Ok(options)
}

fn run_airmf(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_help("airmf help", AIRMF_HELP, invocation.output),
        [flag] if is_help(flag) => print_help("airmf help", AIRMF_HELP, invocation.output),
        [command, rest @ ..] if command == "map" => {
            let options = parse_airmf_options(rest)?;
            airmf_map(options, invocation.output)
        }
        [command, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh airmf subcommand '{command}'"),
            ExitKind::UserError,
        )),
    }
}

#[derive(Debug, Clone, Copy)]
struct AirmfMapping {
    function: &'static str,
    category: &'static str,
    control_ref: &'static str,
}

fn map_audit_action_to_airmf(action: &str) -> Option<AirmfMapping> {
    match action {
        "accepted_envelope" => Some(AirmfMapping {
            function: "Map",
            category: "MAP-1.1",
            control_ref: "context-and-roles",
        }),
        "replay_requested" => Some(AirmfMapping {
            function: "Manage",
            category: "MANAGE-2.1",
            control_ref: "risk-treatment-and-recovery",
        }),
        "redaction_applied" => Some(AirmfMapping {
            function: "Govern",
            category: "GOVERN-5.1",
            control_ref: "data-privacy-and-protection",
        }),
        "dead_lettered" => Some(AirmfMapping {
            function: "Measure",
            category: "MEASURE-3.2",
            control_ref: "incident-tracking",
        }),
        _ => None,
    }
}

fn map_envelope_to_airmf(record: &EvidenceEnvelopeRecord) -> Option<AirmfMapping> {
    match record.delivery_state() {
        "accepted" | "delivered" | "acknowledged" => Some(AirmfMapping {
            function: "Map",
            category: "MAP-1.1",
            control_ref: "context-and-roles",
        }),
        "dead_lettered" => Some(AirmfMapping {
            function: "Measure",
            category: "MEASURE-3.2",
            control_ref: "incident-tracking",
        }),
        "replayed" => Some(AirmfMapping {
            function: "Manage",
            category: "MANAGE-2.1",
            control_ref: "risk-treatment-and-recovery",
        }),
        "redaction_applied" => Some(AirmfMapping {
            function: "Govern",
            category: "GOVERN-5.1",
            control_ref: "data-privacy-and-protection",
        }),
        _ => None,
    }
}

fn map_dead_letter_to_airmf() -> AirmfMapping {
    AirmfMapping {
        function: "Measure",
        category: "MEASURE-3.2",
        control_ref: "incident-tracking",
    }
}

fn airmf_record_evidence_gap(record_kind: &str, missing: &[&str]) -> Option<String> {
    if missing.is_empty() {
        None
    } else {
        Some(format!(
            "{record_kind} missing required metadata: {}",
            missing.join(",")
        ))
    }
}

fn airmf_map(options: AirmfOptions, output: OutputFormat) -> Result<(), CliError> {
    if output == OutputFormat::Ndjson {
        return Err(ndjson_not_supported("airmf map"));
    }
    let evidence_path = resolve_evidence_path(options.evidence_path.as_ref())?.ok_or_else(|| {
        CliError::new(
            "E_EVIDENCE_STORE_UNAVAILABLE",
            format!(
                "evidence store path is not configured; pass --evidence <PATH> or set {ENV_EVIDENCE_PATH}"
            ),
            ExitKind::TemporaryUnavailable,
        )
    })?;
    let store = FileEvidenceStore::open_evidence(&evidence_path).map_err(|error| {
        CliError::new(
            "E_EVIDENCE_STORE_UNAVAILABLE",
            format!("evidence store is unavailable: {error}"),
            ExitKind::TemporaryUnavailable,
        )
    })?;

    let envelopes = match options.correlation_id.as_deref() {
        Some(id) => store.query_envelopes(EvidenceQuery::new().correlation_id(id)),
        None => store.query_envelopes(EvidenceQuery::new()),
    };
    let dead_letters = match options.correlation_id.as_deref() {
        Some(id) => store.query_dead_letters(DeadLetterQuery::new().correlation_id(id)),
        None => store.query_dead_letters(DeadLetterQuery::new()),
    };
    let audit_entries = store.audit_entries();
    let audit_filtered: Vec<&EvidenceAuditEntry> = match options.correlation_id.as_deref() {
        Some(id) => audit_entries
            .iter()
            .filter(|entry| entry.correlation_id() == id)
            .collect(),
        None => audit_entries.iter().collect(),
    };

    let mut records = Vec::new();
    let mut counts = AirmfCounts::default();

    for envelope in &envelopes {
        let mapping = map_envelope_to_airmf(envelope);
        let mut missing: Vec<&str> = Vec::new();
        if envelope.correlation_id().is_empty() {
            missing.push("correlation_id");
        }
        if envelope.trace_id().is_empty() {
            missing.push("trace_id");
        }
        if envelope.subject().is_empty() {
            missing.push("subject");
        }
        let gap = airmf_record_evidence_gap("envelope", &missing);
        let entry =
            airmf_record_json("envelope", envelope.message_id(), envelope.subject(), mapping, gap.as_deref());
        counts.tally(&entry);
        records.push(entry);
    }

    for dead_letter in &dead_letters {
        let mapping = Some(map_dead_letter_to_airmf());
        let entry = airmf_record_json(
            "dead_letter",
            dead_letter.message_id(),
            dead_letter.subject(),
            mapping,
            None,
        );
        counts.tally(&entry);
        records.push(entry);
    }

    for entry in &audit_filtered {
        let mapping = map_audit_action_to_airmf(entry.action());
        let mut missing: Vec<&str> = Vec::new();
        if entry.correlation_id().is_empty() {
            missing.push("correlation_id");
        }
        if entry.trace_id().is_empty() {
            missing.push("trace_id");
        }
        let gap = airmf_record_evidence_gap("audit", &missing);
        let entry_json =
            airmf_record_json("audit", entry.message_id(), entry.action(), mapping, gap.as_deref());
        counts.tally(&entry_json);
        records.push(entry_json);
    }

    let coverage = if counts.total() == 0 {
        "empty"
    } else if counts.unmapped == 0 && counts.evidence_gap == 0 {
        "complete"
    } else {
        "partial"
    };
    let warnings: Vec<DiagnosticWarning> = if counts.unmapped > 0 || counts.evidence_gap > 0 {
        vec![DiagnosticWarning::new(
            "W_AIRMF_COVERAGE_INCOMPLETE",
            format!(
                "{} unmapped record(s), {} evidence_gap record(s); review before claiming control coverage",
                counts.unmapped, counts.evidence_gap
            ),
        )]
    } else {
        Vec::new()
    };

    let generated_at_unix_ms = current_unix_ms_for_retention();
    let data = serde_json::json!({
        "schema_version": AIRMF_REPORT_SCHEMA_VERSION,
        "mapping_definition_version": AIRMF_MAPPING_DEFINITION_VERSION,
        "generated_at_unix_ms": generated_at_unix_ms,
        "evidence_path": evidence_path.display().to_string(),
        "filter": {"correlation_id": options.correlation_id},
        "coverage": coverage,
        "totals": {
            "mapped": counts.mapped,
            "unmapped": counts.unmapped,
            "evidence_gap": counts.evidence_gap,
            "total": counts.total(),
        },
        "records": records,
    });

    match output {
        OutputFormat::Human => {
            println!(
                "airmf map: coverage={} mapping_version={} mapped={} unmapped={} evidence_gap={} total={}",
                coverage,
                AIRMF_MAPPING_DEFINITION_VERSION,
                counts.mapped,
                counts.unmapped,
                counts.evidence_gap,
                counts.total(),
            );
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
                    "command": "airmf",
                    "status": "ok",
                    "data": data,
                    "warnings": warnings.iter().map(warning_json).collect::<Vec<_>>(),
                })
            );
            Ok(())
        }
        OutputFormat::Ndjson => unreachable!("ndjson rejected before airmf rendering"),
    }
}

#[derive(Debug, Clone, Default)]
struct AirmfCounts {
    mapped: usize,
    unmapped: usize,
    evidence_gap: usize,
}

impl AirmfCounts {
    fn tally(&mut self, entry: &serde_json::Value) {
        match entry["automatic_mapping"].as_str() {
            Some("mapped") => self.mapped += 1,
            Some("unmapped") => self.unmapped += 1,
            Some("evidence_gap") => self.evidence_gap += 1,
            _ => {}
        }
    }
    fn total(&self) -> usize {
        self.mapped + self.unmapped + self.evidence_gap
    }
}

fn airmf_record_json(
    kind: &str,
    message_id: &str,
    subject_or_action: &str,
    mapping: Option<AirmfMapping>,
    evidence_gap_reason: Option<&str>,
) -> serde_json::Value {
    let automatic_mapping = if evidence_gap_reason.is_some() {
        "evidence_gap"
    } else if mapping.is_some() {
        "mapped"
    } else {
        "unmapped"
    };
    serde_json::json!({
        "kind": kind,
        "message_id": message_id,
        "subject_or_action": subject_or_action,
        "automatic_mapping": automatic_mapping,
        "mapping": mapping.map(|m| serde_json::json!({
            "function": m.function,
            "category": m.category,
            "control_ref": m.control_ref,
        })),
        "evidence_gap_reason": evidence_gap_reason,
    })
}

#[derive(Debug, Clone, Default)]
struct RedactOptions {
    evidence_path: Option<PathBuf>,
    message_id: Option<String>,
    actor: Option<String>,
    policy_version: Option<String>,
    reason: Option<String>,
    preview: bool,
    yes: bool,
    confirmation_token: Option<String>,
}

fn parse_redact_options(args: &[String]) -> Result<RedactOptions, CliError> {
    let mut options = RedactOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--preview" {
            options.preview = true;
            index += 1;
            continue;
        }
        if arg == "--yes" {
            options.yes = true;
            index += 1;
            continue;
        }
        let (key, value, advance) = if let Some((k, v)) = arg.split_once('=') {
            (k.to_owned(), v.to_owned(), 1)
        } else {
            let value = inspect_option_value(args, index, arg)?.to_owned();
            (arg.clone(), value, 2)
        };
        match key.as_str() {
            "--evidence" | "--evidence-path" => {
                options.evidence_path = Some(parse_non_empty_path(&key, &value)?);
            }
            "--message-id" => {
                options.message_id = Some(parse_non_empty_string(&key, &value)?);
            }
            "--actor" => {
                options.actor = Some(parse_non_empty_string(&key, &value)?);
            }
            "--policy-version" => {
                options.policy_version = Some(parse_non_empty_string(&key, &value)?);
            }
            "--reason" => {
                options.reason = Some(parse_non_empty_string(&key, &value)?);
            }
            "--confirmation-token" => {
                options.confirmation_token = Some(parse_non_empty_string(&key, &value)?);
            }
            other => {
                return Err(CliError::new(
                    "E_UNSUPPORTED_COMMAND",
                    format!("unsupported zornmesh redact argument '{other}'"),
                    ExitKind::UserError,
                ));
            }
        }
        index += advance;
    }
    if options.preview && (options.yes || options.confirmation_token.is_some()) {
        return Err(CliError::new(
            "E_REDACT_INVALID_FLAGS",
            "--preview cannot be combined with --yes or --confirmation-token".to_owned(),
            ExitKind::UserError,
        ));
    }
    if options.yes && options.confirmation_token.is_some() {
        return Err(CliError::new(
            "E_REDACT_INVALID_FLAGS",
            "--yes cannot be combined with --confirmation-token".to_owned(),
            ExitKind::UserError,
        ));
    }
    Ok(options)
}

fn run_redact(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_help("redact help", REDACT_HELP, invocation.output),
        [flag] if is_help(flag) => print_help("redact help", REDACT_HELP, invocation.output),
        [command, rest @ ..] if command == "apply" => {
            let options = parse_redact_options(rest)?;
            redact_apply(options, invocation.output)
        }
        [command, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh redact subcommand '{command}'"),
            ExitKind::UserError,
        )),
    }
}

fn redact_apply(options: RedactOptions, output: OutputFormat) -> Result<(), CliError> {
    if output == OutputFormat::Ndjson {
        return Err(ndjson_not_supported("redact apply"));
    }
    let message_id = options.message_id.as_deref().ok_or_else(|| {
        CliError::new(
            "E_REDACT_SCOPE_REQUIRED",
            "redact apply requires --message-id <ID> to scope the redaction".to_owned(),
            ExitKind::Validation,
        )
    })?;
    let actor = options.actor.as_deref().ok_or_else(|| {
        CliError::new(
            "E_REDACT_ACTOR_REQUIRED",
            "redact apply requires --actor <ID> to authorise the redaction".to_owned(),
            ExitKind::Validation,
        )
    })?;
    let policy_version = options.policy_version.as_deref().ok_or_else(|| {
        CliError::new(
            "E_REDACT_POLICY_REQUIRED",
            "redact apply requires --policy-version <VERSION>".to_owned(),
            ExitKind::Validation,
        )
    })?;
    let reason = options.reason.as_deref().ok_or_else(|| {
        CliError::new(
            "E_REDACT_REASON_REQUIRED",
            "redact apply requires --reason <TEXT>".to_owned(),
            ExitKind::Validation,
        )
    })?;

    let evidence_path = resolve_evidence_path(options.evidence_path.as_ref())?.ok_or_else(|| {
        CliError::new(
            "E_EVIDENCE_STORE_UNAVAILABLE",
            format!(
                "evidence store path is not configured; pass --evidence <PATH> or set {ENV_EVIDENCE_PATH}"
            ),
            ExitKind::TemporaryUnavailable,
        )
    })?;
    let store = FileEvidenceStore::open_evidence(&evidence_path).map_err(|error| {
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
                "E_REDACT_NOT_FOUND",
                format!("no envelope evidence matched message id '{message_id}'"),
                ExitKind::NotFound,
            )
        })?;

    let prior_audit_hash = store
        .audit_entries()
        .into_iter()
        .filter(|entry| entry.message_id() == record.message_id())
        .next_back()
        .map(|entry| entry.current_audit_hash().to_owned())
        .unwrap_or_else(|| "0".to_owned());
    let token = redact_confirmation_token(&record, actor, policy_version, reason);
    let scope_summary = serde_json::json!({
        "message_id": record.message_id(),
        "correlation_id": record.correlation_id(),
        "trace_id": record.trace_id(),
        "subject": record.subject(),
        "checkpoint_daemon_sequence": record.daemon_sequence(),
        "prior_audit_hash": prior_audit_hash,
    });

    if options.preview {
        let data = serde_json::json!({
            "mode": "preview",
            "side_effect": false,
            "confirmation_token": token,
            "scope": scope_summary,
            "actor": actor,
            "policy_version": policy_version,
            "reason": reason,
            "expected_effect": "appends a redaction_applied audit transition that anchors the redaction proof to the original message without rewriting prior audit rows",
            "policy_checks": [
                "evidence_store_available",
                "record_exists",
                "actor_present",
                "policy_version_present",
                "reason_present"
            ],
            "required_confirmation": "rerun with --yes or --confirmation-token <TOKEN>",
        });
        return emit_redact_response(output, data, None);
    }

    if !options.yes {
        match options.confirmation_token.as_deref() {
            None => {
                return Err(CliError::new(
                    "E_REDACT_CONFIRMATION_REQUIRED",
                    format!(
                        "redaction of '{message_id}' requires confirmation; rerun with --preview, --yes, or --confirmation-token <TOKEN>"
                    ),
                    ExitKind::Validation,
                ));
            }
            Some(provided) if provided != token => {
                return Err(CliError::new(
                    "E_REDACT_STALE_CONFIRMATION",
                    format!(
                        "confirmation token does not match preview for '{message_id}'; rerun --preview to obtain a fresh token"
                    ),
                    ExitKind::Validation,
                ));
            }
            Some(_) => {}
        }
    }

    let outcome_details = format!(
        "redaction_applied actor={actor} policy_version={policy_version} reason={reason} prior_audit_hash={prior_audit_hash} checkpoint_sequence={}",
        record.daemon_sequence()
    );
    let transition = EvidenceStateTransitionInput::new(
        record.daemon_sequence(),
        record.message_id(),
        actor,
        "redaction_applied",
        record.subject(),
        record.correlation_id(),
        record.trace_id(),
        record.delivery_state(),
        "redaction_applied",
        outcome_details,
    )
    .map_err(|error| {
        CliError::new(
            "E_REDACT_INVALID_INPUT",
            format!("redaction transition input invalid: {error}"),
            ExitKind::Validation,
        )
    })?;
    let audit = store.persist_state_transition(transition).map_err(|error| {
        CliError::new(
            "E_REDACT_PERSIST_FAILED",
            format!("redaction proof could not be persisted: {error}"),
            ExitKind::Io,
        )
    })?;

    let data = serde_json::json!({
        "mode": "commit",
        "side_effect": true,
        "confirmation_source": if options.yes { "yes_flag" } else { "confirmation_token" },
        "confirmation_token": token,
        "scope": scope_summary,
        "actor": actor,
        "policy_version": policy_version,
        "reason": reason,
        "audit_action": audit.action(),
        "audit_state_to": audit.state_to(),
        "audit_daemon_sequence": audit.daemon_sequence(),
        "audit_previous_hash": audit.previous_audit_hash(),
        "audit_current_hash": audit.current_audit_hash(),
    });
    emit_redact_response(output, data, None)
}

fn redact_confirmation_token(
    record: &EvidenceEnvelopeRecord,
    actor: &str,
    policy_version: &str,
    reason: &str,
) -> String {
    let parts = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        record.message_id(),
        record.correlation_id(),
        record.trace_id(),
        record.subject(),
        record.daemon_sequence(),
        actor,
        policy_version,
        reason,
    );
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in parts.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("zmrk-{hash:016x}")
}

fn emit_redact_response(
    output: OutputFormat,
    data: serde_json::Value,
    warning: Option<DiagnosticWarning>,
) -> Result<(), CliError> {
    let warnings: Vec<DiagnosticWarning> = warning.into_iter().collect();
    match output {
        OutputFormat::Human => {
            println!(
                "redact apply: mode={} side_effect={} message_id={}",
                data["mode"].as_str().unwrap_or(""),
                data["side_effect"].as_bool().unwrap_or(false),
                data["scope"]["message_id"].as_str().unwrap_or(""),
            );
            if let Some(token) = data["confirmation_token"].as_str() {
                println!("  confirmation_token={token}");
            }
            if let Some(actor) = data["actor"].as_str() {
                println!("  actor={actor}");
            }
            if let Some(policy) = data["policy_version"].as_str() {
                println!("  policy_version={policy}");
            }
            if let Some(reason) = data["reason"].as_str() {
                println!("  reason={reason}");
            }
            if let Some(checkpoint) = data["scope"]["checkpoint_daemon_sequence"].as_u64() {
                println!("  checkpoint_daemon_sequence={checkpoint}");
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
                    "command": "redact",
                    "status": "ok",
                    "data": data,
                    "warnings": warnings.iter().map(warning_json).collect::<Vec<_>>(),
                })
            );
            Ok(())
        }
        OutputFormat::Ndjson => unreachable!("ndjson rejected before redact rendering"),
    }
}

#[derive(Debug, Clone, Default)]
struct EvidenceExportOptions {
    evidence_path: Option<PathBuf>,
    release_manifest_path: Option<PathBuf>,
    since_unix_ms: Option<u64>,
    until_unix_ms: Option<u64>,
}

fn parse_evidence_export_options(args: &[String]) -> Result<EvidenceExportOptions, CliError> {
    let mut options = EvidenceExportOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let (key, value, advance) = if let Some((k, v)) = arg.split_once('=') {
            (k.to_owned(), v.to_owned(), 1)
        } else {
            let value = inspect_option_value(args, index, arg)?.to_owned();
            (arg.clone(), value, 2)
        };
        match key.as_str() {
            "--evidence" | "--evidence-path" => {
                options.evidence_path = Some(parse_non_empty_path(&key, &value)?);
            }
            "--release-manifest" => {
                options.release_manifest_path = Some(parse_non_empty_path(&key, &value)?);
            }
            "--since" => {
                let parsed: u64 = value.parse().map_err(|_| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        format!("--since must be a non-negative integer, got '{value}'"),
                        ExitKind::Validation,
                    )
                })?;
                options.since_unix_ms = Some(parsed);
            }
            "--until" => {
                let parsed: u64 = value.parse().map_err(|_| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        format!("--until must be a non-negative integer, got '{value}'"),
                        ExitKind::Validation,
                    )
                })?;
                options.until_unix_ms = Some(parsed);
            }
            other => {
                return Err(CliError::new(
                    "E_UNSUPPORTED_COMMAND",
                    format!("unsupported zornmesh evidence argument '{other}'"),
                    ExitKind::UserError,
                ));
            }
        }
        index += advance;
    }
    Ok(options)
}

fn run_evidence(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_help("evidence help", EVIDENCE_HELP, invocation.output),
        [flag] if is_help(flag) => print_help("evidence help", EVIDENCE_HELP, invocation.output),
        [command, rest @ ..] if command == "export" => {
            let options = parse_evidence_export_options(rest)?;
            evidence_export(options, invocation.output)
        }
        [command, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh evidence subcommand '{command}'"),
            ExitKind::UserError,
        )),
    }
}

fn evidence_export(options: EvidenceExportOptions, output: OutputFormat) -> Result<(), CliError> {
    if output == OutputFormat::Ndjson {
        return Err(ndjson_not_supported("evidence export"));
    }
    let since = options.since_unix_ms.ok_or_else(|| {
        CliError::new(
            "E_VALIDATION_FAILED",
            "evidence export requires --since <UNIX_MS>".to_owned(),
            ExitKind::Validation,
        )
    })?;
    let until = options.until_unix_ms.ok_or_else(|| {
        CliError::new(
            "E_VALIDATION_FAILED",
            "evidence export requires --until <UNIX_MS>".to_owned(),
            ExitKind::Validation,
        )
    })?;
    if since > until {
        return Err(CliError::new(
            "E_VALIDATION_FAILED",
            format!("invalid time window: --since ({since}) must be <= --until ({until})"),
            ExitKind::Validation,
        ));
    }

    let evidence_path = resolve_evidence_path(options.evidence_path.as_ref())?.ok_or_else(|| {
        CliError::new(
            "E_EVIDENCE_STORE_UNAVAILABLE",
            format!(
                "evidence store path is not configured; pass --evidence <PATH> or set {ENV_EVIDENCE_PATH}"
            ),
            ExitKind::TemporaryUnavailable,
        )
    })?;
    let store = FileEvidenceStore::open_evidence(&evidence_path).map_err(|error| {
        CliError::new(
            "E_EVIDENCE_EXPORT_STORE_UNAVAILABLE",
            format!("evidence store is unavailable: {error}"),
            ExitKind::TemporaryUnavailable,
        )
    })?;

    let started_unix_ms = current_unix_ms_for_retention();

    let mut envelopes: Vec<EvidenceEnvelopeRecord> = store
        .query_envelopes(EvidenceQuery::new())
        .into_iter()
        .filter(|record| record.timestamp_unix_ms() >= since && record.timestamp_unix_ms() <= until)
        .collect();
    envelopes.sort_by_key(EvidenceEnvelopeRecord::daemon_sequence);

    let mut dead_letters: Vec<crate::store::EvidenceDeadLetterRecord> = store
        .query_dead_letters(DeadLetterQuery::new())
        .into_iter()
        .filter(|record| {
            let terminal = record.terminal_unix_ms();
            terminal >= since && terminal <= until
        })
        .collect();
    dead_letters.sort_by_key(|record| record.daemon_sequence());

    let included_message_ids: BTreeSet<String> = envelopes
        .iter()
        .map(|record| record.message_id().to_owned())
        .collect();
    let audit_entries: Vec<EvidenceAuditEntry> = store
        .audit_entries()
        .into_iter()
        .filter(|entry| included_message_ids.contains(entry.message_id()))
        .collect();

    let release_section = match options.release_manifest_path.as_ref() {
        Some(path) => match fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(value) => serde_json::json!({
                    "status": "available",
                    "manifest_path": path.display().to_string(),
                    "manifest": sanitize_release_manifest(&value),
                }),
                Err(error) => serde_json::json!({
                    "status": "unavailable",
                    "manifest_path": path.display().to_string(),
                    "reason": format!("manifest is not valid JSON: {error}"),
                }),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::json!({
                "status": "unavailable",
                "manifest_path": path.display().to_string(),
                "reason": "release manifest does not exist at the configured path",
            }),
            Err(error) => serde_json::json!({
                "status": "unavailable",
                "manifest_path": path.display().to_string(),
                "reason": format!("release manifest read failed: {error}"),
            }),
        },
        None => serde_json::json!({
            "status": "unavailable",
            "manifest_path": null,
            "reason": "release manifest not provided; pass --release-manifest <PATH> to include release evidence",
        }),
    };

    let mut evidence_gaps: Vec<serde_json::Value> = Vec::new();
    if envelopes.is_empty() && dead_letters.is_empty() {
        evidence_gaps.push(serde_json::json!({
            "section": "audit_log_slice",
            "reason": "no records fall within the requested window",
        }));
    }
    if release_section["status"] == "unavailable" {
        evidence_gaps.push(serde_json::json!({
            "section": "release_evidence",
            "reason": release_section["reason"],
        }));
    }

    let bundle_envelopes: Vec<serde_json::Value> = envelopes
        .iter()
        .map(envelope_export_json)
        .collect();
    let bundle_dead_letters: Vec<serde_json::Value> = dead_letters
        .iter()
        .map(dead_letter_export_json)
        .collect();
    let bundle_audit: Vec<serde_json::Value> = audit_entries.iter().map(audit_export_json).collect();

    let finished_unix_ms = current_unix_ms_for_retention();
    let duration_ms = finished_unix_ms.saturating_sub(started_unix_ms);

    let manifest = serde_json::json!({
        "schema_version": "zornmesh.evidence.bundle.v1",
        "evidence_path": evidence_path.display().to_string(),
        "time_window": {"since_unix_ms": since, "until_unix_ms": until},
        "generated_at_unix_ms": finished_unix_ms,
        "duration_ms": duration_ms,
        "sections": {
            "audit_log_slice": {"included": bundle_audit.len()},
            "envelopes": {"included": bundle_envelopes.len()},
            "dead_letters": {"included": bundle_dead_letters.len()},
            "release_evidence": {"status": release_section["status"]},
        },
        "evidence_gaps": evidence_gaps,
    });

    let warnings: Vec<DiagnosticWarning> = if evidence_gaps.is_empty() {
        Vec::new()
    } else {
        vec![DiagnosticWarning::new(
            "W_EVIDENCE_BUNDLE_GAP",
            format!(
                "evidence bundle contains {} gap(s); review manifest before claiming completeness",
                evidence_gaps.len()
            ),
        )]
    };

    let bundle = serde_json::json!({
        "manifest": manifest,
        "audit_log_slice": bundle_audit,
        "envelopes": bundle_envelopes,
        "dead_letters": bundle_dead_letters,
        "release_evidence": release_section,
    });

    match output {
        OutputFormat::Human => {
            println!(
                "evidence export: window={}..{} envelopes={} dead_letters={} audit_entries={} release_status={}",
                since,
                until,
                bundle_envelopes.len(),
                bundle_dead_letters.len(),
                bundle_audit.len(),
                bundle["release_evidence"]["status"].as_str().unwrap_or(""),
            );
            for gap in &evidence_gaps {
                println!(
                    "  evidence_gap: section={} reason={}",
                    gap["section"].as_str().unwrap_or(""),
                    gap["reason"].as_str().unwrap_or(""),
                );
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
                    "command": "evidence",
                    "status": "ok",
                    "data": bundle,
                    "warnings": warnings.iter().map(warning_json).collect::<Vec<_>>(),
                })
            );
            Ok(())
        }
        OutputFormat::Ndjson => unreachable!("ndjson rejected before evidence rendering"),
    }
}

fn sanitize_release_manifest(value: &serde_json::Value) -> serde_json::Value {
    let mut sanitized = serde_json::json!({
        "schema_version": value.get("schema_version").cloned().unwrap_or(serde_json::Value::Null),
        "version": value.get("version").cloned().unwrap_or(serde_json::Value::Null),
    });
    if let Some(artifact) = value.get("artifact") {
        sanitized["artifact"] = serde_json::json!({
            "path": artifact.get("path").cloned().unwrap_or(serde_json::Value::Null),
            "digest": artifact.get("digest").cloned().unwrap_or(serde_json::Value::Null),
        });
    }
    if let Some(signature) = value.get("signature") {
        sanitized["signature"] = serde_json::json!({
            "path": signature.get("path").cloned().unwrap_or(serde_json::Value::Null),
            "status": signature.get("status").cloned().unwrap_or(serde_json::Value::Null),
            "issuer": signature.get("issuer").cloned().unwrap_or(serde_json::Value::Null),
            "transparency_log_index": signature
                .get("transparency_log_index")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        });
    }
    if let Some(sbom) = value.get("sbom") {
        sanitized["sbom"] = serde_json::json!({
            "path": sbom.get("path").cloned().unwrap_or(serde_json::Value::Null),
            "format": sbom.get("format").cloned().unwrap_or(serde_json::Value::Null),
            "status": sbom.get("status").cloned().unwrap_or(serde_json::Value::Null),
            "components_count": sbom
                .get("components_count")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            "generated_at": sbom
                .get("generated_at")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        });
    }
    sanitized
}

fn envelope_export_json(record: &EvidenceEnvelopeRecord) -> serde_json::Value {
    serde_json::json!({
        "kind": "envelope",
        "daemon_sequence": record.daemon_sequence(),
        "message_id": record.message_id(),
        "subject": record.subject(),
        "source_agent": record.source_agent(),
        "target_or_subject": record.target_or_subject(),
        "correlation_id": record.correlation_id(),
        "trace_id": record.trace_id(),
        "span_id": record.span_id(),
        "parent_message_id": record.parent_message_id(),
        "delivery_state": record.delivery_state(),
        "timestamp_unix_ms": record.timestamp_unix_ms(),
        "safe_payload_summary": payload_summary(record.payload_len(), record.payload_content_type()),
    })
}

fn dead_letter_export_json(
    record: &crate::store::EvidenceDeadLetterRecord,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "dead_letter",
        "daemon_sequence": record.daemon_sequence(),
        "message_id": record.message_id(),
        "subject": record.subject(),
        "source_agent": record.source_agent(),
        "intended_target": record.intended_target(),
        "correlation_id": record.correlation_id(),
        "trace_id": record.trace_id(),
        "terminal_state": record.terminal_state(),
        "failure_category": record.failure_category().as_str(),
        "attempt_count": record.attempt_count(),
        "terminal_unix_ms": record.terminal_unix_ms(),
        "safe_details": record.safe_details(),
        "safe_payload_summary": payload_summary(record.payload_len(), record.payload_content_type()),
    })
}

fn audit_export_json(entry: &EvidenceAuditEntry) -> serde_json::Value {
    serde_json::json!({
        "kind": "audit",
        "daemon_sequence": entry.daemon_sequence(),
        "message_id": entry.message_id(),
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
}

#[derive(Debug, Clone, Default)]
struct ComplianceOptions {
    evidence_path: Option<PathBuf>,
    correlation_id: Option<String>,
}

fn parse_compliance_options(args: &[String]) -> Result<ComplianceOptions, CliError> {
    let mut options = ComplianceOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let (key, value, advance) = if let Some((k, v)) = arg.split_once('=') {
            (k.to_owned(), v.to_owned(), 1)
        } else {
            let value = inspect_option_value(args, index, arg)?.to_owned();
            (arg.clone(), value, 2)
        };
        match key.as_str() {
            "--evidence" | "--evidence-path" => {
                options.evidence_path = Some(parse_non_empty_path(&key, &value)?);
            }
            "--correlation-id" => {
                options.correlation_id = Some(parse_non_empty_string(&key, &value)?);
            }
            other => {
                return Err(CliError::new(
                    "E_UNSUPPORTED_COMMAND",
                    format!("unsupported zornmesh compliance argument '{other}'"),
                    ExitKind::UserError,
                ));
            }
        }
        index += advance;
    }
    Ok(options)
}

fn run_compliance(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_help("compliance help", COMPLIANCE_HELP, invocation.output),
        [flag] if is_help(flag) => {
            print_help("compliance help", COMPLIANCE_HELP, invocation.output)
        }
        [command, rest @ ..] if command == "traceability" => {
            let options = parse_compliance_options(rest)?;
            compliance_traceability(options, invocation.output)
        }
        [command, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh compliance subcommand '{command}'"),
            ExitKind::UserError,
        )),
    }
}

fn compliance_traceability(
    options: ComplianceOptions,
    output: OutputFormat,
) -> Result<(), CliError> {
    if output == OutputFormat::Ndjson {
        return Err(ndjson_not_supported("compliance traceability"));
    }
    let evidence_path = resolve_evidence_path(options.evidence_path.as_ref())?.ok_or_else(|| {
        CliError::new(
            "E_EVIDENCE_STORE_UNAVAILABLE",
            format!(
                "evidence store path is not configured; pass --evidence <PATH> or set {ENV_EVIDENCE_PATH}"
            ),
            ExitKind::TemporaryUnavailable,
        )
    })?;
    let store = FileEvidenceStore::open_evidence(&evidence_path).map_err(|error| {
        CliError::new(
            "E_EVIDENCE_STORE_UNAVAILABLE",
            format!("evidence store is unavailable: {error}"),
            ExitKind::TemporaryUnavailable,
        )
    })?;

    let envelopes = match options.correlation_id.as_deref() {
        Some(id) => store.query_envelopes(EvidenceQuery::new().correlation_id(id)),
        None => store.query_envelopes(EvidenceQuery::new()),
    };
    let dead_letters = match options.correlation_id.as_deref() {
        Some(id) => store.query_dead_letters(DeadLetterQuery::new().correlation_id(id)),
        None => store.query_dead_letters(DeadLetterQuery::new()),
    };
    let audit_entries = store.audit_entries();
    let audit_filtered: Vec<&EvidenceAuditEntry> = match options.correlation_id.as_deref() {
        Some(id) => audit_entries
            .iter()
            .filter(|entry| entry.correlation_id() == id)
            .collect(),
        None => audit_entries.iter().collect(),
    };

    let mut records = Vec::new();
    let mut counts = ComplianceCounts::default();
    for envelope in &envelopes {
        let assessment = classify_envelope(envelope, &audit_filtered);
        counts.tally(&assessment);
        records.push(envelope_compliance_json(envelope, &assessment));
    }
    for dead_letter in &dead_letters {
        let assessment = classify_dead_letter(dead_letter);
        counts.tally(&assessment);
        records.push(dead_letter_compliance_json(dead_letter, &assessment));
    }
    for entry in &audit_filtered {
        let assessment = classify_audit_entry(entry);
        counts.tally(&assessment);
        records.push(audit_compliance_json(entry, &assessment));
    }

    let aggregate_status = if counts.evidence_gap > 0 {
        "evidence_gap"
    } else if counts.partial > 0 {
        "partial"
    } else if counts.complete > 0 {
        "complete"
    } else {
        "empty"
    };

    let warnings: Vec<DiagnosticWarning> = if counts.evidence_gap > 0 {
        vec![DiagnosticWarning::new(
            "W_COMPLIANCE_EVIDENCE_GAP",
            format!(
                "{} record(s) carry evidence gaps; review missing fields before claiming completeness",
                counts.evidence_gap
            ),
        )]
    } else {
        Vec::new()
    };

    match output {
        OutputFormat::Human => {
            println!(
                "compliance traceability: status={} complete={} partial={} evidence_gap={} total={}",
                aggregate_status,
                counts.complete,
                counts.partial,
                counts.evidence_gap,
                counts.total(),
            );
            for record in &records {
                println!(
                    "  {} {} status={} missing={} reason={}",
                    record["kind"].as_str().unwrap_or(""),
                    record["message_id"].as_str().unwrap_or(""),
                    record["compliance_status"].as_str().unwrap_or(""),
                    record["missing_fields"]
                        .as_array()
                        .map(|fields| fields
                            .iter()
                            .filter_map(|value| value.as_str())
                            .collect::<Vec<_>>()
                            .join(","))
                        .unwrap_or_default(),
                    record["evidence_gap_reason"].as_str().unwrap_or("none"),
                );
            }
            for warning in &warnings {
                eprintln!("{}: {}", warning.code, warning.message);
            }
            Ok(())
        }
        OutputFormat::Json => {
            let data = serde_json::json!({
                "status": aggregate_status,
                "evidence_path": evidence_path.display().to_string(),
                "filter": {
                    "correlation_id": options.correlation_id,
                },
                "totals": {
                    "complete": counts.complete,
                    "partial": counts.partial,
                    "evidence_gap": counts.evidence_gap,
                    "total": counts.total(),
                },
                "records": records,
            });
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": READ_SCHEMA_VERSION,
                    "command": "compliance",
                    "status": "ok",
                    "data": data,
                    "warnings": warnings.iter().map(warning_json).collect::<Vec<_>>(),
                })
            );
            Ok(())
        }
        OutputFormat::Ndjson => unreachable!("ndjson rejected before compliance rendering"),
    }
}

#[derive(Debug, Clone, Default)]
struct ComplianceCounts {
    complete: usize,
    partial: usize,
    evidence_gap: usize,
}

impl ComplianceCounts {
    fn tally(&mut self, assessment: &ComplianceAssessment) {
        match assessment.status {
            "complete" => self.complete += 1,
            "partial" => self.partial += 1,
            "evidence_gap" => self.evidence_gap += 1,
            _ => {}
        }
    }
    fn total(&self) -> usize {
        self.complete + self.partial + self.evidence_gap
    }
}

#[derive(Debug, Clone)]
struct ComplianceAssessment {
    status: &'static str,
    missing_fields: Vec<&'static str>,
    evidence_gap_reason: Option<&'static str>,
    redacted_fields: Vec<&'static str>,
    has_lineage: bool,
}

fn check_field(value: &str) -> bool {
    !value.trim().is_empty()
}

fn redacted_marker(value: &str) -> bool {
    value == "[REDACTED]"
}

fn classify_envelope(
    envelope: &EvidenceEnvelopeRecord,
    audit_entries: &[&EvidenceAuditEntry],
) -> ComplianceAssessment {
    let mut missing: Vec<&'static str> = Vec::new();
    if !check_field(envelope.source_agent()) {
        missing.push("source_agent");
    }
    if !check_field(envelope.subject()) {
        missing.push("subject");
    }
    if envelope.timestamp_unix_ms() == 0 {
        missing.push("timestamp_unix_ms");
    }
    if !check_field(envelope.correlation_id()) {
        missing.push("correlation_id");
    }
    if !check_field(envelope.trace_id()) {
        missing.push("trace_id");
    }

    let mut redacted: Vec<&'static str> = Vec::new();
    if redacted_marker(envelope.payload_content_type()) {
        redacted.push("payload_content_type");
    }

    let lineage_required = matches!(
        envelope.delivery_state(),
        "replayed" | "retrying" | "dead_lettered"
    ) || audit_entries.iter().any(|entry| {
        entry.message_id() == envelope.message_id()
            && matches!(
                entry.action(),
                "replay_requested" | "retry_attempt" | "dead_lettered"
            )
    });
    let has_lineage = envelope.parent_message_id().is_some();
    let mut evidence_gap_reason: Option<&'static str> = None;

    if envelope.source_agent().starts_with("bridge.") {
        evidence_gap_reason = Some("bridge_originated_legacy_fields");
    }

    if !missing.is_empty() && evidence_gap_reason.is_none() {
        evidence_gap_reason = Some("required_field_missing");
    }

    let status: &'static str = if !missing.is_empty() || evidence_gap_reason.is_some() {
        "evidence_gap"
    } else if lineage_required && !has_lineage {
        "partial"
    } else {
        "complete"
    };
    if status == "partial" {
        evidence_gap_reason = Some("lineage_missing_for_action");
    }

    ComplianceAssessment {
        status,
        missing_fields: missing,
        evidence_gap_reason,
        redacted_fields: redacted,
        has_lineage,
    }
}

fn classify_dead_letter(record: &crate::store::EvidenceDeadLetterRecord) -> ComplianceAssessment {
    let mut missing: Vec<&'static str> = Vec::new();
    if !check_field(record.source_agent()) {
        missing.push("source_agent");
    }
    if !check_field(record.subject()) {
        missing.push("subject");
    }
    if !check_field(record.correlation_id()) {
        missing.push("correlation_id");
    }
    if !check_field(record.trace_id()) {
        missing.push("trace_id");
    }
    if !check_field(record.terminal_state()) {
        missing.push("terminal_state");
    }

    let mut redacted: Vec<&'static str> = Vec::new();
    if redacted_marker(record.payload_content_type()) {
        redacted.push("payload_content_type");
    }
    if redacted_marker(record.safe_details()) {
        redacted.push("safe_details");
    }

    let evidence_gap_reason = if missing.is_empty() {
        None
    } else {
        Some("required_field_missing")
    };
    let status: &'static str = if missing.is_empty() {
        "complete"
    } else {
        "evidence_gap"
    };
    ComplianceAssessment {
        status,
        missing_fields: missing,
        evidence_gap_reason,
        redacted_fields: redacted,
        has_lineage: false,
    }
}

fn classify_audit_entry(entry: &EvidenceAuditEntry) -> ComplianceAssessment {
    let mut missing: Vec<&'static str> = Vec::new();
    if !check_field(entry.actor()) {
        missing.push("actor");
    }
    if !check_field(entry.action()) {
        missing.push("action");
    }
    if !check_field(entry.capability_or_subject()) {
        missing.push("capability_or_subject");
    }
    if !check_field(entry.correlation_id()) {
        missing.push("correlation_id");
    }
    if !check_field(entry.trace_id()) {
        missing.push("trace_id");
    }
    if !check_field(entry.state_to()) {
        missing.push("state_to");
    }

    let mut redacted: Vec<&'static str> = Vec::new();
    if redacted_marker(entry.outcome_details()) {
        redacted.push("outcome_details");
    }

    let status: &'static str = if missing.is_empty() {
        "complete"
    } else {
        "evidence_gap"
    };
    let evidence_gap_reason = if missing.is_empty() {
        None
    } else {
        Some("required_field_missing")
    };
    ComplianceAssessment {
        status,
        missing_fields: missing,
        evidence_gap_reason,
        redacted_fields: redacted,
        has_lineage: false,
    }
}

fn envelope_compliance_json(
    envelope: &EvidenceEnvelopeRecord,
    assessment: &ComplianceAssessment,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "envelope",
        "message_id": envelope.message_id(),
        "subject": envelope.subject(),
        "source_agent": envelope.source_agent(),
        "correlation_id": envelope.correlation_id(),
        "trace_id": envelope.trace_id(),
        "delivery_state": envelope.delivery_state(),
        "compliance_status": assessment.status,
        "missing_fields": assessment.missing_fields,
        "redacted_fields": assessment.redacted_fields,
        "evidence_gap_reason": assessment.evidence_gap_reason,
        "has_lineage": assessment.has_lineage,
    })
}

fn dead_letter_compliance_json(
    record: &crate::store::EvidenceDeadLetterRecord,
    assessment: &ComplianceAssessment,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "dead_letter",
        "message_id": record.message_id(),
        "subject": record.subject(),
        "source_agent": record.source_agent(),
        "correlation_id": record.correlation_id(),
        "trace_id": record.trace_id(),
        "terminal_state": record.terminal_state(),
        "compliance_status": assessment.status,
        "missing_fields": assessment.missing_fields,
        "redacted_fields": assessment.redacted_fields,
        "evidence_gap_reason": assessment.evidence_gap_reason,
        "has_lineage": false,
    })
}

fn audit_compliance_json(
    entry: &EvidenceAuditEntry,
    assessment: &ComplianceAssessment,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "audit",
        "message_id": entry.message_id(),
        "actor": entry.actor(),
        "action": entry.action(),
        "capability_or_subject": entry.capability_or_subject(),
        "correlation_id": entry.correlation_id(),
        "trace_id": entry.trace_id(),
        "state_from": entry.state_from(),
        "state_to": entry.state_to(),
        "compliance_status": assessment.status,
        "missing_fields": assessment.missing_fields,
        "redacted_fields": assessment.redacted_fields,
        "evidence_gap_reason": assessment.evidence_gap_reason,
        "has_lineage": false,
    })
}

#[derive(Debug, Clone, Default)]
struct ReleaseOptions {
    manifest_path: Option<PathBuf>,
}

fn parse_release_options(args: &[String]) -> Result<ReleaseOptions, CliError> {
    let mut options = ReleaseOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if let Some(value) = arg.strip_prefix("--manifest=") {
            options.manifest_path = Some(parse_non_empty_path("--manifest", value)?);
            index += 1;
        } else if arg == "--manifest" {
            let value = inspect_option_value(args, index, arg)?;
            options.manifest_path = Some(parse_non_empty_path(arg, value)?);
            index += 2;
        } else {
            return Err(CliError::new(
                "E_UNSUPPORTED_COMMAND",
                format!("unsupported zornmesh release argument '{arg}'"),
                ExitKind::UserError,
            ));
        }
    }
    Ok(options)
}

fn run_release(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_help("release help", RELEASE_HELP, invocation.output),
        [flag] if is_help(flag) => print_help("release help", RELEASE_HELP, invocation.output),
        [command, rest @ ..] if command == "verify" => {
            let options = parse_release_options(rest)?;
            release_verify(options, invocation.output)
        }
        [command, rest @ ..] if command == "sbom" => {
            let options = parse_release_options(rest)?;
            release_sbom(options, invocation.output)
        }
        [command, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh release subcommand '{command}'"),
            ExitKind::UserError,
        )),
    }
}

fn resolve_release_manifest_path(
    cli_path: Option<&PathBuf>,
) -> Result<Option<PathBuf>, CliError> {
    if let Some(path) = cli_path {
        return Ok(Some(path.clone()));
    }
    match std::env::var(ENV_RELEASE_MANIFEST) {
        Ok(raw) if raw.trim().is_empty() => Err(CliError::new(
            "E_INVALID_CONFIG",
            format!("{ENV_RELEASE_MANIFEST} must not be empty"),
            ExitKind::UserError,
        )),
        Ok(raw) => Ok(Some(PathBuf::from(raw))),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(CliError::new(
            "E_INVALID_CONFIG",
            format!("{ENV_RELEASE_MANIFEST} must be valid UTF-8"),
            ExitKind::UserError,
        )),
    }
}

fn load_release_manifest(
    cli_path: Option<&PathBuf>,
) -> Result<(PathBuf, Option<serde_json::Value>), CliError> {
    let path = resolve_release_manifest_path(cli_path)?.ok_or_else(|| {
        CliError::new(
            "E_RELEASE_MANIFEST_REQUIRED",
            format!(
                "release manifest path is not configured; pass --manifest <PATH> or set {ENV_RELEASE_MANIFEST}"
            ),
            ExitKind::UserError,
        )
    })?;
    match fs::read_to_string(&path) {
        Ok(text) => {
            let value: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
                CliError::new(
                    "E_RELEASE_MANIFEST_CORRUPT",
                    format!("release manifest at {} is not valid JSON: {error}", path.display()),
                    ExitKind::Validation,
                )
            })?;
            Ok((path, Some(value)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok((path, None)),
        Err(error) => Err(CliError::new(
            "E_RELEASE_MANIFEST_UNREADABLE",
            format!("release manifest read failed: {error}"),
            ExitKind::Io,
        )),
    }
}

fn release_verify(options: ReleaseOptions, output: OutputFormat) -> Result<(), CliError> {
    if output == OutputFormat::Ndjson {
        return Err(ndjson_not_supported("release verify"));
    }
    let (manifest_path, manifest) = load_release_manifest(options.manifest_path.as_ref())?;
    let manifest = match manifest {
        Some(value) => value,
        None => {
            return emit_release_verify(
                output,
                ReleaseVerifyOutcome {
                    status: "unverifiable",
                    exit_kind: ExitKind::Validation,
                    error_code: "E_RELEASE_MANIFEST_MISSING",
                    manifest_path: &manifest_path,
                    artifact_path: None,
                    signature_path: None,
                    signature_status: None,
                    issuer: None,
                    transparency_log_index: None,
                    remediation: "release manifest does not exist; ensure release evidence is shipped alongside the artifact",
                },
            );
        }
    };

    let signature = manifest.get("signature").cloned().unwrap_or(serde_json::Value::Null);
    let signature_status_raw = signature
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unverifiable");
    let signature_path = signature
        .get("path")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let issuer = signature
        .get("issuer")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let log_index = signature
        .get("transparency_log_index")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let artifact_path = manifest
        .get("artifact")
        .and_then(|artifact| artifact.get("path"))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);

    let (status, exit_kind, error_code, remediation) = match signature_status_raw {
        "verified" => ("verified", ExitKind::UserError, "", ""),
        "missing" => (
            "missing_signature",
            ExitKind::NotFound,
            "E_RELEASE_SIGNATURE_MISSING",
            "no Sigstore signature is shipped with this artifact; obtain the signed release",
        ),
        "mismatch" => (
            "mismatch",
            ExitKind::Validation,
            "E_RELEASE_SIGNATURE_MISMATCH",
            "release signature does not match the installed artifact; do not trust this binary and reinstall from a verified source",
        ),
        _ => (
            "unverifiable",
            ExitKind::Validation,
            "E_RELEASE_VERIFICATION_UNAVAILABLE",
            "release manifest does not declare a verifiable signature status; rebuild release evidence with valid signature metadata",
        ),
    };

    emit_release_verify(
        output,
        ReleaseVerifyOutcome {
            status,
            exit_kind,
            error_code,
            manifest_path: &manifest_path,
            artifact_path: artifact_path.as_deref(),
            signature_path: signature_path.as_deref(),
            signature_status: Some(signature_status_raw),
            issuer: issuer.as_deref(),
            transparency_log_index: log_index.as_deref(),
            remediation,
        },
    )
}

struct ReleaseVerifyOutcome<'a> {
    status: &'static str,
    exit_kind: ExitKind,
    error_code: &'static str,
    manifest_path: &'a Path,
    artifact_path: Option<&'a str>,
    signature_path: Option<&'a str>,
    signature_status: Option<&'a str>,
    issuer: Option<&'a str>,
    transparency_log_index: Option<&'a str>,
    remediation: &'static str,
}

fn emit_release_verify(
    output: OutputFormat,
    outcome: ReleaseVerifyOutcome<'_>,
) -> Result<(), CliError> {
    let manifest_string = outcome.manifest_path.display().to_string();
    let warning = if outcome.status == "verified" {
        None
    } else {
        Some(DiagnosticWarning::new(
            outcome.error_code,
            outcome.remediation.to_owned(),
        ))
    };
    let warnings: Vec<DiagnosticWarning> = warning.into_iter().collect();

    match output {
        OutputFormat::Human => {
            println!(
                "release verify: status={} manifest={}",
                outcome.status, manifest_string
            );
            if let Some(path) = outcome.artifact_path {
                println!("  artifact={path}");
            }
            if let Some(path) = outcome.signature_path {
                println!("  signature={path}");
            }
            if let Some(status) = outcome.signature_status {
                println!("  signature_status={status}");
            }
            if let Some(issuer) = outcome.issuer {
                println!("  issuer={issuer}");
            }
            if let Some(index) = outcome.transparency_log_index {
                println!("  transparency_log_index={index}");
            }
            if !outcome.remediation.is_empty() {
                println!("  remediation: {}", outcome.remediation);
            }
            for warning in &warnings {
                eprintln!("{}: {}", warning.code, warning.message);
            }
        }
        OutputFormat::Json => {
            let data = serde_json::json!({
                "status": outcome.status,
                "manifest_path": manifest_string,
                "artifact_path": outcome.artifact_path,
                "signature_path": outcome.signature_path,
                "signature_status": outcome.signature_status,
                "issuer": outcome.issuer,
                "transparency_log_index": outcome.transparency_log_index,
                "error_code": if outcome.error_code.is_empty() { None } else { Some(outcome.error_code) },
                "remediation": if outcome.remediation.is_empty() { None } else { Some(outcome.remediation) },
            });
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": READ_SCHEMA_VERSION,
                    "command": "release",
                    "status": "ok",
                    "data": data,
                    "warnings": warnings.iter().map(warning_json).collect::<Vec<_>>(),
                })
            );
        }
        OutputFormat::Ndjson => unreachable!("ndjson rejected before release rendering"),
    }
    if outcome.status == "verified" {
        Ok(())
    } else {
        Err(CliError::new(
            outcome.error_code,
            outcome.remediation.to_owned(),
            outcome.exit_kind,
        ))
    }
}

fn release_sbom(options: ReleaseOptions, output: OutputFormat) -> Result<(), CliError> {
    if output == OutputFormat::Ndjson {
        return Err(ndjson_not_supported("release sbom"));
    }
    let (manifest_path, manifest) = load_release_manifest(options.manifest_path.as_ref())?;
    let manifest_string = manifest_path.display().to_string();
    let manifest = match manifest {
        Some(value) => value,
        None => {
            return emit_release_sbom_unavailable(
                output,
                &manifest_string,
                "manifest_missing",
                "release manifest does not exist at the configured path",
            );
        }
    };

    let sbom = manifest.get("sbom").cloned().unwrap_or(serde_json::Value::Null);
    let status_raw = sbom
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unavailable");
    if status_raw != "available" {
        return emit_release_sbom_unavailable(
            output,
            &manifest_string,
            status_raw,
            "the installed artifact does not have a CycloneDX SBOM published with it",
        );
    }
    let format = sbom
        .get("format")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("cyclonedx-json");
    let path = sbom
        .get("path")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let components = sbom
        .get("components_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let generated_at = sbom
        .get("generated_at")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);

    let data = serde_json::json!({
        "status": "available",
        "manifest_path": manifest_string,
        "format": format,
        "path": path,
        "components_count": components,
        "generated_at": generated_at,
    });
    match output {
        OutputFormat::Human => {
            println!(
                "release sbom: status=available format={format} components={components}"
            );
            if let Some(path) = data["path"].as_str() {
                println!("  path={path}");
            }
            if let Some(generated) = data["generated_at"].as_str() {
                println!("  generated_at={generated}");
            }
            Ok(())
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": READ_SCHEMA_VERSION,
                    "command": "release",
                    "status": "ok",
                    "data": data,
                    "warnings": [],
                })
            );
            Ok(())
        }
        OutputFormat::Ndjson => unreachable!("ndjson rejected before release rendering"),
    }
}

fn emit_release_sbom_unavailable(
    output: OutputFormat,
    manifest_path: &str,
    reason: &str,
    remediation: &str,
) -> Result<(), CliError> {
    let warning = DiagnosticWarning::new(
        "W_RELEASE_SBOM_UNAVAILABLE",
        format!("SBOM unavailable: {reason}"),
    );
    let data = serde_json::json!({
        "status": "unavailable",
        "manifest_path": manifest_path,
        "reason": reason,
        "remediation": remediation,
    });
    match output {
        OutputFormat::Human => {
            println!(
                "release sbom: status=unavailable manifest={manifest_path} reason={reason}"
            );
            println!("  remediation: {remediation}");
            eprintln!("{}: {}", warning.code, warning.message);
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": READ_SCHEMA_VERSION,
                    "command": "release",
                    "status": "ok",
                    "data": data,
                    "warnings": [warning_json(&warning)],
                })
            );
        }
        OutputFormat::Ndjson => unreachable!("ndjson rejected before release rendering"),
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
struct AuditVerifyOptions {
    evidence_path: Option<PathBuf>,
}

fn parse_audit_verify_options(args: &[String]) -> Result<AuditVerifyOptions, CliError> {
    let mut options = AuditVerifyOptions::default();
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
        } else {
            return Err(CliError::new(
                "E_UNSUPPORTED_COMMAND",
                format!("unsupported zornmesh audit argument '{arg}'"),
                ExitKind::UserError,
            ));
        }
    }
    Ok(options)
}

fn run_audit(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_help("audit help", AUDIT_HELP, invocation.output),
        [flag] if is_help(flag) => print_help("audit help", AUDIT_HELP, invocation.output),
        [command, rest @ ..] if command == "verify" => {
            let options = parse_audit_verify_options(rest)?;
            audit_verify(options, invocation.output)
        }
        [command, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh audit subcommand '{command}'"),
            ExitKind::UserError,
        )),
    }
}

fn audit_verify(options: AuditVerifyOptions, output: OutputFormat) -> Result<(), CliError> {
    if output == OutputFormat::Ndjson {
        return Err(ndjson_not_supported("audit verify"));
    }
    let evidence_path = resolve_evidence_path(options.evidence_path.as_ref())?.ok_or_else(|| {
        CliError::new(
            "E_AUDIT_EVIDENCE_PATH_REQUIRED",
            format!(
                "evidence store path is not configured; pass --evidence <PATH> or set {ENV_EVIDENCE_PATH}"
            ),
            ExitKind::UserError,
        )
    })?;

    let metadata = fs::metadata(&evidence_path);
    if let Err(error) = &metadata
        && error.kind() == std::io::ErrorKind::NotFound
    {
        return emit_audit_outcome(
            output,
            AuditVerificationOutcome {
                status: "missing_store",
                exit_kind: ExitKind::NotFound,
                evidence_path: &evidence_path,
                audit_entries: 0,
                first_break: None,
                error_code: "E_AUDIT_STORE_MISSING",
                remediation: "evidence store does not exist at the configured path; verify the daemon has written records or correct --evidence",
            },
        );
    }
    if let Err(error) = &metadata {
        return emit_audit_outcome(
            output,
            AuditVerificationOutcome {
                status: "unreadable",
                exit_kind: ExitKind::Io,
                evidence_path: &evidence_path,
                audit_entries: 0,
                first_break: Some(format!("evidence store metadata unavailable: {error}")),
                error_code: "E_AUDIT_STORE_UNREADABLE",
                remediation: "check filesystem permissions and that the path is a regular file",
            },
        );
    }

    match FileEvidenceStore::open_evidence(&evidence_path) {
        Ok(store) => {
            let count = store.audit_entries().len();
            emit_audit_outcome(
                output,
                AuditVerificationOutcome {
                    status: "valid",
                    exit_kind: ExitKind::UserError,
                    evidence_path: &evidence_path,
                    audit_entries: count,
                    first_break: None,
                    error_code: "",
                    remediation: "",
                },
            )
        }
        Err(error) => {
            let (status, exit_kind, code, remediation) = match error.code() {
                EvidenceStoreErrorCode::FutureSchema => (
                    "unsupported_schema",
                    ExitKind::Validation,
                    "E_AUDIT_UNSUPPORTED_SCHEMA",
                    "the evidence store was written by a newer schema version; upgrade the verifier to read it",
                ),
                EvidenceStoreErrorCode::Corrupt | EvidenceStoreErrorCode::Validation => (
                    "tampered",
                    ExitKind::Validation,
                    "E_AUDIT_VERIFICATION_FAILED",
                    "audit chain broken; do not trust this evidence and investigate before further use",
                ),
                EvidenceStoreErrorCode::MigrationLocked => (
                    "unreadable",
                    ExitKind::TemporaryUnavailable,
                    "E_AUDIT_STORE_LOCKED",
                    "evidence store is locked for migration; retry once the migration completes",
                ),
                EvidenceStoreErrorCode::Io => (
                    "unreadable",
                    ExitKind::Io,
                    "E_AUDIT_STORE_UNREADABLE",
                    "the evidence store could not be read; check filesystem permissions and integrity",
                ),
            };
            emit_audit_outcome(
                output,
                AuditVerificationOutcome {
                    status,
                    exit_kind,
                    evidence_path: &evidence_path,
                    audit_entries: 0,
                    first_break: Some(error.message().to_owned()),
                    error_code: code,
                    remediation,
                },
            )
        }
    }
}

struct AuditVerificationOutcome<'a> {
    status: &'static str,
    exit_kind: ExitKind,
    evidence_path: &'a Path,
    audit_entries: usize,
    first_break: Option<String>,
    error_code: &'static str,
    remediation: &'static str,
}

fn emit_audit_outcome(
    output: OutputFormat,
    outcome: AuditVerificationOutcome<'_>,
) -> Result<(), CliError> {
    let path_string = outcome.evidence_path.display().to_string();
    let warning = if outcome.status == "valid" {
        None
    } else {
        Some(DiagnosticWarning::new(
            outcome.error_code,
            outcome
                .first_break
                .clone()
                .unwrap_or_else(|| outcome.remediation.to_owned()),
        ))
    };
    let warnings: Vec<DiagnosticWarning> = warning.into_iter().collect();

    match output {
        OutputFormat::Human => {
            println!(
                "audit verify: status={} entries={} evidence={}",
                outcome.status, outcome.audit_entries, path_string
            );
            if let Some(first_break) = &outcome.first_break {
                println!("  first_break: {first_break}");
            }
            if !outcome.remediation.is_empty() {
                println!("  remediation: {}", outcome.remediation);
            }
            for warning in &warnings {
                eprintln!("{}: {}", warning.code, warning.message);
            }
        }
        OutputFormat::Json => {
            let data = serde_json::json!({
                "status": outcome.status,
                "evidence_path": path_string,
                "audit_entries": outcome.audit_entries,
                "first_break": outcome.first_break,
                "error_code": if outcome.error_code.is_empty() { None } else { Some(outcome.error_code) },
                "remediation": if outcome.remediation.is_empty() { None } else { Some(outcome.remediation) },
            });
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": READ_SCHEMA_VERSION,
                    "command": "audit",
                    "status": "ok",
                    "data": data,
                    "warnings": warnings.iter().map(warning_json).collect::<Vec<_>>(),
                })
            );
        }
        OutputFormat::Ndjson => unreachable!("ndjson rejected before audit rendering"),
    }

    if outcome.status == "valid" {
        Ok(())
    } else {
        Err(CliError::new(
            outcome.error_code,
            outcome
                .first_break
                .unwrap_or_else(|| outcome.remediation.to_owned()),
            outcome.exit_kind,
        ))
    }
}

#[derive(Debug, Clone, Default)]
struct RetentionPlanOptions {
    evidence_path: Option<PathBuf>,
    max_age_ms: Option<u64>,
    max_count: Option<usize>,
    now_unix_ms: Option<u64>,
}

fn parse_retention_plan_options(args: &[String]) -> Result<RetentionPlanOptions, CliError> {
    let mut options = RetentionPlanOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let (key, value, advance) = if let Some((k, v)) = arg.split_once('=') {
            (k.to_owned(), v.to_owned(), 1)
        } else {
            let value = inspect_option_value(args, index, arg)?.to_owned();
            (arg.clone(), value, 2)
        };
        match key.as_str() {
            "--evidence" | "--evidence-path" => {
                options.evidence_path = Some(parse_non_empty_path(&key, &value)?);
            }
            "--max-age-ms" => {
                let parsed: u64 = value.parse().map_err(|_| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        format!("--max-age-ms must be a non-negative integer, got '{value}'"),
                        ExitKind::Validation,
                    )
                })?;
                options.max_age_ms = Some(parsed);
            }
            "--max-count" => {
                let parsed: usize = value.parse().map_err(|_| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        format!("--max-count must be a non-negative integer, got '{value}'"),
                        ExitKind::Validation,
                    )
                })?;
                options.max_count = Some(parsed);
            }
            "--now-unix-ms" => {
                let parsed: u64 = value.parse().map_err(|_| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        format!("--now-unix-ms must be a non-negative integer, got '{value}'"),
                        ExitKind::Validation,
                    )
                })?;
                options.now_unix_ms = Some(parsed);
            }
            other => {
                return Err(CliError::new(
                    "E_UNSUPPORTED_COMMAND",
                    format!("unsupported zornmesh retention argument '{other}'"),
                    ExitKind::UserError,
                ));
            }
        }
        index += advance;
    }
    Ok(options)
}

fn run_retention(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    match args {
        [] => print_help("retention help", RETENTION_HELP, invocation.output),
        [flag] if is_help(flag) => print_help("retention help", RETENTION_HELP, invocation.output),
        [command, rest @ ..] if command == "plan" => {
            let options = parse_retention_plan_options(rest)?;
            retention_plan(options, invocation.output)
        }
        [command, ..] => Err(CliError::new(
            "E_UNSUPPORTED_COMMAND",
            format!("unsupported zornmesh retention subcommand '{command}'"),
            ExitKind::UserError,
        )),
    }
}

fn retention_plan(options: RetentionPlanOptions, output: OutputFormat) -> Result<(), CliError> {
    if output == OutputFormat::Ndjson {
        return Err(ndjson_not_supported("retention plan"));
    }
    let policy = RetentionPolicy::new(options.max_age_ms, options.max_count).map_err(|error| {
        CliError::new(
            "E_VALIDATION_FAILED",
            error.message().to_owned(),
            ExitKind::Validation,
        )
    })?;
    let evidence_path = resolve_evidence_path(options.evidence_path.as_ref())?.ok_or_else(|| {
        CliError::new(
            "E_EVIDENCE_STORE_UNAVAILABLE",
            format!(
                "evidence store path is not configured; pass --evidence <PATH> or set {ENV_EVIDENCE_PATH}"
            ),
            ExitKind::TemporaryUnavailable,
        )
    })?;
    let store = FileEvidenceStore::open_evidence(&evidence_path).map_err(|error| {
        CliError::new(
            "E_EVIDENCE_STORE_UNAVAILABLE",
            format!("evidence store is unavailable: {error}"),
            ExitKind::TemporaryUnavailable,
        )
    })?;
    let now_unix_ms = options
        .now_unix_ms
        .unwrap_or_else(current_unix_ms_for_retention);
    let report = store.plan_retention(&policy, now_unix_ms);

    let plan_state = retention_plan_state(&report);
    let next_actions = retention_next_actions(plan_state);
    let warnings = if plan_state == "purge_required" {
        vec![DiagnosticWarning::new(
            "W_RETENTION_GAP_PROJECTED",
            "applying this plan would create a retention gap; downstream trace and inspect output must surface that gap",
        )]
    } else {
        Vec::new()
    };

    match output {
        OutputFormat::Human => {
            println!(
                "retention: state={} max_age_ms={} max_count={} now_unix_ms={}",
                plan_state,
                options
                    .max_age_ms
                    .map_or_else(|| "none".to_owned(), |v| v.to_string()),
                options
                    .max_count
                    .map_or_else(|| "none".to_owned(), |v| v.to_string()),
                report.now_unix_ms,
            );
            println!(
                "  purgeable_envelopes={} retained_envelopes={} purgeable_dead_letters={} retained_dead_letters={}",
                report.purgeable_envelope_ids.len(),
                report.retained_envelope_count,
                report.purgeable_dead_letter_ids.len(),
                report.retained_dead_letter_count,
            );
            if let Some(checkpoint) = &report.retention_checkpoint {
                println!(
                    "  retention_checkpoint sequence_range={}..{} purge_reason={} purged_audit_count={} prior_hash={} last_hash={}",
                    checkpoint.sequence_start,
                    checkpoint.sequence_end,
                    checkpoint.purge_reason,
                    checkpoint.purged_count,
                    checkpoint.prior_audit_hash,
                    checkpoint.last_audit_hash,
                );
            }
            for action in &next_actions {
                println!("  next_action: {action}");
            }
            for warning in &warnings {
                eprintln!("{}: {}", warning.code, warning.message);
            }
            Ok(())
        }
        OutputFormat::Json => {
            let data = retention_report_json(&policy, &report, plan_state, &next_actions);
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": READ_SCHEMA_VERSION,
                    "command": "retention",
                    "status": "ok",
                    "data": data,
                    "warnings": warnings.iter().map(warning_json).collect::<Vec<_>>(),
                })
            );
            Ok(())
        }
        OutputFormat::Ndjson => unreachable!("ndjson rejected before retention rendering"),
    }
}

fn retention_plan_state(report: &RetentionReport) -> &'static str {
    if report.purgeable_envelope_ids.is_empty() && report.purgeable_dead_letter_ids.is_empty() {
        "no_purge_required"
    } else {
        "purge_required"
    }
}

fn retention_next_actions(state: &str) -> Vec<&'static str> {
    match state {
        "purge_required" => vec![
            "review_purgeable_records",
            "audit_verification",
            "schedule_commit",
        ],
        _ => Vec::new(),
    }
}

fn retention_report_json(
    policy: &RetentionPolicy,
    report: &RetentionReport,
    state: &str,
    next_actions: &[&'static str],
) -> serde_json::Value {
    serde_json::json!({
        "mode": "plan",
        "state": state,
        "policy": {
            "max_age_ms": policy.max_age_ms(),
            "max_envelope_count": policy.max_envelope_count(),
        },
        "now_unix_ms": report.now_unix_ms,
        "purgeable_envelope_ids": report.purgeable_envelope_ids,
        "purgeable_dead_letter_ids": report.purgeable_dead_letter_ids,
        "retained_envelope_count": report.retained_envelope_count,
        "retained_dead_letter_count": report.retained_dead_letter_count,
        "retention_checkpoint": report.retention_checkpoint.as_ref().map(|checkpoint| {
            serde_json::json!({
                "sequence_start": checkpoint.sequence_start,
                "sequence_end": checkpoint.sequence_end,
                "prior_audit_hash": checkpoint.prior_audit_hash,
                "last_audit_hash": checkpoint.last_audit_hash,
                "purge_reason": checkpoint.purge_reason,
                "purged_audit_count": checkpoint.purged_count,
            })
        }),
        "next_actions": next_actions,
    })
}

fn current_unix_ms_for_retention() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
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

fn cli_error_from_daemon(error: crate::daemon::DaemonError) -> CliError {
    let kind = match error.code() {
        crate::daemon::DaemonErrorCode::ExistingOwner => ExitKind::TemporaryUnavailable,
        crate::daemon::DaemonErrorCode::LocalTrustUnsafe
        | crate::daemon::DaemonErrorCode::ElevatedPrivilege => ExitKind::PermissionDenied,
        crate::daemon::DaemonErrorCode::DaemonUnreachable => ExitKind::DaemonUnreachable,
        crate::daemon::DaemonErrorCode::PersistenceUnavailable => ExitKind::TemporaryUnavailable,
        crate::daemon::DaemonErrorCode::InvalidConfig => ExitKind::UserError,
        crate::daemon::DaemonErrorCode::Io => ExitKind::Io,
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
    broker: crate::broker::Broker,
    agent_id: String,
    display_name: String,
    credentials: crate::broker::PeerCredentials,
    trust_policy: crate::broker::SocketTrustPolicy,
    state: BridgeState,
}

impl StdioBridge {
    pub fn new(
        broker: crate::broker::Broker,
        agent_id: impl Into<String>,
        display_name: impl Into<String>,
        credentials: crate::broker::PeerCredentials,
        trust_policy: crate::broker::SocketTrustPolicy,
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
        if !MCP_BRIDGE_PROTOCOL_VERSIONS.contains(&protocol_version.as_str()) {
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
        let card = crate::core::AgentCard::from_input(crate::core::AgentCardInput {
            profile_version: crate::core::AGENT_CARD_PROFILE_VERSION.to_owned(),
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
            crate::broker::AgentRegistrationOutcome::Registered { canonical }
            | crate::broker::AgentRegistrationOutcome::Compatible { canonical } => canonical,
            crate::broker::AgentRegistrationOutcome::Conflict { .. } => {
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
            Ok(crate::broker::ConnectionAcceptanceOutcome::Accepted { .. }) => {}
            Ok(crate::broker::ConnectionAcceptanceOutcome::Rejected { code, remediation }) => {
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
            crate::core::CapabilityDescriptor::builder(
                capability_id,
                "v1",
                crate::core::CapabilityDirection::Both,
            )
            .with_summary(summary)
            .with_schema_ref(
                crate::core::CapabilitySchemaDialect::JsonSchema,
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
            if let crate::broker::AuthorizationDecision::Denied { reason } = self
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
                    *value = serde_json::Value::String(crate::core::REDACTION_MARKER.to_owned());
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

#[derive(Debug, Clone, Default)]
struct UiOptions {
    preferred_port: Option<u16>,
    no_open: bool,
}

fn parse_ui_options(args: &[String]) -> Result<UiOptions, CliError> {
    let mut options = UiOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--no-open" {
            options.no_open = true;
            index += 1;
            continue;
        }
        let (key, value, advance) = if let Some((k, v)) = arg.split_once('=') {
            (k.to_owned(), v.to_owned(), 1)
        } else {
            let value = inspect_option_value(args, index, arg)?.to_owned();
            (arg.clone(), value, 2)
        };
        match key.as_str() {
            "--port" => {
                let port: u16 = value.parse().map_err(|_| {
                    CliError::new(
                        "E_VALIDATION_FAILED",
                        format!("--port must be a 0-65535 integer, got '{value}'"),
                        ExitKind::Validation,
                    )
                })?;
                options.preferred_port = Some(port);
            }
            other => {
                return Err(CliError::new(
                    "E_UNSUPPORTED_COMMAND",
                    format!("unsupported zornmesh ui argument '{other}'"),
                    ExitKind::UserError,
                ));
            }
        }
        index += advance;
    }
    Ok(options)
}

fn run_ui(args: &[String], invocation: &Invocation) -> Result<(), CliError> {
    if let Some(flag) = args.iter().find(|arg| is_help(arg)) {
        let _ = flag;
        return print_help("ui help", UI_HELP, invocation.output);
    }
    let options = parse_ui_options(args)?;
    if invocation.output == OutputFormat::Ndjson {
        return Err(ndjson_not_supported("ui"));
    }
    let session = UiLaunchSession::reserve(options.preferred_port.unwrap_or(UI_PREFERRED_PORT))?;
    let data = session.launch_report_json(options.no_open);
    match invocation.output {
        OutputFormat::Human => {
            println!(
                "ui launch: status=ready loopback_url={} port={} schema_version={} bundled_assets=offline",
                session.loopback_url(),
                session.port(),
                UI_SCHEMA_VERSION,
            );
            println!("  session_token_length={}", UI_TOKEN_HEX_LEN);
            println!("  csrf_token_length={}", UI_TOKEN_HEX_LEN);
            println!("  referrer_policy={UI_REFERRER_POLICY}");
            println!(
                "  open_browser={}",
                if options.no_open { "no" } else { "yes" }
            );
            Ok(())
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": READ_SCHEMA_VERSION,
                    "command": "ui",
                    "status": "ok",
                    "data": data,
                    "warnings": [],
                })
            );
            Ok(())
        }
        OutputFormat::Ndjson => unreachable!("ndjson rejected before ui rendering"),
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct UiLaunchSession {
    bind_addr: std::net::SocketAddr,
    session_token: String,
    csrf_token: String,
    revoked: std::cell::Cell<bool>,
}

#[allow(dead_code)]
impl UiLaunchSession {
    pub(crate) fn reserve(preferred_port: u16) -> Result<Self, CliError> {
        let mut tried: Vec<u16> = vec![preferred_port];
        tried.extend_from_slice(UI_FALLBACK_PORTS);
        for port in &tried {
            let addr: std::net::SocketAddr =
                ([127, 0, 0, 1], *port).into();
            match std::net::TcpListener::bind(addr) {
                Ok(listener) => {
                    drop(listener);
                    return Ok(Self {
                        bind_addr: addr,
                        session_token: random_hex_token(UI_TOKEN_HEX_LEN)?,
                        csrf_token: random_hex_token(UI_TOKEN_HEX_LEN)?,
                        revoked: std::cell::Cell::new(false),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => continue,
                Err(error) => {
                    return Err(CliError::new(
                        "E_UI_BIND_FAILED",
                        format!(
                            "loopback bind on 127.0.0.1:{port} failed with non-recoverable error: {error}"
                        ),
                        ExitKind::Io,
                    ));
                }
            }
        }
        Err(CliError::new(
            "UI_PORT_IN_USE",
            format!(
                "all candidate loopback ports are in use ({tried:?}); free a port or pass --port <PORT>"
            ),
            ExitKind::TemporaryUnavailable,
        ))
    }

    pub(crate) fn port(&self) -> u16 {
        self.bind_addr.port()
    }

    pub(crate) fn loopback_url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.bind_addr.port())
    }

    pub(crate) fn loopback_origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.bind_addr.port())
    }

    pub(crate) fn session_token(&self) -> &str {
        &self.session_token
    }

    pub(crate) fn csrf_token(&self) -> &str {
        &self.csrf_token
    }

    pub(crate) fn is_revoked(&self) -> bool {
        self.revoked.get()
    }

    pub(crate) fn revoke(&self) {
        self.revoked.set(true);
    }

    pub(crate) fn validate_session_token(&self, provided: Option<&str>) -> UiTokenOutcome {
        if self.is_revoked() {
            return UiTokenOutcome::Revoked;
        }
        match provided {
            None => UiTokenOutcome::Missing,
            Some(value) if value == self.session_token => UiTokenOutcome::Verified,
            Some(_) => UiTokenOutcome::Invalid,
        }
    }

    pub(crate) fn validate_origin(&self, origin: Option<&str>) -> UiOriginOutcome {
        match origin {
            None => UiOriginOutcome::Missing,
            Some(value) if value == self.loopback_origin() => UiOriginOutcome::Allowed,
            Some(_) => UiOriginOutcome::Rejected,
        }
    }

    pub(crate) fn validate_csrf(&self, provided: Option<&str>) -> UiCsrfOutcome {
        if self.is_revoked() {
            return UiCsrfOutcome::Revoked;
        }
        match provided {
            None => UiCsrfOutcome::Missing,
            Some(value) if value == self.csrf_token => UiCsrfOutcome::Verified,
            Some(_) => UiCsrfOutcome::Invalid,
        }
    }

    pub(crate) fn launch_report_json(&self, no_open: bool) -> serde_json::Value {
        serde_json::json!({
            "schema_version": UI_SCHEMA_VERSION,
            "status": "ready",
            "loopback_url": self.loopback_url(),
            "loopback_origin": self.loopback_origin(),
            "port": self.port(),
            "open_browser": !no_open,
            "session_token_length": UI_TOKEN_HEX_LEN,
            "csrf_token_length": UI_TOKEN_HEX_LEN,
            "referrer_policy": UI_REFERRER_POLICY,
            "bundled_assets": "offline",
            "non_loopback_bind_refused": true,
            "actor_session_binding": "server_derived",
            "websocket_sse_session_required": true,
            "cors_allowed_origin": self.loopback_origin(),
            "schema_version_pinned": UI_SCHEMA_VERSION,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiTokenOutcome {
    Verified,
    Missing,
    Invalid,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiOriginOutcome {
    Allowed,
    Missing,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCsrfOutcome {
    Verified,
    Missing,
    Invalid,
    Revoked,
}

fn random_hex_token(hex_len: usize) -> Result<String, CliError> {
    debug_assert!(hex_len % 2 == 0, "hex token length must be even");
    let bytes = hex_len / 2;
    let mut buffer = vec![0u8; bytes];
    let mut file = fs::File::open("/dev/urandom").map_err(|error| {
        CliError::new(
            "E_UI_TOKEN_ENTROPY",
            format!("could not read /dev/urandom for session token: {error}"),
            ExitKind::Io,
        )
    })?;
    use std::io::Read;
    file.read_exact(&mut buffer).map_err(|error| {
        CliError::new(
            "E_UI_TOKEN_ENTROPY",
            format!("could not read {bytes} bytes of entropy: {error}"),
            ExitKind::Io,
        )
    })?;
    let mut hex = String::with_capacity(hex_len);
    for byte in &buffer {
        hex.push_str(&format!("{byte:02x}"));
    }
    Ok(hex)
}

pub fn ui_referrer_policy() -> &'static str {
    UI_REFERRER_POLICY
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum UiConnectionStatus {
    Starting,
    Ready,
    Reconnecting,
    Degraded,
    Unavailable,
    Stale,
    SchemaMismatch,
    SessionExpired,
}

impl UiConnectionStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Reconnecting => "reconnecting",
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
            Self::Stale => "stale",
            Self::SchemaMismatch => "schema_mismatch",
            Self::SessionExpired => "session_expired",
        }
    }

    pub(crate) const fn allows_unsafe_actions(self) -> bool {
        matches!(self, Self::Ready)
    }

    pub(crate) const fn disabled_action_reason(self) -> Option<&'static str> {
        match self {
            Self::Ready => None,
            Self::Starting => {
                Some("daemon is starting; wait for the protected UI session to become ready")
            }
            Self::Reconnecting => Some("daemon reconnect/backfill is in progress"),
            Self::Degraded => Some(
                "reconnect/backfill did not complete; retry or inspect evidence before sending",
            ),
            Self::Unavailable => {
                Some("daemon is unavailable; reconnect before running state-changing actions")
            }
            Self::Stale => Some("view is stale; refresh backfill before trusting action scope"),
            Self::SchemaMismatch => {
                Some("daemon schema changed; update the client before running actions")
            }
            Self::SessionExpired => {
                Some("UI session expired; start a new protected local UI session")
            }
        }
    }

    pub(crate) const fn status_copy(self) -> &'static str {
        match self {
            Self::Starting => "Starting local daemon UI session.",
            Self::Ready => "Connected with complete loaded evidence for the current view.",
            Self::Reconnecting => "Reconnecting and backfilling daemon-sequence evidence.",
            Self::Degraded => "Recovery is degraded; partial state remains visible.",
            Self::Unavailable => "Daemon is unavailable; evidence may be incomplete.",
            Self::Stale => "Current view is stale until backfill refreshes it.",
            Self::SchemaMismatch => "Daemon schema changed; this UI cannot safely act on it.",
            Self::SessionExpired => "Session expired; launch a new protected UI session.",
        }
    }

    pub(crate) const fn default_recovery_cue(self) -> Option<&'static str> {
        match self {
            Self::Ready => None,
            Self::Starting => Some("wait_for_daemon_ready"),
            Self::Reconnecting => Some("wait_for_backfill"),
            Self::Degraded => Some("retry_reconnect_or_inspect_trace"),
            Self::Unavailable => Some("retry_reconnect_or_check_doctor"),
            Self::Stale => Some("refresh_backfill_window"),
            Self::SchemaMismatch => Some("upgrade_client_or_export_audit"),
            Self::SessionExpired => Some("rerun_zornmesh_ui"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum BackfillOutcome {
    Restored,
    RestoredPartial,
    Failed,
}

impl BackfillOutcome {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Restored => "restored",
            Self::RestoredPartial => "restored_partial",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct UiContextSelection {
    pub selected_correlation_id: Option<String>,
    pub selected_agent_id: Option<String>,
    pub active_filters: Vec<(String, String)>,
    pub view_mode: String,
    pub selected_message_id: Option<String>,
}

#[allow(dead_code)]
impl UiContextSelection {
    pub(crate) fn empty() -> Self {
        Self {
            selected_correlation_id: None,
            selected_agent_id: None,
            active_filters: Vec::new(),
            view_mode: "control_room".to_owned(),
            selected_message_id: None,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct UiContext {
    pub status: UiConnectionStatus,
    pub selection: UiContextSelection,
    pub timeline: TimelinePage,
    pub last_outcome: Option<BackfillOutcome>,
    pub recovery_cue: Option<&'static str>,
    pub mapping_version: &'static str,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct BackfillBatch {
    pub events: Vec<TimelineEvent>,
    pub total_count: usize,
    pub gaps: Vec<TimelineGapMarker>,
    pub partial_window: bool,
}

#[allow(dead_code)]
impl UiContext {
    pub(crate) fn ready(selection: UiContextSelection, timeline: TimelinePage) -> Self {
        let mut timeline = timeline;
        timeline.selected_message_id = selection
            .selected_message_id
            .clone()
            .filter(|id| timeline.events.iter().any(|event| &event.message_id == id));
        Self {
            status: UiConnectionStatus::Ready,
            selection,
            timeline,
            last_outcome: None,
            recovery_cue: None,
            mapping_version: "zornmesh.ui.context.v1",
        }
    }

    pub(crate) fn start_reconnect(mut self) -> Self {
        self.status = UiConnectionStatus::Reconnecting;
        self.last_outcome = None;
        self
    }

    pub(crate) fn daemon_status_changed(mut self, status: UiConnectionStatus) -> Self {
        self.status = status;
        self.recovery_cue = status.default_recovery_cue();
        self
    }

    pub(crate) fn complete_backfill(mut self, batch: BackfillBatch) -> Self {
        let prior_selected = self.selection.selected_message_id.clone();
        let mut seen: BTreeSet<(u64, String)> = self
            .timeline
            .events
            .iter()
            .map(TimelineEvent::stable_identity)
            .collect();
        let mut merged = self.timeline.events.clone();
        for event in batch.events {
            if seen.insert(event.stable_identity()) {
                merged.push(event);
            }
        }
        merged.sort_by_key(|event| event.daemon_sequence);
        let total_count = batch.total_count.max(merged.len());
        let window = merged
            .first()
            .zip(merged.last())
            .map_or((0, 0), |(first, last)| {
                (first.daemon_sequence, last.daemon_sequence)
            });
        let mut new_timeline = TimelinePage::paginate(merged, window, total_count, batch.gaps);
        if batch.partial_window {
            new_timeline.partial_window = true;
            new_timeline.condition = TimelinePanelCondition::PartialWindow;
        }
        let partial = new_timeline.partial_window;
        self.timeline = new_timeline;
        self.selection.selected_message_id = prior_selected.filter(|id| {
            self.timeline
                .events
                .iter()
                .any(|event| &event.message_id == id)
        });
        self.timeline.selected_message_id = self.selection.selected_message_id.clone();
        self.status = UiConnectionStatus::Ready;
        self.last_outcome = Some(if partial {
            BackfillOutcome::RestoredPartial
        } else {
            BackfillOutcome::Restored
        });
        self.recovery_cue = if partial {
            Some("load_more_pages_for_full_window")
        } else {
            None
        };
        self
    }

    pub(crate) fn fail_backfill(mut self, reason: &'static str) -> Self {
        self.status = UiConnectionStatus::Degraded;
        self.last_outcome = Some(BackfillOutcome::Failed);
        self.recovery_cue = Some(reason);
        self
    }

    fn status_copy(&self) -> &'static str {
        if self.status == UiConnectionStatus::Ready && self.timeline.partial_window {
            "Loaded a daemon-sequence window; additional pages are required before the trace is complete."
        } else {
            self.status.status_copy()
        }
    }

    fn recovery_next_actions(&self) -> Vec<&'static str> {
        let mut actions = Vec::new();
        if self.timeline.partial_window {
            actions.push("load_more_pages");
        }
        if matches!(self.last_outcome, Some(BackfillOutcome::Failed))
            || matches!(
                self.status,
                UiConnectionStatus::Degraded
                    | UiConnectionStatus::Unavailable
                    | UiConnectionStatus::Stale
            )
        {
            actions.extend([
                "retry_reconnect",
                "inspect_trace_by_cli",
                "inspect_daemon_health",
                "export_audit_evidence",
            ]);
        } else {
            match self.status {
                UiConnectionStatus::Starting | UiConnectionStatus::Reconnecting => {
                    actions.extend(["inspect_daemon_health"]);
                }
                UiConnectionStatus::SchemaMismatch => {
                    actions.extend(["inspect_daemon_health", "export_audit_evidence"]);
                }
                UiConnectionStatus::SessionExpired => {
                    actions.extend([
                        "rerun_zornmesh_ui",
                        "inspect_trace_by_cli",
                        "export_audit_evidence",
                    ]);
                }
                UiConnectionStatus::Ready
                | UiConnectionStatus::Degraded
                | UiConnectionStatus::Unavailable
                | UiConnectionStatus::Stale => {}
            }
        }
        actions.sort_unstable();
        actions.dedup();
        actions
    }

    fn recovery_panel_json(&self) -> Option<serde_json::Value> {
        let actions = self.recovery_next_actions();
        let reason = self
            .recovery_cue
            .or_else(|| self.status.default_recovery_cue())
            .or_else(|| {
                self.timeline
                    .partial_window
                    .then_some("partial_trace_window")
            });
        if reason.is_none() && actions.is_empty() {
            return None;
        }
        Some(serde_json::json!({
            "reason": reason,
            "partial_state_visible": !self.timeline.events.is_empty(),
            "next_actions": actions,
        }))
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema_version": self.mapping_version,
            "status": self.status.as_str(),
            "allows_unsafe_actions": self.status.allows_unsafe_actions(),
            "disabled_action_reason": self.status.disabled_action_reason(),
            "status_copy": self.status_copy(),
            "last_outcome": self.last_outcome.map(|outcome| outcome.as_str()),
            "recovery_cue": self.recovery_cue,
            "recovery_panel": self.recovery_panel_json(),
            "selection": {
                "selected_correlation_id": self.selection.selected_correlation_id,
                "selected_agent_id": self.selection.selected_agent_id,
                "selected_message_id": self.selection.selected_message_id,
                "active_filters": self.selection.active_filters.iter().map(|(k, v)| {
                    serde_json::json!({"key": k, "value": v})
                }).collect::<Vec<_>>(),
                "view_mode": self.selection.view_mode,
            },
            "timeline": self.timeline.to_json(),
        })
    }
}

#[cfg(test)]
mod ui_context_tests {
    use super::*;

    fn event(seq: u64, message: &str) -> TimelineEvent {
        TimelineEvent {
            daemon_sequence: seq,
            message_id: message.to_owned(),
            correlation_id: "corr-context".to_owned(),
            trace_id: "trace-context".to_owned(),
            source_agent: "agent.local/sender".to_owned(),
            target_or_subject: "agent.local/target".to_owned(),
            subject: "mesh.context.created".to_owned(),
            timestamp_unix_ms: 1_700_000_000_000 + seq,
            browser_received_unix_ms: None,
            state: TimelineEventState::Accepted,
            causal_marker: TimelineCausalMarker::Root,
            parent_message_id: None,
            safe_payload_summary: serde_json::json!({}),
            suggested_next_action: None,
            cli_handoff_command: None,
            recovery_cue: None,
        }
    }

    fn ready_context() -> UiContext {
        let timeline = TimelinePage::ready(vec![event(1, "msg-1"), event(2, "msg-2")]);
        let mut selection = UiContextSelection::empty();
        selection.selected_correlation_id = Some("corr-context".to_owned());
        selection.selected_agent_id = Some("agent.local/sender".to_owned());
        selection.selected_message_id = Some("msg-2".to_owned());
        selection.view_mode = "focused_trace".to_owned();
        selection
            .active_filters
            .push(("status".to_owned(), "active".to_owned()));
        UiContext::ready(selection, timeline)
    }

    fn event_with_marker(seq: u64, message: &str, marker: TimelineCausalMarker) -> TimelineEvent {
        let mut event = event(seq, message);
        event.causal_marker = marker;
        event
    }

    #[test]
    fn ready_context_allows_unsafe_actions_and_pins_schema() {
        let context = ready_context();
        assert_eq!(context.status, UiConnectionStatus::Ready);
        assert!(context.status.allows_unsafe_actions());
        assert_eq!(context.mapping_version, "zornmesh.ui.context.v1");
        assert_eq!(
            context.to_json()["schema_version"],
            "zornmesh.ui.context.v1"
        );
    }

    #[test]
    fn start_reconnect_transitions_to_reconnecting_and_blocks_unsafe_actions() {
        let context = ready_context().start_reconnect();
        assert_eq!(context.status, UiConnectionStatus::Reconnecting);
        assert!(!context.status.allows_unsafe_actions());
    }

    #[test]
    fn complete_backfill_restores_selection_dedupes_by_stable_identity_and_orders_by_sequence() {
        let context = ready_context().start_reconnect();
        let batch = BackfillBatch {
            events: vec![
                event(2, "msg-2"),
                event(3, "msg-3"),
                event(1, "msg-1"),
                event(4, "msg-4"),
            ],
            total_count: 4,
            gaps: Vec::new(),
            partial_window: false,
        };
        let restored = context.complete_backfill(batch);
        assert_eq!(restored.status, UiConnectionStatus::Ready);
        assert_eq!(restored.last_outcome, Some(BackfillOutcome::Restored));
        let sequences: Vec<u64> = restored
            .timeline
            .events
            .iter()
            .map(|e| e.daemon_sequence)
            .collect();
        assert_eq!(
            sequences,
            vec![1, 2, 3, 4],
            "ordered by daemon sequence with no duplicates"
        );
        assert_eq!(
            restored.selection.selected_message_id.as_deref(),
            Some("msg-2"),
            "selected detail survives backfill"
        );
    }

    #[test]
    fn complete_backfill_updates_timeline_selected_detail() {
        let restored = ready_context()
            .start_reconnect()
            .complete_backfill(BackfillBatch {
                events: vec![event(3, "msg-3")],
                total_count: 3,
                gaps: Vec::new(),
                partial_window: false,
            });

        assert_eq!(
            restored.timeline.selected_message_id.as_deref(),
            Some("msg-2"),
            "timeline panel keeps the selected detail stable after restore"
        );
        assert_eq!(
            restored.to_json()["timeline"]["selected_message_id"],
            "msg-2"
        );
    }

    #[test]
    fn partial_backfill_reports_actual_loaded_window_not_total_trace() {
        let mut selection = UiContextSelection::empty();
        selection.selected_correlation_id = Some("corr-context".to_owned());
        selection.selected_message_id = Some("msg-50".to_owned());
        selection.view_mode = "focused_trace".to_owned();
        let context = UiContext::ready(selection, TimelinePage::ready(vec![event(50, "msg-50")]))
            .start_reconnect();

        let restored = context.complete_backfill(BackfillBatch {
            events: vec![
                event(49, "msg-49"),
                event(50, "msg-50"),
                event(51, "msg-51"),
            ],
            total_count: 1_000,
            gaps: Vec::new(),
            partial_window: true,
        });

        assert_eq!(
            restored.timeline.condition,
            TimelinePanelCondition::PartialWindow
        );
        assert_eq!(restored.timeline.loaded_range, Some((49, 51)));
        assert_eq!(restored.to_json()["timeline"]["loaded_range"]["low"], 49);
        assert_eq!(restored.to_json()["timeline"]["loaded_range"]["high"], 51);
        assert_eq!(
            restored.recovery_cue,
            Some("load_more_pages_for_full_window")
        );
    }

    #[test]
    fn complete_backfill_partial_window_marks_partial_and_load_more_cue() {
        let context = ready_context().start_reconnect();
        let batch = BackfillBatch {
            events: vec![event(3, "msg-3")],
            total_count: 100,
            gaps: vec![TimelineGapMarker {
                before_sequence: 2,
                after_sequence: 3,
                reason: "retention_purge",
            }],
            partial_window: true,
        };
        let restored = context.complete_backfill(batch);
        assert_eq!(
            restored.last_outcome,
            Some(BackfillOutcome::RestoredPartial)
        );
        assert_eq!(
            restored.recovery_cue,
            Some("load_more_pages_for_full_window")
        );
        assert_eq!(
            restored.timeline.condition,
            TimelinePanelCondition::PartialWindow
        );
        assert!(
            restored
                .timeline
                .gaps
                .iter()
                .any(|gap| gap.reason == "retention_purge")
        );
    }

    #[test]
    fn complete_backfill_clears_selection_when_event_no_longer_present() {
        let mut selection = UiContextSelection::empty();
        selection.selected_message_id = Some("msg-vanished".to_owned());
        let context = UiContext::ready(selection, TimelinePage::ready(vec![event(1, "msg-1")]))
            .start_reconnect();
        let restored = context.complete_backfill(BackfillBatch {
            events: vec![event(2, "msg-2")],
            total_count: 2,
            gaps: Vec::new(),
            partial_window: false,
        });
        assert!(restored.selection.selected_message_id.is_none());
    }

    #[test]
    fn daemon_status_changed_carries_recovery_cues_for_unavailable_states() {
        let unavailable = ready_context().daemon_status_changed(UiConnectionStatus::Unavailable);
        assert_eq!(unavailable.status, UiConnectionStatus::Unavailable);
        assert_eq!(
            unavailable.recovery_cue,
            Some("retry_reconnect_or_check_doctor")
        );
        assert!(!unavailable.status.allows_unsafe_actions());

        let schema = ready_context().daemon_status_changed(UiConnectionStatus::SchemaMismatch);
        assert_eq!(schema.recovery_cue, Some("upgrade_client_or_export_audit"));

        let session = ready_context().daemon_status_changed(UiConnectionStatus::SessionExpired);
        assert_eq!(session.recovery_cue, Some("rerun_zornmesh_ui"));
    }

    #[test]
    fn non_ready_status_json_explains_disabled_actions() {
        for status in [
            UiConnectionStatus::Starting,
            UiConnectionStatus::Reconnecting,
            UiConnectionStatus::Degraded,
            UiConnectionStatus::Unavailable,
            UiConnectionStatus::Stale,
            UiConnectionStatus::SchemaMismatch,
            UiConnectionStatus::SessionExpired,
        ] {
            let json = ready_context().daemon_status_changed(status).to_json();
            assert_eq!(json["status"], status.as_str());
            assert_eq!(json["allows_unsafe_actions"], false);
            assert!(
                json["disabled_action_reason"]
                    .as_str()
                    .is_some_and(|reason| !reason.is_empty()),
                "non-ready status {status:?} must explain why unsafe actions are disabled"
            );
            assert!(
                json["status_copy"]
                    .as_str()
                    .is_some_and(|copy| !copy.is_empty()),
                "non-ready status {status:?} must render persistent status copy"
            );
        }
    }

    #[test]
    fn backfill_marks_retention_late_and_reconstructed_evidence() {
        let restored = ready_context()
            .start_reconnect()
            .complete_backfill(BackfillBatch {
                events: vec![
                    event_with_marker(3, "msg-late", TimelineCausalMarker::LateArrival),
                    event_with_marker(4, "msg-rebuilt", TimelineCausalMarker::Reconstructed),
                ],
                total_count: 4,
                gaps: vec![TimelineGapMarker {
                    before_sequence: 2,
                    after_sequence: 3,
                    reason: "retention_purge",
                }],
                partial_window: false,
            });

        let json = restored.to_json();
        let events = json["timeline"]["events"].as_array().unwrap();
        let late = events
            .iter()
            .find(|event| event["message_id"] == "msg-late")
            .expect("late event exists");
        assert_eq!(late["causal_marker"], "late_arrival");
        assert!(
            late["evidence_flags"]
                .as_array()
                .unwrap()
                .iter()
                .any(|flag| flag == "late")
        );

        let rebuilt = events
            .iter()
            .find(|event| event["message_id"] == "msg-rebuilt")
            .expect("reconstructed event exists");
        assert_eq!(rebuilt["causal_marker"], "reconstructed");
        assert!(
            rebuilt["evidence_flags"]
                .as_array()
                .unwrap()
                .iter()
                .any(|flag| flag == "reconstructed")
        );
        assert_eq!(json["timeline"]["gaps"][0]["reason"], "retention_purge");
    }

    #[test]
    fn failed_backfill_emits_evidence_gap_recovery_panel() {
        let context = ready_context().start_reconnect();
        let failed = context.fail_backfill("daemon_unreachable");
        assert_eq!(failed.status, UiConnectionStatus::Degraded);
        assert_eq!(failed.last_outcome, Some(BackfillOutcome::Failed));
        assert_eq!(failed.recovery_cue, Some("daemon_unreachable"));
        assert!(!failed.status.allows_unsafe_actions());
    }

    #[test]
    fn failed_backfill_keeps_partial_state_and_offers_recovery_actions() {
        let failed = ready_context()
            .start_reconnect()
            .fail_backfill("backfill_timeout");
        let json = failed.to_json();

        assert_eq!(json["timeline"]["events"].as_array().unwrap().len(), 2);
        assert_eq!(json["recovery_panel"]["reason"], "backfill_timeout");
        assert_eq!(json["recovery_panel"]["partial_state_visible"], true);
        let actions = json["recovery_panel"]["next_actions"].as_array().unwrap();
        for expected in [
            "retry_reconnect",
            "inspect_trace_by_cli",
            "inspect_daemon_health",
            "export_audit_evidence",
        ] {
            assert!(
                actions.iter().any(|action| action == expected),
                "recovery panel must offer {expected}"
            );
        }
    }

    #[test]
    fn json_carries_persistent_status_chrome_and_selection() {
        let context = ready_context();
        let json = context.to_json();
        assert_eq!(json["status"], "ready");
        assert_eq!(json["allows_unsafe_actions"], true);
        let selection = &json["selection"];
        assert_eq!(selection["selected_correlation_id"], "corr-context");
        assert_eq!(selection["selected_agent_id"], "agent.local/sender");
        assert_eq!(selection["selected_message_id"], "msg-2");
        assert_eq!(selection["view_mode"], "focused_trace");
        let filters = selection["active_filters"].as_array().unwrap();
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0]["key"], "status");
        assert_eq!(filters[0]["value"], "active");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum BroadcastExclusionReason {
    Incompatible,
    Stale,
    Disconnected,
    DeniedByAllowlist,
    UnsafeScope,
}

impl BroadcastExclusionReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Incompatible => "incompatible_capability",
            Self::Stale => "stale_recipient",
            Self::Disconnected => "disconnected_recipient",
            Self::DeniedByAllowlist => "denied_by_allowlist",
            Self::UnsafeScope => "unsafe_scope",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum BroadcastAggregateOutcome {
    Pending,
    Success,
    PartialSuccess,
    AllFailed,
}

impl BroadcastAggregateOutcome {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Success => "success",
            Self::PartialSuccess => "partial_success",
            Self::AllFailed => "all_failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum BroadcastSubmitError {
    NoConfirmation,
    DuplicateInFlight,
    SnapshotDrift,
    ValidationFailed(DirectComposerValidation),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct BroadcastExclusion {
    pub recipient: RosterEntry,
    pub reason: BroadcastExclusionReason,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct RecipientOutcomeRow {
    pub agent_id: String,
    pub display_name: String,
    pub outcome: DirectSendOutcome,
    pub failure_reason: Option<&'static str>,
    pub recorded_unix_ms: Option<u64>,
    pub retry_handoff_available: bool,
    pub inspect_handoff_available: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct BroadcastConfirmation {
    pub snapshot_revision: u64,
    pub included_recipient_ids: Vec<String>,
    pub excluded_recipient_ids: Vec<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct BroadcastAuditLink {
    pub actor_session: String,
    pub correlation_id: String,
    pub trace_id: String,
    pub requested_recipient_ids: Vec<String>,
    pub previewed_snapshot_revision: u64,
    pub accepted_snapshot_revision: u64,
    pub actual_recipient_ids: Vec<String>,
    pub excluded_recipient_ids: Vec<String>,
    pub drift_reconfirmation_required: bool,
    pub safe_payload_summary: serde_json::Value,
    pub per_recipient_outcomes: Vec<(String, &'static str)>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct BroadcastComposerState {
    pub mode: DirectComposerMode,
    pub draft: DirectMessageDraft,
    pub validation: DirectComposerValidation,
    pub included: Vec<RosterEntry>,
    pub excluded: Vec<BroadcastExclusion>,
    pub current_snapshot_revision: u64,
    pub confirmation: Option<BroadcastConfirmation>,
    pub pending: bool,
    pub per_recipient: Vec<RecipientOutcomeRow>,
    pub aggregate_outcome: BroadcastAggregateOutcome,
    pub correlation_id: Option<String>,
    pub audit_link: Option<BroadcastAuditLink>,
    pub mapping_version: &'static str,
}

#[allow(dead_code)]
impl BroadcastComposerState {
    pub(crate) fn open(
        recipients: Vec<RosterEntry>,
        draft: DirectMessageDraft,
        daemon_offline: bool,
    ) -> Self {
        let validation = compute_broadcast_validation(&draft, daemon_offline);
        let mut included = Vec::new();
        let mut excluded = Vec::new();
        for recipient in recipients {
            match classify_broadcast_recipient(&recipient, &draft.subject) {
                Ok(()) => included.push(recipient),
                Err(reason) => excluded.push(BroadcastExclusion { recipient, reason }),
            }
        }
        let current_snapshot_revision = compute_broadcast_snapshot_revision(&included, &draft);
        Self {
            mode: DirectComposerMode::Broadcast,
            draft,
            validation,
            included,
            excluded,
            current_snapshot_revision,
            confirmation: None,
            pending: false,
            per_recipient: Vec::new(),
            aggregate_outcome: BroadcastAggregateOutcome::Pending,
            correlation_id: None,
            audit_link: None,
            mapping_version: "zornmesh.ui.broadcast_composer.v1",
        }
    }

    pub(crate) fn preview(&self) -> BroadcastConfirmation {
        BroadcastConfirmation {
            snapshot_revision: self.current_snapshot_revision,
            included_recipient_ids: self
                .included
                .iter()
                .map(|entry| entry.agent_id.clone())
                .collect(),
            excluded_recipient_ids: self
                .excluded
                .iter()
                .map(|exclusion| exclusion.recipient.agent_id.clone())
                .collect(),
        }
    }

    pub(crate) fn confirm(mut self, confirmation: BroadcastConfirmation) -> Self {
        self.confirmation = Some(confirmation);
        self
    }

    pub(crate) fn submit(
        mut self,
        correlation_id: impl Into<String>,
    ) -> Result<Self, (Self, BroadcastSubmitError)> {
        if self.pending {
            return Err((self, BroadcastSubmitError::DuplicateInFlight));
        }
        if !self.validation.is_ok() {
            let validation = self.validation;
            return Err((
                self,
                BroadcastSubmitError::ValidationFailed(validation),
            ));
        }
        let confirmation = match self.confirmation.clone() {
            Some(value) => value,
            None => return Err((self, BroadcastSubmitError::NoConfirmation)),
        };
        if confirmation.snapshot_revision != self.current_snapshot_revision {
            self.confirmation = None;
            return Err((self, BroadcastSubmitError::SnapshotDrift));
        }
        self.pending = true;
        self.correlation_id = Some(correlation_id.into());
        self.per_recipient = self
            .included
            .iter()
            .map(|entry| RecipientOutcomeRow {
                agent_id: entry.agent_id.clone(),
                display_name: entry.display_name.clone(),
                outcome: DirectSendOutcome::Queued,
                failure_reason: None,
                recorded_unix_ms: None,
                retry_handoff_available: false,
                inspect_handoff_available: false,
            })
            .collect();
        self.aggregate_outcome = if self.per_recipient.is_empty() {
            BroadcastAggregateOutcome::AllFailed
        } else {
            BroadcastAggregateOutcome::Pending
        };
        Ok(self)
    }

    pub(crate) fn record_recipient_outcome(
        mut self,
        recipient_id: &str,
        outcome: DirectSendOutcome,
        failure_reason: Option<&'static str>,
        recorded_unix_ms: u64,
    ) -> Self {
        for row in &mut self.per_recipient {
            if row.agent_id == recipient_id {
                row.outcome = outcome;
                row.failure_reason = failure_reason;
                row.recorded_unix_ms = Some(recorded_unix_ms);
                row.retry_handoff_available = matches!(
                    outcome,
                    DirectSendOutcome::TimedOut | DirectSendOutcome::DeadLettered
                );
                row.inspect_handoff_available = matches!(
                    outcome,
                    DirectSendOutcome::Rejected
                        | DirectSendOutcome::TimedOut
                        | DirectSendOutcome::DeadLettered
                );
            }
        }
        self.aggregate_outcome = aggregate_broadcast_outcome(&self.per_recipient);
        if matches!(
            self.aggregate_outcome,
            BroadcastAggregateOutcome::Success
                | BroadcastAggregateOutcome::PartialSuccess
                | BroadcastAggregateOutcome::AllFailed
        ) {
            self.pending = false;
        }
        self
    }

    pub(crate) fn record_drift_after_preview(mut self, new_recipients: Vec<RosterEntry>) -> Self {
        let mut included = Vec::new();
        let mut excluded = Vec::new();
        for recipient in new_recipients {
            match classify_broadcast_recipient(&recipient, &self.draft.subject) {
                Ok(()) => included.push(recipient),
                Err(reason) => excluded.push(BroadcastExclusion { recipient, reason }),
            }
        }
        self.included = included;
        self.excluded = excluded;
        self.current_snapshot_revision =
            compute_broadcast_snapshot_revision(&self.included, &self.draft);
        if let Some(confirmation) = &self.confirmation
            && confirmation.snapshot_revision != self.current_snapshot_revision
        {
            self.confirmation = None;
        }
        self
    }

    pub(crate) fn finalize_audit(
        mut self,
        actor_session: impl Into<String>,
        trace_id: impl Into<String>,
        requested_recipient_ids: Vec<String>,
    ) -> Self {
        let confirmation = self
            .confirmation
            .clone()
            .unwrap_or_else(|| BroadcastConfirmation {
                snapshot_revision: 0,
                included_recipient_ids: Vec::new(),
                excluded_recipient_ids: Vec::new(),
            });
        let drift_reconfirmation_required =
            confirmation.snapshot_revision != self.current_snapshot_revision;
        let per_recipient_outcomes: Vec<(String, &'static str)> = self
            .per_recipient
            .iter()
            .map(|row| (row.agent_id.clone(), row.outcome.as_str()))
            .collect();
        self.audit_link = Some(BroadcastAuditLink {
            actor_session: actor_session.into(),
            correlation_id: self.correlation_id.clone().unwrap_or_default(),
            trace_id: trace_id.into(),
            requested_recipient_ids,
            previewed_snapshot_revision: confirmation.snapshot_revision,
            accepted_snapshot_revision: self.current_snapshot_revision,
            actual_recipient_ids: self
                .included
                .iter()
                .map(|entry| entry.agent_id.clone())
                .collect(),
            excluded_recipient_ids: self
                .excluded
                .iter()
                .map(|exclusion| exclusion.recipient.agent_id.clone())
                .collect(),
            drift_reconfirmation_required,
            safe_payload_summary: serde_json::json!({
                "subject": self.draft.subject,
                "body_len": self.draft.body.len(),
            }),
            per_recipient_outcomes,
        });
        self
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema_version": self.mapping_version,
            "mode": self.mode.as_str(),
            "validation": self.validation.as_str(),
            "draft": {
                "subject": self.draft.subject,
                "body_length": self.draft.body.len(),
            },
            "current_snapshot_revision": self.current_snapshot_revision,
            "confirmation": self.confirmation.as_ref().map(|c| serde_json::json!({
                "snapshot_revision": c.snapshot_revision,
                "included_recipient_ids": c.included_recipient_ids,
                "excluded_recipient_ids": c.excluded_recipient_ids,
            })),
            "included": self.included.iter().map(|entry| serde_json::json!({
                "agent_id": entry.agent_id,
                "display_name": entry.display_name,
                "status": entry.status.as_str(),
            })).collect::<Vec<_>>(),
            "excluded": self.excluded.iter().map(|exclusion| serde_json::json!({
                "agent_id": exclusion.recipient.agent_id,
                "display_name": exclusion.recipient.display_name,
                "reason": exclusion.reason.as_str(),
            })).collect::<Vec<_>>(),
            "pending": self.pending,
            "aggregate_outcome": self.aggregate_outcome.as_str(),
            "per_recipient": self.per_recipient.iter().map(|row| serde_json::json!({
                "agent_id": row.agent_id,
                "display_name": row.display_name,
                "outcome": row.outcome.as_str(),
                "failure_reason": row.failure_reason,
                "recorded_unix_ms": row.recorded_unix_ms,
                "retry_handoff_available": row.retry_handoff_available,
                "inspect_handoff_available": row.inspect_handoff_available,
            })).collect::<Vec<_>>(),
            "correlation_id": self.correlation_id,
            "audit_link": self.audit_link.as_ref().map(|link| serde_json::json!({
                "actor_session": link.actor_session,
                "correlation_id": link.correlation_id,
                "trace_id": link.trace_id,
                "requested_recipient_ids": link.requested_recipient_ids,
                "previewed_snapshot_revision": link.previewed_snapshot_revision,
                "accepted_snapshot_revision": link.accepted_snapshot_revision,
                "actual_recipient_ids": link.actual_recipient_ids,
                "excluded_recipient_ids": link.excluded_recipient_ids,
                "drift_reconfirmation_required": link.drift_reconfirmation_required,
                "safe_payload_summary": link.safe_payload_summary,
                "per_recipient_outcomes": link.per_recipient_outcomes.iter().map(|(id, outcome)| {
                    serde_json::json!({"agent_id": id, "outcome": outcome})
                }).collect::<Vec<_>>(),
            })),
        })
    }
}

#[allow(dead_code)]
fn compute_broadcast_validation(
    draft: &DirectMessageDraft,
    daemon_offline: bool,
) -> DirectComposerValidation {
    if daemon_offline {
        return DirectComposerValidation::DaemonOffline;
    }
    if draft.body.trim().is_empty() {
        return DirectComposerValidation::EmptyBody;
    }
    if draft.body.len() > DIRECT_BODY_BUDGET {
        return DirectComposerValidation::BodyTooLarge;
    }
    if draft.subject.trim().is_empty() {
        return DirectComposerValidation::EmptySubject;
    }
    if draft.subject == "zorn" || draft.subject.starts_with("zorn.") {
        return DirectComposerValidation::ReservedSubject;
    }
    DirectComposerValidation::Ok
}

#[allow(dead_code)]
fn classify_broadcast_recipient(
    recipient: &RosterEntry,
    subject: &str,
) -> Result<(), BroadcastExclusionReason> {
    if recipient
        .warnings
        .iter()
        .any(|warning| *warning == "denied_by_allowlist")
    {
        return Err(BroadcastExclusionReason::DeniedByAllowlist);
    }
    if recipient
        .warnings
        .iter()
        .any(|warning| *warning == "unsafe_scope")
    {
        return Err(BroadcastExclusionReason::UnsafeScope);
    }
    match recipient.status {
        RosterStatus::Stale => return Err(BroadcastExclusionReason::Stale),
        RosterStatus::Disconnected | RosterStatus::Errored => {
            return Err(BroadcastExclusionReason::Disconnected);
        }
        _ => {}
    }
    if !recipient
        .capability_summary
        .iter()
        .any(|cap| cap == subject)
    {
        return Err(BroadcastExclusionReason::Incompatible);
    }
    Ok(())
}

#[allow(dead_code)]
fn compute_broadcast_snapshot_revision(included: &[RosterEntry], draft: &DirectMessageDraft) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in draft.subject.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for byte in (draft.body.len() as u64).to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let mut sorted_ids: Vec<&str> = included.iter().map(|entry| entry.agent_id.as_str()).collect();
    sorted_ids.sort();
    for id in sorted_ids {
        hash ^= 0x9e_3779_b97f_4a7c_15;
        hash = hash.wrapping_mul(0x100000001b3);
        for byte in id.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[allow(dead_code)]
fn aggregate_broadcast_outcome(rows: &[RecipientOutcomeRow]) -> BroadcastAggregateOutcome {
    if rows.is_empty() {
        return BroadcastAggregateOutcome::AllFailed;
    }
    let mut any_success = false;
    let mut any_failure = false;
    let mut any_pending = false;
    for row in rows {
        match row.outcome {
            DirectSendOutcome::Acknowledged | DirectSendOutcome::Delivered => any_success = true,
            DirectSendOutcome::Rejected
            | DirectSendOutcome::TimedOut
            | DirectSendOutcome::DeadLettered => any_failure = true,
            DirectSendOutcome::Queued | DirectSendOutcome::Stale => any_pending = true,
        }
    }
    if any_pending {
        return BroadcastAggregateOutcome::Pending;
    }
    match (any_success, any_failure) {
        (true, false) => BroadcastAggregateOutcome::Success,
        (true, true) => BroadcastAggregateOutcome::PartialSuccess,
        (false, true) => BroadcastAggregateOutcome::AllFailed,
        (false, false) => BroadcastAggregateOutcome::Pending,
    }
}

#[cfg(test)]
mod broadcast_composer_tests {
    use super::*;

    fn entry(id: &str, status: RosterStatus, capability: &str) -> RosterEntry {
        RosterEntry {
            display_name: id.to_owned(),
            agent_id: id.to_owned(),
            status,
            transport: RosterTransport::NativeSdk,
            capability_summary: vec![capability.to_owned()],
            last_seen_unix_ms: 1_700_000_000_000,
            recent_activity_count: 0,
            warnings: Vec::new(),
            high_privilege_required: false,
        }
    }

    fn draft(subject: &str, body: &str) -> DirectMessageDraft {
        DirectMessageDraft {
            subject: subject.to_owned(),
            body: body.to_owned(),
        }
    }

    #[test]
    fn open_partitions_recipients_into_included_and_excluded() {
        let active = entry("agent.local/a", RosterStatus::Active, "mesh.broadcast.go");
        let stale = entry("agent.local/b", RosterStatus::Stale, "mesh.broadcast.go");
        let mismatched = entry("agent.local/c", RosterStatus::Active, "mesh.other");
        let mut denied = entry("agent.local/d", RosterStatus::Active, "mesh.broadcast.go");
        denied.warnings.push("denied_by_allowlist");
        let state = BroadcastComposerState::open(
            vec![active.clone(), stale, mismatched, denied],
            draft("mesh.broadcast.go", "hi everyone"),
            false,
        );
        assert_eq!(state.mode, DirectComposerMode::Broadcast);
        assert_eq!(state.included.len(), 1);
        assert_eq!(state.included[0].agent_id, "agent.local/a");
        let exclusion_reasons: Vec<&'static str> = state
            .excluded
            .iter()
            .map(|excl| excl.reason.as_str())
            .collect();
        assert!(exclusion_reasons.contains(&"stale_recipient"));
        assert!(exclusion_reasons.contains(&"incompatible_capability"));
        assert!(exclusion_reasons.contains(&"denied_by_allowlist"));
    }

    #[test]
    fn submit_without_confirmation_is_rejected() {
        let state = BroadcastComposerState::open(
            vec![entry("agent.local/a", RosterStatus::Active, "mesh.broadcast.go")],
            draft("mesh.broadcast.go", "hi"),
            false,
        );
        match state.submit("corr-broadcast") {
            Err((_state, BroadcastSubmitError::NoConfirmation)) => {}
            other => panic!("expected NoConfirmation, got {other:?}"),
        }
    }

    #[test]
    fn confirmed_broadcast_with_unchanged_snapshot_marks_pending_and_seeds_recipient_rows() {
        let state = BroadcastComposerState::open(
            vec![
                entry("agent.local/a", RosterStatus::Active, "mesh.broadcast.go"),
                entry("agent.local/b", RosterStatus::Active, "mesh.broadcast.go"),
            ],
            draft("mesh.broadcast.go", "hi"),
            false,
        );
        let snapshot = state.preview();
        assert_eq!(snapshot.included_recipient_ids.len(), 2);
        let confirmed = state.confirm(snapshot);
        let pending = confirmed.submit("corr-broadcast").expect("submit accepted");
        assert!(pending.pending);
        assert_eq!(pending.per_recipient.len(), 2);
        assert!(pending
            .per_recipient
            .iter()
            .all(|row| row.outcome == DirectSendOutcome::Queued));
        assert_eq!(pending.aggregate_outcome, BroadcastAggregateOutcome::Pending);
    }

    #[test]
    fn drift_after_preview_invalidates_confirmation_and_blocks_submit() {
        let state = BroadcastComposerState::open(
            vec![
                entry("agent.local/a", RosterStatus::Active, "mesh.broadcast.go"),
                entry("agent.local/b", RosterStatus::Active, "mesh.broadcast.go"),
            ],
            draft("mesh.broadcast.go", "hi"),
            false,
        );
        let snapshot = state.preview();
        let confirmed = state.confirm(snapshot.clone());
        // Recipient set drifts: b becomes stale, c joins.
        let drifted = confirmed.record_drift_after_preview(vec![
            entry("agent.local/a", RosterStatus::Active, "mesh.broadcast.go"),
            entry("agent.local/b", RosterStatus::Stale, "mesh.broadcast.go"),
            entry("agent.local/c", RosterStatus::Active, "mesh.broadcast.go"),
        ]);
        assert!(
            drifted.confirmation.is_none(),
            "drift invalidates the prior confirmation"
        );
        match drifted.submit("corr-broadcast") {
            Err((_state, BroadcastSubmitError::NoConfirmation)) => {}
            other => panic!("drifted submit must require reconfirmation, got {other:?}"),
        }
    }

    #[test]
    fn stale_snapshot_reconfirmation_is_explicitly_required() {
        let state = BroadcastComposerState::open(
            vec![
                entry("agent.local/a", RosterStatus::Active, "mesh.broadcast.go"),
                entry("agent.local/b", RosterStatus::Active, "mesh.broadcast.go"),
            ],
            draft("mesh.broadcast.go", "hi"),
            false,
        );
        let stale_snapshot = BroadcastConfirmation {
            snapshot_revision: state.current_snapshot_revision.wrapping_add(1),
            included_recipient_ids: vec!["agent.local/a".to_owned()],
            excluded_recipient_ids: Vec::new(),
        };
        let confirmed = state.confirm(stale_snapshot);
        match confirmed.submit("corr-broadcast") {
            Err((_state, BroadcastSubmitError::SnapshotDrift)) => {}
            other => panic!("expected snapshot drift, got {other:?}"),
        }
    }

    #[test]
    fn per_recipient_outcomes_drive_aggregate_state_through_partial_failure() {
        let state = BroadcastComposerState::open(
            vec![
                entry("agent.local/a", RosterStatus::Active, "mesh.broadcast.go"),
                entry("agent.local/b", RosterStatus::Active, "mesh.broadcast.go"),
                entry("agent.local/c", RosterStatus::Active, "mesh.broadcast.go"),
            ],
            draft("mesh.broadcast.go", "hi"),
            false,
        );
        let snapshot = state.preview();
        let after_submit = state.confirm(snapshot).submit("corr-broadcast").expect("submit");

        let one_done = after_submit.record_recipient_outcome(
            "agent.local/a",
            DirectSendOutcome::Acknowledged,
            None,
            1_700_000_001_000,
        );
        assert_eq!(one_done.aggregate_outcome, BroadcastAggregateOutcome::Pending);
        assert!(one_done.pending);

        let two_done = one_done.record_recipient_outcome(
            "agent.local/b",
            DirectSendOutcome::Rejected,
            Some("policy_denied"),
            1_700_000_001_100,
        );
        assert_eq!(two_done.aggregate_outcome, BroadcastAggregateOutcome::Pending);

        let final_state = two_done.record_recipient_outcome(
            "agent.local/c",
            DirectSendOutcome::DeadLettered,
            Some("retry_exhausted"),
            1_700_000_001_200,
        );
        assert_eq!(
            final_state.aggregate_outcome,
            BroadcastAggregateOutcome::PartialSuccess
        );
        assert!(!final_state.pending);
        let row_c = final_state
            .per_recipient
            .iter()
            .find(|row| row.agent_id == "agent.local/c")
            .unwrap();
        assert!(row_c.retry_handoff_available);
        assert!(row_c.inspect_handoff_available);
    }

    #[test]
    fn all_failed_aggregate_when_every_recipient_terminates_with_failure() {
        let state = BroadcastComposerState::open(
            vec![
                entry("agent.local/a", RosterStatus::Active, "mesh.broadcast.go"),
                entry("agent.local/b", RosterStatus::Active, "mesh.broadcast.go"),
            ],
            draft("mesh.broadcast.go", "hi"),
            false,
        );
        let after = state
            .clone()
            .confirm(state.preview())
            .submit("corr-broadcast")
            .expect("submit")
            .record_recipient_outcome(
                "agent.local/a",
                DirectSendOutcome::Rejected,
                Some("policy_denied"),
                1_700_000_001_000,
            )
            .record_recipient_outcome(
                "agent.local/b",
                DirectSendOutcome::TimedOut,
                Some("agent_timeout"),
                1_700_000_001_100,
            );
        assert_eq!(after.aggregate_outcome, BroadcastAggregateOutcome::AllFailed);
        assert!(!after.pending);
    }

    #[test]
    fn duplicate_submit_is_rejected_while_pending() {
        let state = BroadcastComposerState::open(
            vec![entry("agent.local/a", RosterStatus::Active, "mesh.broadcast.go")],
            draft("mesh.broadcast.go", "hi"),
            false,
        );
        let snapshot = state.preview();
        let pending = state.confirm(snapshot).submit("corr-1").expect("first submit");
        match pending.submit("corr-2") {
            Err((_state, BroadcastSubmitError::DuplicateInFlight)) => {}
            other => panic!("expected duplicate-in-flight, got {other:?}"),
        }
    }

    #[test]
    fn audit_link_records_snapshot_revisions_drift_status_and_per_recipient_outcomes() {
        let state = BroadcastComposerState::open(
            vec![
                entry("agent.local/a", RosterStatus::Active, "mesh.broadcast.go"),
                entry("agent.local/b", RosterStatus::Active, "mesh.broadcast.go"),
            ],
            draft("mesh.broadcast.go", "hi"),
            false,
        );
        let snapshot = state.preview();
        let confirmed = state.confirm(snapshot);
        let revision_before = confirmed.current_snapshot_revision;
        let final_state = confirmed
            .submit("corr-broadcast")
            .expect("submit")
            .record_recipient_outcome(
                "agent.local/a",
                DirectSendOutcome::Acknowledged,
                None,
                1_700_000_001_000,
            )
            .record_recipient_outcome(
                "agent.local/b",
                DirectSendOutcome::Acknowledged,
                None,
                1_700_000_001_100,
            )
            .finalize_audit(
                "session-1",
                "trace-1",
                vec!["agent.local/a".to_owned(), "agent.local/b".to_owned()],
            );
        let json = final_state.to_json();
        let audit = &json["audit_link"];
        assert_eq!(audit["previewed_snapshot_revision"], revision_before);
        assert_eq!(audit["accepted_snapshot_revision"], revision_before);
        assert_eq!(audit["drift_reconfirmation_required"], false);
        let outcomes = audit["per_recipient_outcomes"].as_array().unwrap();
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0]["outcome"], "acknowledged");
        assert_eq!(audit["safe_payload_summary"]["subject"], "mesh.broadcast.go");
        assert_eq!(audit["safe_payload_summary"]["body_len"], 2);
    }

    #[test]
    fn schema_version_pins_broadcast_composer_contract() {
        let state = BroadcastComposerState::open(
            Vec::new(),
            draft("mesh.broadcast.go", "hi"),
            false,
        );
        assert_eq!(state.mapping_version, "zornmesh.ui.broadcast_composer.v1");
        assert_eq!(
            state.to_json()["schema_version"],
            "zornmesh.ui.broadcast_composer.v1"
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum DirectComposerMode {
    Direct,
    Broadcast,
}

impl DirectComposerMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Broadcast => "broadcast",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum DirectComposerValidation {
    Ok,
    EmptyBody,
    EmptySubject,
    ReservedSubject,
    BodyTooLarge,
    RecipientStale,
    RecipientDisconnected,
    RecipientMissingCapability,
    DeniedByAllowlist,
    DaemonOffline,
}

impl DirectComposerValidation {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::EmptyBody => "empty_body",
            Self::EmptySubject => "empty_subject",
            Self::ReservedSubject => "reserved_subject",
            Self::BodyTooLarge => "body_too_large",
            Self::RecipientStale => "recipient_stale",
            Self::RecipientDisconnected => "recipient_disconnected",
            Self::RecipientMissingCapability => "recipient_missing_capability",
            Self::DeniedByAllowlist => "denied_by_allowlist",
            Self::DaemonOffline => "daemon_offline",
        }
    }

    pub(crate) const fn is_ok(self) -> bool {
        matches!(self, Self::Ok)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum DirectSendOutcome {
    Queued,
    Delivered,
    Acknowledged,
    Rejected,
    TimedOut,
    DeadLettered,
    Stale,
}

impl DirectSendOutcome {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Delivered => "delivered",
            Self::Acknowledged => "acknowledged",
            Self::Rejected => "rejected",
            Self::TimedOut => "timed_out",
            Self::DeadLettered => "dead_lettered",
            Self::Stale => "stale",
        }
    }

    pub(crate) const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Acknowledged | Self::Delivered | Self::Rejected | Self::TimedOut | Self::DeadLettered
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum DirectComposerSubmitError {
    DuplicateInFlight,
    ValidationFailed(DirectComposerValidation),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct DirectMessageDraft {
    pub subject: String,
    pub body: String,
}

pub(crate) const DIRECT_BODY_BUDGET: usize = 64 * 1024;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct DirectComposerState {
    pub mode: DirectComposerMode,
    pub target: RosterEntry,
    pub draft: DirectMessageDraft,
    pub validation: DirectComposerValidation,
    pub pending: bool,
    pub outcome: Option<DirectSendOutcome>,
    pub correlation_id: Option<String>,
    pub audit_link: Option<DirectAuditLink>,
    pub mapping_version: &'static str,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct DirectAuditLink {
    pub actor_session: String,
    pub correlation_id: String,
    pub trace_id: String,
    pub recipient_agent_id: String,
    pub safe_payload_summary: serde_json::Value,
    pub validation_outcome: &'static str,
    pub delivery_outcome: &'static str,
}

#[allow(dead_code)]
impl DirectComposerState {
    pub(crate) fn open(target: RosterEntry, draft: DirectMessageDraft, daemon_offline: bool) -> Self {
        let mut state = Self {
            mode: DirectComposerMode::Direct,
            target,
            draft,
            validation: DirectComposerValidation::Ok,
            pending: false,
            outcome: None,
            correlation_id: None,
            audit_link: None,
            mapping_version: "zornmesh.ui.direct_composer.v1",
        };
        state.validation = state.compute_validation(daemon_offline);
        state
    }

    fn compute_validation(&self, daemon_offline: bool) -> DirectComposerValidation {
        if daemon_offline {
            return DirectComposerValidation::DaemonOffline;
        }
        if self.draft.body.trim().is_empty() {
            return DirectComposerValidation::EmptyBody;
        }
        if self.draft.body.len() > DIRECT_BODY_BUDGET {
            return DirectComposerValidation::BodyTooLarge;
        }
        if self.draft.subject.trim().is_empty() {
            return DirectComposerValidation::EmptySubject;
        }
        if self.draft.subject == "zorn" || self.draft.subject.starts_with("zorn.") {
            return DirectComposerValidation::ReservedSubject;
        }
        match self.target.status {
            RosterStatus::Stale => return DirectComposerValidation::RecipientStale,
            RosterStatus::Disconnected | RosterStatus::Errored => {
                return DirectComposerValidation::RecipientDisconnected;
            }
            _ => {}
        }
        if self
            .target
            .warnings
            .iter()
            .any(|warning| *warning == "denied_by_allowlist")
        {
            return DirectComposerValidation::DeniedByAllowlist;
        }
        if !self
            .target
            .capability_summary
            .iter()
            .any(|cap| cap == &self.draft.subject)
        {
            return DirectComposerValidation::RecipientMissingCapability;
        }
        DirectComposerValidation::Ok
    }

    pub(crate) fn submit(
        mut self,
        correlation_id: impl Into<String>,
    ) -> Result<Self, (Self, DirectComposerSubmitError)> {
        if self.pending {
            return Err((self, DirectComposerSubmitError::DuplicateInFlight));
        }
        if !self.validation.is_ok() {
            let validation = self.validation;
            return Err((self, DirectComposerSubmitError::ValidationFailed(validation)));
        }
        self.pending = true;
        self.correlation_id = Some(correlation_id.into());
        self.outcome = None;
        Ok(self)
    }

    pub(crate) fn record_outcome(
        mut self,
        outcome: DirectSendOutcome,
        actor_session: impl Into<String>,
        trace_id: impl Into<String>,
    ) -> Self {
        if outcome.is_terminal() {
            self.pending = false;
        }
        let correlation_id = self
            .correlation_id
            .clone()
            .unwrap_or_else(|| "corr-pending".to_owned());
        self.audit_link = Some(DirectAuditLink {
            actor_session: actor_session.into(),
            correlation_id: correlation_id.clone(),
            trace_id: trace_id.into(),
            recipient_agent_id: self.target.agent_id.clone(),
            safe_payload_summary: serde_json::json!({
                "subject": self.draft.subject,
                "body_len": self.draft.body.len(),
            }),
            validation_outcome: self.validation.as_str(),
            delivery_outcome: outcome.as_str(),
        });
        self.outcome = Some(outcome);
        self
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema_version": self.mapping_version,
            "mode": self.mode.as_str(),
            "target": {
                "agent_id": self.target.agent_id,
                "display_name": self.target.display_name,
                "status": self.target.status.as_str(),
                "transport": self.target.transport.as_str(),
                "capability_summary": self.target.capability_summary,
                "high_privilege_required": self.target.high_privilege_required,
            },
            "draft": {
                "subject": self.draft.subject,
                "body_length": self.draft.body.len(),
            },
            "validation": self.validation.as_str(),
            "pending": self.pending,
            "outcome": self.outcome.map(|o| o.as_str()),
            "correlation_id": self.correlation_id,
            "audit_link": self.audit_link.as_ref().map(|link| serde_json::json!({
                "actor_session": link.actor_session,
                "correlation_id": link.correlation_id,
                "trace_id": link.trace_id,
                "recipient_agent_id": link.recipient_agent_id,
                "safe_payload_summary": link.safe_payload_summary,
                "validation_outcome": link.validation_outcome,
                "delivery_outcome": link.delivery_outcome,
            })),
        })
    }
}

#[cfg(test)]
mod direct_composer_tests {
    use super::*;

    fn target(status: RosterStatus, capability: &str) -> RosterEntry {
        RosterEntry {
            display_name: "Recipient".to_owned(),
            agent_id: "agent.local/recipient".to_owned(),
            status,
            transport: RosterTransport::NativeSdk,
            capability_summary: vec![capability.to_owned()],
            last_seen_unix_ms: 1_700_000_000_000,
            recent_activity_count: 1,
            warnings: Vec::new(),
            high_privilege_required: false,
        }
    }

    fn draft(subject: &str, body: &str) -> DirectMessageDraft {
        DirectMessageDraft {
            subject: subject.to_owned(),
            body: body.to_owned(),
        }
    }

    #[test]
    fn valid_state_passes_validation_and_distinguishes_direct_mode() {
        let state = DirectComposerState::open(
            target(RosterStatus::Active, "mesh.direct.send"),
            draft("mesh.direct.send", "hello world"),
            false,
        );
        assert_eq!(state.mode, DirectComposerMode::Direct);
        assert_eq!(state.validation, DirectComposerValidation::Ok);
        let json = state.to_json();
        assert_eq!(json["mode"], "direct");
        assert_eq!(json["validation"], "ok");
    }

    #[test]
    fn empty_body_blocks_send_with_explanatory_validation() {
        let state = DirectComposerState::open(
            target(RosterStatus::Active, "mesh.direct.send"),
            draft("mesh.direct.send", "   "),
            false,
        );
        assert_eq!(state.validation, DirectComposerValidation::EmptyBody);
        let result = state.submit("corr-1");
        match result {
            Err((_state, DirectComposerSubmitError::ValidationFailed(v))) => {
                assert_eq!(v, DirectComposerValidation::EmptyBody);
            }
            _ => panic!("expected validation failure"),
        }
    }

    #[test]
    fn invalid_subject_or_reserved_prefix_is_rejected() {
        let empty = DirectComposerState::open(
            target(RosterStatus::Active, ""),
            draft("", "hello"),
            false,
        );
        assert_eq!(empty.validation, DirectComposerValidation::EmptySubject);

        let reserved = DirectComposerState::open(
            target(RosterStatus::Active, "zorn.system"),
            draft("zorn.system", "hello"),
            false,
        );
        assert_eq!(reserved.validation, DirectComposerValidation::ReservedSubject);
    }

    #[test]
    fn body_above_budget_is_rejected_as_too_large() {
        let big = "x".repeat(DIRECT_BODY_BUDGET + 1);
        let state = DirectComposerState::open(
            target(RosterStatus::Active, "mesh.direct.send"),
            draft("mesh.direct.send", &big),
            false,
        );
        assert_eq!(state.validation, DirectComposerValidation::BodyTooLarge);
    }

    #[test]
    fn stale_or_disconnected_recipient_blocks_send() {
        let stale = DirectComposerState::open(
            target(RosterStatus::Stale, "mesh.direct.send"),
            draft("mesh.direct.send", "hello"),
            false,
        );
        assert_eq!(stale.validation, DirectComposerValidation::RecipientStale);

        let disconnected = DirectComposerState::open(
            target(RosterStatus::Disconnected, "mesh.direct.send"),
            draft("mesh.direct.send", "hello"),
            false,
        );
        assert_eq!(
            disconnected.validation,
            DirectComposerValidation::RecipientDisconnected
        );
    }

    #[test]
    fn missing_capability_or_allowlist_denial_is_explicit() {
        let missing_cap = DirectComposerState::open(
            target(RosterStatus::Active, "mesh.other"),
            draft("mesh.direct.send", "hello"),
            false,
        );
        assert_eq!(
            missing_cap.validation,
            DirectComposerValidation::RecipientMissingCapability
        );

        let mut denied_target = target(RosterStatus::Active, "mesh.direct.send");
        denied_target.warnings.push("denied_by_allowlist");
        let denied = DirectComposerState::open(
            denied_target,
            draft("mesh.direct.send", "hello"),
            false,
        );
        assert_eq!(denied.validation, DirectComposerValidation::DeniedByAllowlist);
    }

    #[test]
    fn daemon_offline_blocks_send_regardless_of_other_validation() {
        let state = DirectComposerState::open(
            target(RosterStatus::Active, "mesh.direct.send"),
            draft("mesh.direct.send", "hello"),
            true,
        );
        assert_eq!(state.validation, DirectComposerValidation::DaemonOffline);
    }

    #[test]
    fn submit_marks_pending_and_blocks_duplicate_in_flight_clicks() {
        let state = DirectComposerState::open(
            target(RosterStatus::Active, "mesh.direct.send"),
            draft("mesh.direct.send", "hello"),
            false,
        );
        let pending = state.submit("corr-direct").expect("first submit");
        assert!(pending.pending);
        assert_eq!(pending.correlation_id.as_deref(), Some("corr-direct"));
        match pending.submit("corr-other") {
            Err((_state, DirectComposerSubmitError::DuplicateInFlight)) => {}
            _ => panic!("duplicate submit must be rejected"),
        }
    }

    #[test]
    fn terminal_outcomes_clear_pending_and_persist_audit_link() {
        let state = DirectComposerState::open(
            target(RosterStatus::Active, "mesh.direct.send"),
            draft("mesh.direct.send", "hello"),
            false,
        )
        .submit("corr-direct")
        .expect("submit accepted");

        let queued = state
            .clone()
            .record_outcome(DirectSendOutcome::Queued, "session-1", "trace-1");
        assert!(queued.pending, "queued is non-terminal so pending stays");
        assert_eq!(queued.outcome, Some(DirectSendOutcome::Queued));

        let acknowledged = queued.record_outcome(
            DirectSendOutcome::Acknowledged,
            "session-1",
            "trace-1",
        );
        assert!(!acknowledged.pending, "terminal state clears pending");
        let json = acknowledged.to_json();
        assert_eq!(json["outcome"], "acknowledged");
        assert_eq!(json["audit_link"]["delivery_outcome"], "acknowledged");
        assert_eq!(json["audit_link"]["validation_outcome"], "ok");
        assert_eq!(json["audit_link"]["recipient_agent_id"], "agent.local/recipient");
        assert_eq!(json["audit_link"]["safe_payload_summary"]["subject"], "mesh.direct.send");
        assert_eq!(json["audit_link"]["safe_payload_summary"]["body_len"], 5);
    }

    #[test]
    fn dead_lettered_and_timed_out_outcomes_clear_pending() {
        let dead = DirectComposerState::open(
            target(RosterStatus::Active, "mesh.direct.send"),
            draft("mesh.direct.send", "hello"),
            false,
        )
        .submit("corr-1")
        .expect("submit accepted")
        .record_outcome(DirectSendOutcome::DeadLettered, "session-1", "trace-1");
        assert!(!dead.pending);
        assert_eq!(dead.outcome, Some(DirectSendOutcome::DeadLettered));

        let timed = DirectComposerState::open(
            target(RosterStatus::Active, "mesh.direct.send"),
            draft("mesh.direct.send", "hello"),
            false,
        )
        .submit("corr-2")
        .expect("submit accepted")
        .record_outcome(DirectSendOutcome::TimedOut, "session-1", "trace-1");
        assert!(!timed.pending);
        assert_eq!(timed.outcome, Some(DirectSendOutcome::TimedOut));
    }

    #[test]
    fn audit_summary_does_not_include_raw_body() {
        let state = DirectComposerState::open(
            target(RosterStatus::Active, "mesh.direct.send"),
            draft("mesh.direct.send", "secret-body-must-not-leak"),
            false,
        )
        .submit("corr-leak")
        .expect("submit accepted")
        .record_outcome(DirectSendOutcome::Acknowledged, "session-1", "trace-1");
        let serialized = serde_json::to_string(&state.to_json()).expect("serialize");
        assert!(
            !serialized.contains("secret-body-must-not-leak"),
            "raw body must not appear in audit JSON"
        );
    }

    #[test]
    fn schema_version_pins_direct_composer_contract() {
        let state = DirectComposerState::open(
            target(RosterStatus::Active, "mesh.direct.send"),
            draft("mesh.direct.send", "hello"),
            false,
        );
        assert_eq!(state.mapping_version, "zornmesh.ui.direct_composer.v1");
        assert_eq!(
            state.to_json()["schema_version"],
            "zornmesh.ui.direct_composer.v1"
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum CliHandoffOperation {
    Trace,
    Inspect,
    Replay,
    Agents,
    Doctor,
    Audit,
}

impl CliHandoffOperation {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Inspect => "inspect",
            Self::Replay => "replay",
            Self::Agents => "agents",
            Self::Doctor => "doctor",
            Self::Audit => "audit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum CliHandoffAvailability {
    Available,
    RequiresDaemon,
    RequiresOfflineAudit,
    InsufficientContext,
    Unsafe,
}

impl CliHandoffAvailability {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::RequiresDaemon => "requires_daemon",
            Self::RequiresOfflineAudit => "requires_offline_audit",
            Self::InsufficientContext => "insufficient_context",
            Self::Unsafe => "unsafe",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct CliHandoff {
    pub operation: CliHandoffOperation,
    pub argv: Vec<String>,
    pub description: &'static str,
    pub expected_outcome: &'static str,
    pub availability: CliHandoffAvailability,
    pub unavailable_reason: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum CliHandoffError {
    NewlineInValue,
    NulByteInValue,
    EmptyValue,
}

#[allow(dead_code)]
impl CliHandoff {
    pub(crate) fn build(
        operation: CliHandoffOperation,
        positional: &[&str],
        flags: &[(&str, &str)],
        description: &'static str,
        expected_outcome: &'static str,
    ) -> Result<Self, CliHandoffError> {
        let mut argv: Vec<String> = vec!["zornmesh".to_owned(), operation.as_str().to_owned()];
        let mut needs_separator = false;
        for value in positional {
            validate_handoff_value(value)?;
            if !needs_separator && value.starts_with('-') {
                argv.push("--".to_owned());
                needs_separator = true;
            }
            argv.push((*value).to_owned());
        }
        for (flag, value) in flags {
            validate_handoff_flag(flag)?;
            validate_handoff_value(value)?;
            argv.push((*flag).to_owned());
            argv.push((*value).to_owned());
        }
        Ok(Self {
            operation,
            argv,
            description,
            expected_outcome,
            availability: CliHandoffAvailability::Available,
            unavailable_reason: None,
        })
    }

    pub(crate) fn unavailable(
        operation: CliHandoffOperation,
        availability: CliHandoffAvailability,
        reason: &'static str,
        description: &'static str,
        expected_outcome: &'static str,
    ) -> Self {
        Self {
            operation,
            argv: Vec::new(),
            description,
            expected_outcome,
            availability,
            unavailable_reason: Some(reason),
        }
    }

    pub(crate) fn shell_command(&self) -> String {
        if self.argv.is_empty() {
            return String::new();
        }
        self.argv
            .iter()
            .map(|token| posix_quote_token(token))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "operation": self.operation.as_str(),
            "argv": self.argv,
            "shell_command": self.shell_command(),
            "description": self.description,
            "expected_outcome": self.expected_outcome,
            "availability": self.availability.as_str(),
            "unavailable_reason": self.unavailable_reason,
            "copy_action": if matches!(self.availability, CliHandoffAvailability::Available) {
                "copy_to_clipboard"
            } else {
                "no_command_offered"
            },
        })
    }
}

#[allow(dead_code)]
fn validate_handoff_value(value: &str) -> Result<(), CliHandoffError> {
    if value.is_empty() {
        return Err(CliHandoffError::EmptyValue);
    }
    if value.contains('\n') || value.contains('\r') {
        return Err(CliHandoffError::NewlineInValue);
    }
    if value.contains('\0') {
        return Err(CliHandoffError::NulByteInValue);
    }
    Ok(())
}

#[allow(dead_code)]
fn validate_handoff_flag(flag: &str) -> Result<(), CliHandoffError> {
    if !flag.starts_with('-') {
        return Err(CliHandoffError::EmptyValue);
    }
    validate_handoff_value(flag)
}

#[allow(dead_code)]
fn posix_quote_token(token: &str) -> String {
    let safe = !token.is_empty()
        && token.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '.' | '_' | '-' | '/' | '=' | ':' | ',' | '@' | '+')
        });
    if safe {
        return token.to_owned();
    }
    let mut quoted = String::with_capacity(token.len() + 2);
    quoted.push('\'');
    for ch in token.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum FocusedTraceMode {
    Full,
    Windowed,
}

impl FocusedTraceMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Windowed => "windowed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum FocusedTraceRecoveryCue {
    InspectTrace,
    InspectDeadLetter,
    Replay,
    Reconnect,
    AuditVerify,
}

impl FocusedTraceRecoveryCue {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::InspectTrace => "inspect_trace",
            Self::InspectDeadLetter => "inspect_dead_letter",
            Self::Replay => "replay",
            Self::Reconnect => "reconnect",
            Self::AuditVerify => "audit_verify",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct FocusedTrace {
    pub correlation_id: String,
    pub mode: FocusedTraceMode,
    pub page: TimelinePage,
    pub recovery_cues: Vec<FocusedTraceRecoveryCue>,
    pub handoffs: Vec<CliHandoff>,
    pub paused: bool,
    pub mapping_version: &'static str,
}

#[allow(dead_code)]
impl FocusedTrace {
    pub(crate) const FULL_RENDER_BUDGET: usize = 500;

    pub(crate) fn open(
        correlation_id: impl Into<String>,
        page: TimelinePage,
        recovery_cues: Vec<FocusedTraceRecoveryCue>,
        handoffs: Vec<CliHandoff>,
    ) -> Self {
        let correlation_id = correlation_id.into();
        let mode = if page.partial_window || page.events.len() > Self::FULL_RENDER_BUDGET {
            FocusedTraceMode::Windowed
        } else {
            FocusedTraceMode::Full
        };
        Self {
            correlation_id,
            mode,
            page,
            recovery_cues,
            handoffs,
            paused: false,
            mapping_version: "zornmesh.ui.focused_trace.v1",
        }
    }

    pub(crate) fn pause(mut self) -> Self {
        self.paused = true;
        self
    }

    pub(crate) fn resume(mut self) -> Self {
        self.paused = false;
        self
    }

    pub(crate) fn ingest_live(self, events: Vec<TimelineEvent>) -> Self {
        if self.paused {
            return self;
        }
        let new_page = self.page.append_live(events);
        Self {
            correlation_id: self.correlation_id,
            mode: self.mode,
            page: new_page,
            recovery_cues: self.recovery_cues,
            handoffs: self.handoffs,
            paused: self.paused,
            mapping_version: self.mapping_version,
        }
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema_version": self.mapping_version,
            "correlation_id": self.correlation_id,
            "mode": self.mode.as_str(),
            "paused": self.paused,
            "page": self.page.to_json(),
            "recovery_cues": self.recovery_cues.iter().map(|cue| cue.as_str()).collect::<Vec<_>>(),
            "handoffs": self.handoffs.iter().map(CliHandoff::to_json).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod cli_handoff_tests {
    use super::*;

    #[test]
    fn argv_starts_with_zornmesh_and_operation() {
        let handoff = CliHandoff::build(
            CliHandoffOperation::Trace,
            &["corr-123"],
            &[("--evidence", "/tmp/evidence.log")],
            "inspect trace by correlation",
            "structured timeline JSON",
        )
        .expect("valid handoff");
        assert_eq!(handoff.argv[0], "zornmesh");
        assert_eq!(handoff.argv[1], "trace");
        assert_eq!(handoff.argv[2], "corr-123");
        assert_eq!(handoff.argv[3], "--evidence");
        assert_eq!(handoff.argv[4], "/tmp/evidence.log");
    }

    #[test]
    fn shell_command_quotes_metacharacters_and_preserves_semantics() {
        let handoff = CliHandoff::build(
            CliHandoffOperation::Inspect,
            &["dead_letters"],
            &[("--correlation-id", "corr; rm -rf /")],
            "inspect dead letters",
            "filtered DLQ records",
        )
        .expect("metacharacters are quoted, not concatenated");
        let cmd = handoff.shell_command();
        assert!(cmd.contains("'corr; rm -rf /'"), "metacharacters single-quoted: {cmd}");
        assert!(!cmd.contains("`"));
        assert!(!cmd.contains("$("));
    }

    #[test]
    fn shell_command_escapes_embedded_single_quote() {
        let handoff = CliHandoff::build(
            CliHandoffOperation::Replay,
            &["msg-it's-here"],
            &[],
            "replay message",
            "audit transition",
        )
        .expect("embedded quotes are escaped");
        let cmd = handoff.shell_command();
        assert!(
            cmd.contains("'msg-it'\\''s-here'"),
            "embedded quote uses '\\'' escape: {cmd}"
        );
    }

    #[test]
    fn newline_in_value_is_refused_at_construction() {
        let result = CliHandoff::build(
            CliHandoffOperation::Trace,
            &["corr\nrm -rf /"],
            &[],
            "ignored",
            "ignored",
        );
        assert!(matches!(result, Err(CliHandoffError::NewlineInValue)));
    }

    #[test]
    fn nul_byte_in_value_is_refused_at_construction() {
        let result = CliHandoff::build(
            CliHandoffOperation::Trace,
            &["corr\0extra"],
            &[],
            "ignored",
            "ignored",
        );
        assert!(matches!(result, Err(CliHandoffError::NulByteInValue)));
    }

    #[test]
    fn option_like_positional_value_gets_double_dash_separator() {
        let handoff = CliHandoff::build(
            CliHandoffOperation::Trace,
            &["--malicious-id"],
            &[],
            "preserve option-like ID as positional",
            "looked up trace",
        )
        .expect("option-like positional accepted");
        assert!(handoff.argv.contains(&"--".to_owned()));
        let dash_idx = handoff.argv.iter().position(|t| t == "--").unwrap();
        let mal_idx = handoff
            .argv
            .iter()
            .position(|t| t == "--malicious-id")
            .unwrap();
        assert!(dash_idx < mal_idx, "double dash precedes option-like value");
    }

    #[test]
    fn shell_metachars_dollar_backtick_and_glob_are_single_quoted() {
        let handoff = CliHandoff::build(
            CliHandoffOperation::Inspect,
            &["messages"],
            &[("--subject", "$(cat /etc/passwd)")],
            "test",
            "test",
        )
        .expect("dollar substitution is quoted");
        let cmd = handoff.shell_command();
        assert!(cmd.contains("'$(cat /etc/passwd)'"));
        let backtick = CliHandoff::build(
            CliHandoffOperation::Inspect,
            &["messages"],
            &[("--subject", "`whoami`")],
            "test",
            "test",
        )
        .expect("backtick substitution is quoted");
        assert!(backtick.shell_command().contains("'`whoami`'"));
        let glob = CliHandoff::build(
            CliHandoffOperation::Inspect,
            &["messages"],
            &[("--subject", "*.log")],
            "test",
            "test",
        )
        .expect("glob is quoted");
        assert!(glob.shell_command().contains("'*.log'"));
    }

    #[test]
    fn unavailable_handoff_carries_reason_and_offers_no_command() {
        let handoff = CliHandoff::unavailable(
            CliHandoffOperation::Replay,
            CliHandoffAvailability::RequiresDaemon,
            "daemon_offline",
            "replay requires running daemon",
            "would create new replay attempt",
        );
        assert!(handoff.argv.is_empty());
        assert_eq!(handoff.shell_command(), "");
        let json = handoff.to_json();
        assert_eq!(json["availability"], "requires_daemon");
        assert_eq!(json["unavailable_reason"], "daemon_offline");
        assert_eq!(json["copy_action"], "no_command_offered");
        assert_eq!(json["shell_command"], "");
    }

    #[test]
    fn safe_token_does_not_get_quoted_unnecessarily() {
        assert_eq!(posix_quote_token("zornmesh"), "zornmesh");
        assert_eq!(posix_quote_token("--evidence"), "--evidence");
        assert_eq!(posix_quote_token("/tmp/evidence.log"), "/tmp/evidence.log");
        assert_eq!(posix_quote_token("corr-123"), "corr-123");
    }
}

#[cfg(test)]
mod focused_trace_tests {
    use super::*;

    fn event(seq: u64, msg: &str) -> TimelineEvent {
        TimelineEvent {
            daemon_sequence: seq,
            message_id: msg.to_owned(),
            correlation_id: "corr-focus".to_owned(),
            trace_id: "trace-focus".to_owned(),
            source_agent: "agent.local/sender".to_owned(),
            target_or_subject: "agent.local/target".to_owned(),
            subject: "mesh.focus.created".to_owned(),
            timestamp_unix_ms: 1_700_000_000_000 + seq,
            browser_received_unix_ms: None,
            state: TimelineEventState::Accepted,
            causal_marker: TimelineCausalMarker::Root,
            parent_message_id: None,
            safe_payload_summary: serde_json::json!({}),
            suggested_next_action: None,
            cli_handoff_command: None,
            recovery_cue: None,
        }
    }

    #[test]
    fn within_budget_trace_renders_full_mode() {
        let page = TimelinePage::ready(vec![event(1, "msg-1"), event(2, "msg-2")]);
        let trace = FocusedTrace::open("corr-focus", page, Vec::new(), Vec::new());
        assert_eq!(trace.mode, FocusedTraceMode::Full);
        assert_eq!(trace.to_json()["mode"], "full");
    }

    #[test]
    fn over_budget_or_partial_window_trace_renders_windowed_mode() {
        let events: Vec<TimelineEvent> = (1..=600u64).map(|s| event(s, &format!("msg-{s}"))).collect();
        let page = TimelinePage::ready(events);
        let trace = FocusedTrace::open("corr-focus", page, Vec::new(), Vec::new());
        assert_eq!(trace.mode, FocusedTraceMode::Windowed);

        let small_partial = TimelinePage::paginate(
            vec![event(1, "msg-1")],
            (1, 10),
            10,
            Vec::new(),
        );
        let partial_trace = FocusedTrace::open("corr-focus", small_partial, Vec::new(), Vec::new());
        assert_eq!(partial_trace.mode, FocusedTraceMode::Windowed);
    }

    #[test]
    fn paused_trace_does_not_ingest_live_events() {
        let page = TimelinePage::ready(vec![event(1, "msg-1")]);
        let trace = FocusedTrace::open("corr-focus", page, Vec::new(), Vec::new()).pause();
        let updated = trace.ingest_live(vec![event(2, "msg-2")]);
        assert_eq!(updated.page.events.len(), 1);
        let resumed = updated.resume().ingest_live(vec![event(2, "msg-2")]);
        assert_eq!(resumed.page.events.len(), 2);
    }

    #[test]
    fn recovery_cues_and_handoffs_appear_in_json() {
        let page = TimelinePage::ready(vec![event(1, "msg-1")]);
        let handoff = CliHandoff::build(
            CliHandoffOperation::Trace,
            &["corr-focus"],
            &[],
            "inspect trace",
            "json output",
        )
        .expect("valid handoff");
        let trace = FocusedTrace::open(
            "corr-focus",
            page,
            vec![
                FocusedTraceRecoveryCue::InspectTrace,
                FocusedTraceRecoveryCue::AuditVerify,
            ],
            vec![handoff],
        );
        let json = trace.to_json();
        assert_eq!(json["correlation_id"], "corr-focus");
        let cues = json["recovery_cues"].as_array().unwrap();
        assert!(cues.iter().any(|c| c == "inspect_trace"));
        assert!(cues.iter().any(|c| c == "audit_verify"));
        let handoffs = json["handoffs"].as_array().unwrap();
        assert_eq!(handoffs.len(), 1);
        assert_eq!(handoffs[0]["operation"], "trace");
    }

    #[test]
    fn schema_version_pins_focused_trace_contract() {
        let page = TimelinePage::ready(Vec::new());
        let trace = FocusedTrace::open("corr-focus", page, Vec::new(), Vec::new());
        assert_eq!(trace.mapping_version, "zornmesh.ui.focused_trace.v1");
        assert_eq!(
            trace.to_json()["schema_version"],
            "zornmesh.ui.focused_trace.v1"
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum TimelineEventState {
    Pending,
    Queued,
    Accepted,
    Delivered,
    Acknowledged,
    Rejected,
    Failed,
    Cancelled,
    Replayed,
    DeadLettered,
    Stale,
    Unknown,
}

impl TimelineEventState {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Queued => "queued",
            Self::Accepted => "accepted",
            Self::Delivered => "delivered",
            Self::Acknowledged => "acknowledged",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Replayed => "replayed",
            Self::DeadLettered => "dead_lettered",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum TimelineCausalMarker {
    Root,
    CausedBy,
    RespondsTo,
    ReplayedFrom,
    RetryOf,
    DeadLetterTerminal,
    LateArrival,
    Reconstructed,
    Unknown,
}

impl TimelineCausalMarker {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::CausedBy => "caused_by",
            Self::RespondsTo => "responds_to",
            Self::ReplayedFrom => "replayed_from",
            Self::RetryOf => "retry_of",
            Self::DeadLetterTerminal => "dead_letter_terminal",
            Self::LateArrival => "late_arrival",
            Self::Reconstructed => "reconstructed",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct TimelineEvent {
    pub daemon_sequence: u64,
    pub message_id: String,
    pub correlation_id: String,
    pub trace_id: String,
    pub source_agent: String,
    pub target_or_subject: String,
    pub subject: String,
    pub timestamp_unix_ms: u64,
    pub browser_received_unix_ms: Option<u64>,
    pub state: TimelineEventState,
    pub causal_marker: TimelineCausalMarker,
    pub parent_message_id: Option<String>,
    pub safe_payload_summary: serde_json::Value,
    pub suggested_next_action: Option<&'static str>,
    pub cli_handoff_command: Option<String>,
    pub recovery_cue: Option<&'static str>,
}

#[allow(dead_code)]
impl TimelineEvent {
    pub(crate) fn stable_identity(&self) -> (u64, String) {
        (self.daemon_sequence, self.message_id.clone())
    }

    pub(crate) fn evidence_flags(&self) -> Vec<&'static str> {
        match self.causal_marker {
            TimelineCausalMarker::LateArrival => vec!["late"],
            TimelineCausalMarker::Reconstructed => vec!["reconstructed"],
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct TimelineGapMarker {
    pub before_sequence: u64,
    pub after_sequence: u64,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum TimelinePanelCondition {
    Ready,
    Empty,
    Loading,
    PartialWindow,
    Stale,
    Unavailable,
    SessionExpired,
}

impl TimelinePanelCondition {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Empty => "empty",
            Self::Loading => "loading",
            Self::PartialWindow => "partial_window",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
            Self::SessionExpired => "session_expired",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct TimelinePage {
    pub condition: TimelinePanelCondition,
    pub events: Vec<TimelineEvent>,
    pub loaded_range: Option<(u64, u64)>,
    pub total_count: usize,
    pub unknown_count: usize,
    pub gaps: Vec<TimelineGapMarker>,
    pub partial_window: bool,
    pub mapping_version: &'static str,
    pub selected_message_id: Option<String>,
}

#[allow(dead_code)]
impl TimelinePage {
    pub(crate) fn ready(mut events: Vec<TimelineEvent>) -> Self {
        events.sort_by_key(|event| event.daemon_sequence);
        let condition = if events.is_empty() {
            TimelinePanelCondition::Empty
        } else {
            TimelinePanelCondition::Ready
        };
        let loaded_range = events.first().zip(events.last()).map(|(first, last)| {
            (first.daemon_sequence, last.daemon_sequence)
        });
        let unknown_count = events
            .iter()
            .filter(|event| event.state == TimelineEventState::Unknown)
            .count();
        let total_count = events.len();
        Self {
            condition,
            events,
            loaded_range,
            total_count,
            unknown_count,
            gaps: Vec::new(),
            partial_window: false,
            mapping_version: "zornmesh.ui.timeline.v1",
            selected_message_id: None,
        }
    }

    pub(crate) fn unavailable(reason: &'static str) -> Self {
        Self {
            condition: TimelinePanelCondition::Unavailable,
            events: Vec::new(),
            loaded_range: None,
            total_count: 0,
            unknown_count: 0,
            gaps: vec![TimelineGapMarker {
                before_sequence: 0,
                after_sequence: 0,
                reason,
            }],
            partial_window: false,
            mapping_version: "zornmesh.ui.timeline.v1",
            selected_message_id: None,
        }
    }

    pub(crate) fn session_expired() -> Self {
        Self {
            condition: TimelinePanelCondition::SessionExpired,
            events: Vec::new(),
            loaded_range: None,
            total_count: 0,
            unknown_count: 0,
            gaps: Vec::new(),
            partial_window: false,
            mapping_version: "zornmesh.ui.timeline.v1",
            selected_message_id: None,
        }
    }

    pub(crate) fn paginate(
        events: Vec<TimelineEvent>,
        window: (u64, u64),
        total_count: usize,
        gaps: Vec<TimelineGapMarker>,
    ) -> Self {
        let mut page = Self::ready(events);
        page.loaded_range = (!page.events.is_empty()).then_some(window);
        page.total_count = total_count;
        page.gaps = gaps;
        page.partial_window = page.events.len() < total_count || !page.gaps.is_empty();
        if page.partial_window {
            page.condition = TimelinePanelCondition::PartialWindow;
        }
        page
    }

    pub(crate) fn select(mut self, message_id: &str) -> Self {
        let exists = self
            .events
            .iter()
            .any(|event| event.message_id == message_id);
        self.selected_message_id = if exists {
            Some(message_id.to_owned())
        } else {
            None
        };
        self
    }

    pub(crate) fn append_live(mut self, mut new_events: Vec<TimelineEvent>) -> Self {
        let prior_selection = self.selected_message_id.clone();
        let mut seen: BTreeSet<(u64, String)> =
            self.events.iter().map(TimelineEvent::stable_identity).collect();
        for event in new_events.drain(..) {
            if seen.insert(event.stable_identity()) {
                self.events.push(event);
            }
        }
        self.events.sort_by_key(|event| event.daemon_sequence);
        if let Some((_, ref mut high)) = self.loaded_range
            && let Some(last) = self.events.last()
        {
            *high = (*high).max(last.daemon_sequence);
        }
        if self.loaded_range.is_none()
            && let (Some(first), Some(last)) = (self.events.first(), self.events.last())
        {
            self.loaded_range = Some((first.daemon_sequence, last.daemon_sequence));
        }
        self.total_count = self.total_count.max(self.events.len());
        self.unknown_count = self
            .events
            .iter()
            .filter(|event| event.state == TimelineEventState::Unknown)
            .count();
        self.selected_message_id = prior_selection.filter(|id| {
            self.events.iter().any(|event| &event.message_id == id)
        });
        if !self.events.is_empty() && self.condition == TimelinePanelCondition::Empty {
            self.condition = TimelinePanelCondition::Ready;
        }
        self
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema_version": self.mapping_version,
            "condition": self.condition.as_str(),
            "ordering": "daemon_sequence",
            "loaded_range": self.loaded_range.map(|(low, high)| serde_json::json!({"low": low, "high": high})),
            "total_count": self.total_count,
            "unknown_count": self.unknown_count,
            "partial_window": self.partial_window,
            "selected_message_id": self.selected_message_id,
            "gaps": self.gaps.iter().map(|gap| serde_json::json!({
                "before_sequence": gap.before_sequence,
                "after_sequence": gap.after_sequence,
                "reason": gap.reason,
            })).collect::<Vec<_>>(),
            "events": self.events.iter().map(|event| serde_json::json!({
                "daemon_sequence": event.daemon_sequence,
                "message_id": event.message_id,
                "correlation_id": event.correlation_id,
                "trace_id": event.trace_id,
                "source_agent": event.source_agent,
                "target_or_subject": event.target_or_subject,
                "subject": event.subject,
                "timestamp_unix_ms": event.timestamp_unix_ms,
                "browser_received_unix_ms": event.browser_received_unix_ms,
                "state": event.state.as_str(),
                "causal_marker": event.causal_marker.as_str(),
                "event_identity": format!("{}:{}", event.daemon_sequence, event.message_id),
                "evidence_flags": event.evidence_flags(),
                "parent_message_id": event.parent_message_id,
                "safe_payload_summary": event.safe_payload_summary,
                "suggested_next_action": event.suggested_next_action,
                "cli_handoff_command": event.cli_handoff_command,
                "recovery_cue": event.recovery_cue,
            })).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod timeline_tests {
    use super::*;

    fn make_event(sequence: u64, message: &str, state: TimelineEventState) -> TimelineEvent {
        TimelineEvent {
            daemon_sequence: sequence,
            message_id: message.to_owned(),
            correlation_id: "corr-timeline".to_owned(),
            trace_id: "trace-timeline".to_owned(),
            source_agent: "agent.local/sender".to_owned(),
            target_or_subject: "agent.local/receiver".to_owned(),
            subject: "mesh.timeline.created".to_owned(),
            timestamp_unix_ms: 1_700_000_000_000 + sequence,
            browser_received_unix_ms: Some(1_700_000_000_999),
            state,
            causal_marker: TimelineCausalMarker::Root,
            parent_message_id: None,
            safe_payload_summary: serde_json::json!({"payload_len": 16}),
            suggested_next_action: None,
            cli_handoff_command: None,
            recovery_cue: None,
        }
    }

    #[test]
    fn ready_page_orders_by_daemon_sequence_not_browser_receipt_time() {
        let mut later = make_event(2, "msg-late", TimelineEventState::Accepted);
        later.browser_received_unix_ms = Some(1_500_000_000_000);
        let earlier = make_event(1, "msg-first", TimelineEventState::Accepted);
        let page = TimelinePage::ready(vec![later, earlier]);
        let sequences: Vec<u64> = page.events.iter().map(|e| e.daemon_sequence).collect();
        assert_eq!(sequences, vec![1, 2]);
        assert_eq!(page.loaded_range, Some((1, 2)));
        assert_eq!(page.condition, TimelinePanelCondition::Ready);
        let json = page.to_json();
        assert_eq!(json["ordering"], "daemon_sequence");
    }

    #[test]
    fn empty_page_marks_empty_condition() {
        let page = TimelinePage::ready(Vec::new());
        assert_eq!(page.condition, TimelinePanelCondition::Empty);
        assert_eq!(page.events.len(), 0);
        assert!(page.loaded_range.is_none());
    }

    #[test]
    fn unknown_state_is_counted_separately_for_taxonomy_drift_detection() {
        let page = TimelinePage::ready(vec![
            make_event(1, "msg-1", TimelineEventState::Accepted),
            make_event(2, "msg-2", TimelineEventState::Unknown),
            make_event(3, "msg-3", TimelineEventState::Unknown),
        ]);
        assert_eq!(page.unknown_count, 2);
        assert_eq!(page.total_count, 3);
    }

    #[test]
    fn paginate_marks_partial_window_when_total_exceeds_loaded() {
        let events = vec![
            make_event(10, "msg-10", TimelineEventState::Accepted),
            make_event(11, "msg-11", TimelineEventState::Accepted),
        ];
        let page = TimelinePage::paginate(events, (10, 11), 100, Vec::new());
        assert!(page.partial_window);
        assert_eq!(page.condition, TimelinePanelCondition::PartialWindow);
        assert_eq!(page.loaded_range, Some((10, 11)));
        assert_eq!(page.total_count, 100);
    }

    #[test]
    fn paginate_records_gap_markers_without_reordering_events() {
        let events = vec![
            make_event(1, "msg-1", TimelineEventState::Accepted),
            make_event(5, "msg-5", TimelineEventState::Accepted),
        ];
        let gaps = vec![TimelineGapMarker {
            before_sequence: 1,
            after_sequence: 5,
            reason: "retention_purge",
        }];
        let page = TimelinePage::paginate(events, (1, 5), 5, gaps);
        let sequences: Vec<u64> = page.events.iter().map(|e| e.daemon_sequence).collect();
        assert_eq!(sequences, vec![1, 5]);
        assert!(page.partial_window);
        assert_eq!(page.gaps.len(), 1);
        assert_eq!(page.gaps[0].reason, "retention_purge");
    }

    #[test]
    fn append_live_keeps_selection_stable_when_event_remains() {
        let initial = TimelinePage::ready(vec![
            make_event(1, "msg-1", TimelineEventState::Accepted),
            make_event(2, "msg-2", TimelineEventState::Accepted),
        ])
        .select("msg-2");
        assert_eq!(initial.selected_message_id.as_deref(), Some("msg-2"));
        let appended = initial.append_live(vec![make_event(3, "msg-3", TimelineEventState::Accepted)]);
        assert_eq!(appended.selected_message_id.as_deref(), Some("msg-2"));
        let sequences: Vec<u64> = appended.events.iter().map(|e| e.daemon_sequence).collect();
        assert_eq!(sequences, vec![1, 2, 3]);
        assert_eq!(appended.loaded_range, Some((1, 3)));
    }

    #[test]
    fn select_unknown_message_clears_selection() {
        let page =
            TimelinePage::ready(vec![make_event(1, "msg-1", TimelineEventState::Accepted)])
                .select("msg-missing");
        assert!(page.selected_message_id.is_none());
    }

    #[test]
    fn unavailable_and_session_expired_pages_carry_persistent_state() {
        let unavailable = TimelinePage::unavailable("daemon_unreachable");
        assert_eq!(unavailable.condition, TimelinePanelCondition::Unavailable);
        assert_eq!(unavailable.gaps.len(), 1);
        assert_eq!(unavailable.gaps[0].reason, "daemon_unreachable");

        let expired = TimelinePage::session_expired();
        assert_eq!(expired.condition, TimelinePanelCondition::SessionExpired);
        assert_eq!(expired.to_json()["condition"], "session_expired");
    }

    #[test]
    fn schema_version_pins_timeline_contract() {
        let page = TimelinePage::ready(Vec::new());
        assert_eq!(page.mapping_version, "zornmesh.ui.timeline.v1");
        assert_eq!(page.to_json()["schema_version"], "zornmesh.ui.timeline.v1");
    }

    #[test]
    fn five_hundred_event_window_renders_in_under_one_second() {
        let events: Vec<TimelineEvent> = (1..=500u64)
            .map(|seq| make_event(seq, &format!("msg-{seq}"), TimelineEventState::Accepted))
            .collect();
        let started = std::time::Instant::now();
        let page = TimelinePage::ready(events);
        let json = page.to_json();
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "500-event timeline must render under 1s, took {elapsed:?}"
        );
        assert_eq!(page.events.len(), 500);
        assert_eq!(json["events"].as_array().unwrap().len(), 500);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum RosterStatus {
    Active,
    Stale,
    Errored,
    Disconnected,
    Reconnecting,
    Unknown,
}

impl RosterStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Stale => "stale",
            Self::Errored => "errored",
            Self::Disconnected => "disconnected",
            Self::Reconnecting => "reconnecting",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum RosterTransport {
    NativeSdk,
    McpStdio,
    Unknown,
}

impl RosterTransport {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NativeSdk => "native_sdk",
            Self::McpStdio => "mcp_stdio",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum RosterPanelCondition {
    Ready,
    Empty,
    Loading,
    Stale,
    Degraded,
    Unavailable,
    SessionExpired,
}

impl RosterPanelCondition {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Empty => "empty",
            Self::Loading => "loading",
            Self::Stale => "stale",
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
            Self::SessionExpired => "session_expired",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct RosterEntry {
    pub display_name: String,
    pub agent_id: String,
    pub status: RosterStatus,
    pub transport: RosterTransport,
    pub capability_summary: Vec<String>,
    pub last_seen_unix_ms: u64,
    pub recent_activity_count: u32,
    pub warnings: Vec<&'static str>,
    pub high_privilege_required: bool,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub(crate) struct RosterFilter {
    pub query: Option<String>,
    pub status: Option<RosterStatus>,
    pub transport: Option<RosterTransport>,
    pub capability: Option<String>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct RosterSnapshot {
    pub condition: RosterPanelCondition,
    pub next_action: Option<&'static str>,
    pub entries: Vec<RosterEntry>,
    pub mapping_version: &'static str,
}

#[allow(dead_code)]
impl RosterSnapshot {
    pub(crate) fn ready(entries: Vec<RosterEntry>) -> Self {
        let condition = if entries.is_empty() {
            RosterPanelCondition::Empty
        } else {
            RosterPanelCondition::Ready
        };
        let next_action = match condition {
            RosterPanelCondition::Empty => Some("connect_an_agent_or_inspect_doctor"),
            _ => None,
        };
        Self {
            condition,
            next_action,
            entries,
            mapping_version: "zornmesh.ui.roster.v1",
        }
    }

    pub(crate) fn unavailable(reason: &'static str) -> Self {
        Self {
            condition: RosterPanelCondition::Unavailable,
            next_action: Some(reason),
            entries: Vec::new(),
            mapping_version: "zornmesh.ui.roster.v1",
        }
    }

    pub(crate) fn session_expired() -> Self {
        Self {
            condition: RosterPanelCondition::SessionExpired,
            next_action: Some("rerun_zornmesh_ui_to_renew_session"),
            entries: Vec::new(),
            mapping_version: "zornmesh.ui.roster.v1",
        }
    }

    pub(crate) fn filter(self, filter: &RosterFilter) -> Self {
        let entries = self
            .entries
            .into_iter()
            .filter(|entry| filter_matches(entry, filter))
            .collect::<Vec<_>>();
        let condition = if entries.is_empty()
            && self.condition == RosterPanelCondition::Ready
        {
            RosterPanelCondition::Empty
        } else {
            self.condition
        };
        Self {
            condition,
            next_action: self.next_action,
            entries,
            mapping_version: self.mapping_version,
        }
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema_version": self.mapping_version,
            "condition": self.condition.as_str(),
            "next_action": self.next_action,
            "entries": self.entries.iter().map(|entry| serde_json::json!({
                "display_name": entry.display_name,
                "agent_id": entry.agent_id,
                "status": entry.status.as_str(),
                "transport": entry.transport.as_str(),
                "capability_summary": entry.capability_summary,
                "last_seen_unix_ms": entry.last_seen_unix_ms,
                "recent_activity_count": entry.recent_activity_count,
                "warnings": entry.warnings,
                "high_privilege_required": entry.high_privilege_required,
            })).collect::<Vec<_>>(),
        })
    }
}

#[allow(dead_code)]
fn filter_matches(entry: &RosterEntry, filter: &RosterFilter) -> bool {
    if let Some(query) = filter.query.as_deref() {
        let needle = query.to_ascii_lowercase();
        let haystack = format!(
            "{} {} {}",
            entry.display_name.to_ascii_lowercase(),
            entry.agent_id.to_ascii_lowercase(),
            entry
                .capability_summary
                .iter()
                .map(|cap| cap.to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join(" "),
        );
        if !haystack.contains(&needle) {
            return false;
        }
    }
    if let Some(status) = filter.status
        && entry.status != status
    {
        return false;
    }
    if let Some(transport) = filter.transport
        && entry.transport != transport
    {
        return false;
    }
    if let Some(capability) = filter.capability.as_deref()
        && !entry
            .capability_summary
            .iter()
            .any(|cap| cap.eq_ignore_ascii_case(capability))
    {
        return false;
    }
    if let Some(warning) = filter.warning.as_deref()
        && !entry
            .warnings
            .iter()
            .any(|cap| cap.eq_ignore_ascii_case(warning))
    {
        return false;
    }
    true
}

#[cfg(test)]
mod roster_tests {
    use super::*;

    fn entry(
        display: &str,
        id: &str,
        status: RosterStatus,
        transport: RosterTransport,
    ) -> RosterEntry {
        RosterEntry {
            display_name: display.to_owned(),
            agent_id: id.to_owned(),
            status,
            transport,
            capability_summary: vec!["mesh.basic.send".to_owned()],
            last_seen_unix_ms: 1_700_000_000_000,
            recent_activity_count: 3,
            warnings: Vec::new(),
            high_privilege_required: false,
        }
    }

    #[test]
    fn empty_snapshot_marks_empty_condition_with_next_action() {
        let snapshot = RosterSnapshot::ready(Vec::new());
        assert_eq!(snapshot.condition, RosterPanelCondition::Empty);
        assert!(snapshot.next_action.is_some());
        let json = snapshot.to_json();
        assert_eq!(json["condition"], "empty");
        assert_eq!(json["entries"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn ready_snapshot_distinguishes_status_and_transport_for_each_entry() {
        let snapshot = RosterSnapshot::ready(vec![
            entry(
                "alice",
                "agent.local/alice",
                RosterStatus::Active,
                RosterTransport::NativeSdk,
            ),
            entry(
                "bob",
                "agent.local/bob",
                RosterStatus::Stale,
                RosterTransport::McpStdio,
            ),
            entry(
                "carol",
                "agent.local/carol",
                RosterStatus::Disconnected,
                RosterTransport::NativeSdk,
            ),
        ]);
        assert_eq!(snapshot.condition, RosterPanelCondition::Ready);
        let statuses: Vec<&str> = snapshot
            .entries
            .iter()
            .map(|entry| entry.status.as_str())
            .collect();
        assert_eq!(statuses, vec!["active", "stale", "disconnected"]);
        let transports: Vec<&str> = snapshot
            .entries
            .iter()
            .map(|entry| entry.transport.as_str())
            .collect();
        assert_eq!(transports, vec!["native_sdk", "mcp_stdio", "native_sdk"]);
    }

    #[test]
    fn high_privilege_warning_surfaces_without_enabling_unsafe_actions() {
        let mut high_priv = entry(
            "ops",
            "agent.local/ops",
            RosterStatus::Active,
            RosterTransport::NativeSdk,
        );
        high_priv.high_privilege_required = true;
        high_priv.warnings.push("high_privilege_capability_present");
        let snapshot = RosterSnapshot::ready(vec![high_priv]);
        let json = snapshot.to_json();
        assert_eq!(json["entries"][0]["high_privilege_required"], true);
        let warnings = json["entries"][0]["warnings"].as_array().unwrap();
        assert!(warnings.iter().any(|w| w == "high_privilege_capability_present"));
    }

    #[test]
    fn filter_by_query_matches_id_name_and_capability() {
        let snapshot = RosterSnapshot::ready(vec![
            entry(
                "Alice",
                "agent.local/alice",
                RosterStatus::Active,
                RosterTransport::NativeSdk,
            ),
            entry(
                "Bob",
                "agent.local/bob",
                RosterStatus::Active,
                RosterTransport::NativeSdk,
            ),
        ]);
        let by_name = snapshot.clone().filter(&RosterFilter {
            query: Some("alice".to_owned()),
            ..Default::default()
        });
        assert_eq!(by_name.entries.len(), 1);
        assert_eq!(by_name.entries[0].agent_id, "agent.local/alice");

        let by_capability = snapshot.filter(&RosterFilter {
            capability: Some("mesh.basic.send".to_owned()),
            ..Default::default()
        });
        assert_eq!(by_capability.entries.len(), 2);
    }

    #[test]
    fn filter_keeping_no_entries_marks_empty_panel() {
        let snapshot = RosterSnapshot::ready(vec![entry(
            "alice",
            "agent.local/alice",
            RosterStatus::Active,
            RosterTransport::NativeSdk,
        )]);
        let filtered = snapshot.filter(&RosterFilter {
            query: Some("nonexistent".to_owned()),
            ..Default::default()
        });
        assert_eq!(filtered.condition, RosterPanelCondition::Empty);
    }

    #[test]
    fn unavailable_and_session_expired_states_render_persistent_panels() {
        let unavailable = RosterSnapshot::unavailable("daemon_unreachable");
        assert_eq!(unavailable.condition, RosterPanelCondition::Unavailable);
        assert_eq!(unavailable.next_action, Some("daemon_unreachable"));
        assert_eq!(unavailable.to_json()["condition"], "unavailable");

        let expired = RosterSnapshot::session_expired();
        assert_eq!(expired.condition, RosterPanelCondition::SessionExpired);
        assert_eq!(expired.to_json()["condition"], "session_expired");
        assert!(expired.next_action.is_some());
    }

    #[test]
    fn filter_by_status_and_transport_isolates_categories() {
        let snapshot = RosterSnapshot::ready(vec![
            entry(
                "alice",
                "agent.local/alice",
                RosterStatus::Active,
                RosterTransport::NativeSdk,
            ),
            entry(
                "bob",
                "agent.local/bob",
                RosterStatus::Stale,
                RosterTransport::McpStdio,
            ),
        ]);
        let stale_only = snapshot.clone().filter(&RosterFilter {
            status: Some(RosterStatus::Stale),
            ..Default::default()
        });
        assert_eq!(stale_only.entries.len(), 1);
        assert_eq!(stale_only.entries[0].agent_id, "agent.local/bob");

        let mcp_only = snapshot.filter(&RosterFilter {
            transport: Some(RosterTransport::McpStdio),
            ..Default::default()
        });
        assert_eq!(mcp_only.entries.len(), 1);
        assert_eq!(mcp_only.entries[0].agent_id, "agent.local/bob");
    }

    #[test]
    fn schema_version_pins_roster_contract() {
        let snapshot = RosterSnapshot::ready(Vec::new());
        assert_eq!(snapshot.mapping_version, "zornmesh.ui.roster.v1");
        assert_eq!(snapshot.to_json()["schema_version"], "zornmesh.ui.roster.v1");
    }
}

#[cfg(test)]
mod ui_tests {
    use super::*;

    fn fresh_session() -> UiLaunchSession {
        UiLaunchSession::reserve(0).expect("ephemeral loopback bind")
    }

    #[test]
    fn reserve_yields_loopback_bind_and_high_entropy_tokens() {
        let session = fresh_session();
        assert!(
            session.loopback_origin().starts_with("http://127.0.0.1:"),
            "must bind loopback only"
        );
        assert_eq!(session.session_token().len(), UI_TOKEN_HEX_LEN);
        assert_eq!(session.csrf_token().len(), UI_TOKEN_HEX_LEN);
        assert!(session.session_token().chars().all(|c| c.is_ascii_hexdigit()));
        assert!(session.csrf_token().chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(session.session_token(), session.csrf_token());
    }

    #[test]
    fn distinct_launches_produce_distinct_tokens() {
        let a = fresh_session();
        let b = fresh_session();
        assert_ne!(a.session_token(), b.session_token());
        assert_ne!(a.csrf_token(), b.csrf_token());
    }

    #[test]
    fn validate_session_token_classifies_outcomes() {
        let session = fresh_session();
        assert_eq!(session.validate_session_token(None), UiTokenOutcome::Missing);
        assert_eq!(
            session.validate_session_token(Some("nope")),
            UiTokenOutcome::Invalid
        );
        let copy = session.session_token().to_owned();
        assert_eq!(
            session.validate_session_token(Some(&copy)),
            UiTokenOutcome::Verified
        );
    }

    #[test]
    fn revoke_blocks_token_and_csrf_validation() {
        let session = fresh_session();
        let token = session.session_token().to_owned();
        let csrf = session.csrf_token().to_owned();
        session.revoke();
        assert_eq!(
            session.validate_session_token(Some(&token)),
            UiTokenOutcome::Revoked
        );
        assert_eq!(session.validate_csrf(Some(&csrf)), UiCsrfOutcome::Revoked);
    }

    #[test]
    fn validate_origin_only_allows_exact_loopback() {
        let session = fresh_session();
        let origin = session.loopback_origin();
        assert_eq!(
            session.validate_origin(Some(&origin)),
            UiOriginOutcome::Allowed
        );
        assert_eq!(
            session.validate_origin(Some("http://example.com")),
            UiOriginOutcome::Rejected
        );
        assert_eq!(
            session.validate_origin(Some("http://127.0.0.1:1")),
            UiOriginOutcome::Rejected
        );
        assert_eq!(session.validate_origin(None), UiOriginOutcome::Missing);
    }

    #[test]
    fn validate_csrf_classifies_outcomes() {
        let session = fresh_session();
        assert_eq!(session.validate_csrf(None), UiCsrfOutcome::Missing);
        assert_eq!(
            session.validate_csrf(Some("not-the-csrf")),
            UiCsrfOutcome::Invalid
        );
        let copy = session.csrf_token().to_owned();
        assert_eq!(session.validate_csrf(Some(&copy)), UiCsrfOutcome::Verified);
    }

    #[test]
    fn launch_report_json_pins_security_posture_without_leaking_tokens() {
        let session = fresh_session();
        let report = session.launch_report_json(true);
        assert_eq!(report["status"], "ready");
        assert_eq!(report["bundled_assets"], "offline");
        assert_eq!(report["referrer_policy"], UI_REFERRER_POLICY);
        assert_eq!(report["non_loopback_bind_refused"], true);
        assert_eq!(report["actor_session_binding"], "server_derived");
        assert_eq!(report["websocket_sse_session_required"], true);
        assert_eq!(report["session_token_length"], UI_TOKEN_HEX_LEN as u64);
        assert_eq!(report["csrf_token_length"], UI_TOKEN_HEX_LEN as u64);
        assert_eq!(report["open_browser"], false);
        let token = session.session_token();
        let csrf = session.csrf_token();
        let serialized = serde_json::to_string(&report).expect("report serializes");
        assert!(!serialized.contains(token), "report must not leak session token");
        assert!(!serialized.contains(csrf), "report must not leak CSRF token");
    }
}
