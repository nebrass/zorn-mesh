#![doc = "Command skeleton for the public zornmesh CLI."]

pub const ROOT_HELP: &str = include_str!("../../../fixtures/cli/root-help.stdout");
pub const DAEMON_HELP: &str = include_str!("../../../fixtures/cli/daemon-help.stdout");
pub const TRACE_HELP: &str = include_str!("../../../fixtures/cli/trace-help.stdout");
pub const VERSION: &str = "zornmesh 0.1.0\n";

pub fn run(args: impl IntoIterator<Item = String>) -> i32 {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.as_slice() {
        [] => {
            print!("{ROOT_HELP}");
            0
        }
        [flag] if flag == "--help" || flag == "-h" || flag == "help" => {
            print!("{ROOT_HELP}");
            0
        }
        [flag] if flag == "--version" || flag == "-V" => {
            print!("{VERSION}");
            0
        }
        [command] if command == "daemon" => run_daemon(&[]),
        [command, rest @ ..] if command == "daemon" => run_daemon(rest),
        [command] if command == "trace" => {
            print!("{TRACE_HELP}");
            0
        }
        [command, flag]
            if command == "trace" && (flag == "--help" || flag == "-h" || flag == "help") =>
        {
            print!("{TRACE_HELP}");
            0
        }
        [command, ..] => {
            eprintln!("E_UNSUPPORTED_COMMAND: unsupported zornmesh command '{command}'");
            64
        }
    }
}

fn run_daemon(args: &[String]) -> i32 {
    match args {
        [] => match zornmesh_daemon::DaemonConfig::from_env()
            .and_then(zornmesh_daemon::run_foreground)
        {
            Ok(report) => match report.outcome {
                zornmesh_daemon::ShutdownOutcome::Clean => 0,
                zornmesh_daemon::ShutdownOutcome::BudgetExceeded => 75,
            },
            Err(error) => {
                eprintln!("{error}");
                daemon_exit_code(error.code())
            }
        },
        [flag] if flag == "--help" || flag == "-h" || flag == "help" => {
            print!("{DAEMON_HELP}");
            0
        }
        [flag, path] if flag == "--socket" => {
            let config = match zornmesh_daemon::DaemonConfig::from_env() {
                Ok(config) => config.with_socket_path(path),
                Err(error) => {
                    eprintln!("{error}");
                    return daemon_exit_code(error.code());
                }
            };

            match zornmesh_daemon::run_foreground(config) {
                Ok(report) => match report.outcome {
                    zornmesh_daemon::ShutdownOutcome::Clean => 0,
                    zornmesh_daemon::ShutdownOutcome::BudgetExceeded => 75,
                },
                Err(error) => {
                    eprintln!("{error}");
                    daemon_exit_code(error.code())
                }
            }
        }
        [flag, ..] if flag == "--socket" => {
            eprintln!("E_INVALID_CONFIG: --socket requires a path");
            64
        }
        [arg, ..] => {
            eprintln!("E_UNSUPPORTED_COMMAND: unsupported zornmesh daemon argument '{arg}'");
            64
        }
    }
}

fn daemon_exit_code(code: zornmesh_daemon::DaemonErrorCode) -> i32 {
    match code {
        zornmesh_daemon::DaemonErrorCode::ExistingOwner => 75,
        zornmesh_daemon::DaemonErrorCode::LocalTrustUnsafe
        | zornmesh_daemon::DaemonErrorCode::ElevatedPrivilege => 77,
        zornmesh_daemon::DaemonErrorCode::DaemonUnreachable => 69,
        zornmesh_daemon::DaemonErrorCode::InvalidConfig => 64,
        zornmesh_daemon::DaemonErrorCode::Io => 74,
    }
}
