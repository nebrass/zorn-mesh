use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::daemon::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRuntime};

use super::{ConnectOptions, SdkError};

struct ManagedDaemon {
    shutdown: std::sync::Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

static DAEMONS: OnceLock<Mutex<HashMap<PathBuf, ManagedDaemon>>> = OnceLock::new();
static CONNECT_GATE: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn connect_gate() -> &'static Mutex<()> {
    CONNECT_GATE.get_or_init(|| Mutex::new(()))
}

pub(crate) fn ensure_daemon_started(options: &ConnectOptions) -> Result<bool, SdkError> {
    let mut daemons = registry()
        .lock()
        .expect("daemon registry lock is not poisoned");
    if daemons.contains_key(options.socket_path()) {
        return Ok(false);
    }

    let shutdown = std::sync::Arc::new(AtomicBool::new(false));
    let socket_path = options.socket_path().to_path_buf();
    let allow_elevated_for_tests = options.allow_elevated_daemon_for_tests;
    let start_delay = options.daemon_start_delay_for_tests;

    let handle = if start_delay.is_zero() {
        match start_runtime(socket_path.clone(), allow_elevated_for_tests) {
            Ok(runtime) => spawn_runtime(runtime, std::sync::Arc::clone(&shutdown)),
            Err(error) if error.code() == DaemonErrorCode::ExistingOwner => return Ok(false),
            Err(error) => return Err(SdkError::from(error)),
        }
    } else {
        let delayed_shutdown = std::sync::Arc::clone(&shutdown);
        thread::spawn(move || {
            if wait_for_start_delay(start_delay, &delayed_shutdown) {
                match start_runtime(socket_path, allow_elevated_for_tests) {
                    Ok(runtime) => run_runtime(runtime, delayed_shutdown),
                    Err(error) => eprintln!("{error}"),
                }
            }
        })
    };

    daemons.insert(
        options.socket_path().to_path_buf(),
        ManagedDaemon {
            shutdown,
            handle: Some(handle),
        },
    );
    Ok(true)
}

pub(crate) fn shutdown_daemon_for_tests(socket_path: &Path) {
    let managed = registry()
        .lock()
        .expect("daemon registry lock is not poisoned")
        .remove(socket_path);
    if let Some(mut managed) = managed {
        managed.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = managed.handle.take() {
            let _ = handle.join();
        }
    }
}

pub(crate) fn has_daemon_for_tests(socket_path: &Path) -> bool {
    registry()
        .lock()
        .expect("daemon registry lock is not poisoned")
        .contains_key(socket_path)
}

pub(crate) fn daemon_count_for_tests(socket_path: &Path) -> usize {
    usize::from(has_daemon_for_tests(socket_path))
}

fn registry() -> &'static Mutex<HashMap<PathBuf, ManagedDaemon>> {
    DAEMONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn start_runtime(
    socket_path: PathBuf,
    allow_elevated_for_tests: bool,
) -> Result<DaemonRuntime, DaemonError> {
    let mut config = DaemonConfig::for_socket_path(socket_path);
    if allow_elevated_for_tests {
        config = config.allow_elevated_for_tests(true);
    }
    DaemonRuntime::start(config)
}

fn spawn_runtime(runtime: DaemonRuntime, shutdown: std::sync::Arc<AtomicBool>) -> JoinHandle<()> {
    thread::spawn(move || run_runtime(runtime, shutdown))
}

fn run_runtime(mut runtime: DaemonRuntime, shutdown: std::sync::Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Relaxed) {
        if runtime.accept_once().is_err() {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let _ = runtime.shutdown_with_in_flight(0);
}

fn wait_for_start_delay(delay: Duration, shutdown: &AtomicBool) -> bool {
    let started_at = Instant::now();
    while started_at.elapsed() < delay {
        if shutdown.load(Ordering::Relaxed) {
            return false;
        }
        let remaining = delay.saturating_sub(started_at.elapsed());
        thread::sleep(Duration::from_millis(5).min(remaining));
    }
    !shutdown.load(Ordering::Relaxed)
}
