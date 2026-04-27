#![doc = "Rust SDK entrypoints for zornmesh agents."]

pub const CRATE_BOUNDARY: &str = "zornmesh-sdk";

mod spawn;

use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use zornmesh_daemon::DaemonError;
use zornmesh_rpc::local::{self, ENV_NO_AUTOSPAWN, ENV_SOCKET_PATH, LocalError, LocalErrorCode};

pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_millis(1_000);
pub const DEFAULT_RETRY_DELAY: Duration = Duration::from_millis(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdkBoundary;

impl SdkBoundary {
    pub const fn name(self) -> &'static str {
        CRATE_BOUNDARY
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoSpawn {
    Enabled,
    Disabled,
}

impl AutoSpawn {
    pub const fn enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }

    fn from_no_autospawn_env(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("1" | "true" | "TRUE" | "yes" | "YES") => Self::Disabled,
            _ => Self::Enabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectOptions {
    socket_path: PathBuf,
    auto_spawn: AutoSpawn,
    connect_timeout: Duration,
    retry_delay: Duration,
    allow_elevated_daemon_for_tests: bool,
    daemon_start_delay_for_tests: Duration,
}

impl ConnectOptions {
    pub fn from_env() -> Result<Self, SdkError> {
        Self::from_env_pairs(std::env::vars())
    }

    pub fn from_env_pairs<I, K, V>(pairs: I) -> Result<Self, SdkError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut socket_path = None;
        let mut no_autospawn = None;
        for (key, value) in pairs {
            match key.as_ref() {
                ENV_SOCKET_PATH => socket_path = Some(PathBuf::from(value.as_ref())),
                ENV_NO_AUTOSPAWN => no_autospawn = Some(value.as_ref().to_owned()),
                _ => {}
            }
        }

        let socket_path = match socket_path {
            Some(path) if path.as_os_str().is_empty() => {
                return Err(SdkError::new(
                    SdkErrorCode::InvalidConfig,
                    "ZORN_SOCKET_PATH must not be empty",
                ));
            }
            Some(path) => path,
            None => local::default_socket_path().map_err(SdkError::from)?,
        };

        Ok(Self {
            socket_path,
            auto_spawn: AutoSpawn::from_no_autospawn_env(no_autospawn.as_deref()),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            retry_delay: DEFAULT_RETRY_DELAY,
            allow_elevated_daemon_for_tests: false,
            daemon_start_delay_for_tests: Duration::ZERO,
        })
    }

    pub fn for_socket(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            auto_spawn: AutoSpawn::Enabled,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            retry_delay: DEFAULT_RETRY_DELAY,
            allow_elevated_daemon_for_tests: false,
            daemon_start_delay_for_tests: Duration::ZERO,
        }
    }

    pub fn without_auto_spawn(mut self) -> Self {
        self.auto_spawn = AutoSpawn::Disabled;
        self
    }

    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    #[doc(hidden)]
    pub fn allow_elevated_daemon_for_tests(mut self) -> Self {
        self.allow_elevated_daemon_for_tests = true;
        self
    }

    #[doc(hidden)]
    pub fn with_daemon_start_delay_for_tests(mut self, delay: Duration) -> Self {
        self.daemon_start_delay_for_tests = delay;
        self
    }

    pub const fn auto_spawn_enabled(&self) -> bool {
        self.auto_spawn.enabled()
    }

    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }

    pub const fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }
}

#[derive(Debug)]
pub struct Mesh {
    socket_path: PathBuf,
}

impl Mesh {
    pub fn connect() -> Result<Self, SdkError> {
        Self::connect_with_options(ConnectOptions::from_env()?)
    }

    pub fn connect_with_options(options: ConnectOptions) -> Result<Self, SdkError> {
        let _connect_guard = spawn::connect_gate()
            .lock()
            .expect("SDK connect gate lock is not poisoned");
        let uid = local::effective_uid().map_err(SdkError::from)?;

        match local::connect_trusted_socket(&options.socket_path, uid) {
            Ok(_stream) => Ok(Self {
                socket_path: options.socket_path,
            }),
            Err(error)
                if error.code() == LocalErrorCode::DaemonUnreachable
                    && !options.auto_spawn_enabled() =>
            {
                Err(daemon_unreachable_autospawn_disabled())
            }
            Err(error) => {
                if error.code() != LocalErrorCode::DaemonUnreachable {
                    return Err(SdkError::from(error));
                }

                let sdk_started_daemon = spawn::ensure_daemon_started(&options)?;
                match wait_for_readiness(&options, uid) {
                    Ok(()) => Ok(Self {
                        socket_path: options.socket_path,
                    }),
                    Err(error) => {
                        if sdk_started_daemon
                            && error.code() == SdkErrorCode::DaemonReadinessTimeout
                        {
                            spawn::shutdown_daemon_for_tests(&options.socket_path);
                        }
                        Err(error)
                    }
                }
            }
        }
    }

    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }

    #[doc(hidden)]
    pub fn shutdown_autospawned_daemon_for_tests(socket_path: &Path) {
        spawn::shutdown_daemon_for_tests(socket_path);
    }

    #[doc(hidden)]
    pub fn has_autospawned_daemon_for_tests(socket_path: &Path) -> bool {
        spawn::has_daemon_for_tests(socket_path)
    }

    #[doc(hidden)]
    pub fn autospawned_daemon_count_for_tests(socket_path: &Path) -> usize {
        spawn::daemon_count_for_tests(socket_path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdkErrorCode {
    LocalTrustUnsafe,
    ElevatedPrivilege,
    DaemonUnreachable,
    DaemonReadinessTimeout,
    InvalidConfig,
    Io,
}

impl SdkErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalTrustUnsafe => "E_LOCAL_TRUST_UNSAFE",
            Self::ElevatedPrivilege => "E_ELEVATED_PRIVILEGE",
            Self::DaemonUnreachable => "E_DAEMON_UNREACHABLE",
            Self::DaemonReadinessTimeout => "E_DAEMON_READINESS_TIMEOUT",
            Self::InvalidConfig => "E_INVALID_CONFIG",
            Self::Io => "E_DAEMON_IO",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkError {
    code: SdkErrorCode,
    message: String,
}

impl SdkError {
    fn new(code: SdkErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> SdkErrorCode {
        self.code
    }

    pub const fn retryable(&self) -> bool {
        matches!(
            self.code,
            SdkErrorCode::DaemonUnreachable | SdkErrorCode::DaemonReadinessTimeout
        )
    }
}

impl From<LocalError> for SdkError {
    fn from(value: LocalError) -> Self {
        let code = match value.code() {
            LocalErrorCode::LocalTrustUnsafe => SdkErrorCode::LocalTrustUnsafe,
            LocalErrorCode::ElevatedPrivilege => SdkErrorCode::ElevatedPrivilege,
            LocalErrorCode::DaemonUnreachable | LocalErrorCode::ExistingOwner => {
                SdkErrorCode::DaemonUnreachable
            }
            LocalErrorCode::InvalidConfig => SdkErrorCode::InvalidConfig,
            LocalErrorCode::Io => SdkErrorCode::Io,
        };
        Self::new(code, value.message())
    }
}

impl From<DaemonError> for SdkError {
    fn from(value: DaemonError) -> Self {
        let code = match value.code() {
            zornmesh_daemon::DaemonErrorCode::LocalTrustUnsafe => SdkErrorCode::LocalTrustUnsafe,
            zornmesh_daemon::DaemonErrorCode::ElevatedPrivilege => SdkErrorCode::ElevatedPrivilege,
            zornmesh_daemon::DaemonErrorCode::DaemonUnreachable
            | zornmesh_daemon::DaemonErrorCode::ExistingOwner => SdkErrorCode::DaemonUnreachable,
            zornmesh_daemon::DaemonErrorCode::InvalidConfig => SdkErrorCode::InvalidConfig,
            zornmesh_daemon::DaemonErrorCode::Io => SdkErrorCode::Io,
        };
        Self::new(code, value.message())
    }
}

impl std::fmt::Display for SdkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for SdkError {}

fn daemon_unreachable_autospawn_disabled() -> SdkError {
    SdkError::new(
        SdkErrorCode::DaemonUnreachable,
        "daemon is unreachable and ZORN_NO_AUTOSPAWN=1 is set; run `zornmesh daemon` and retry",
    )
}

fn readiness_timeout(timeout: Duration, last_error: Option<LocalError>) -> SdkError {
    let detail = last_error
        .map(|error| format!(" last daemon state: {}", error.code().as_str()))
        .unwrap_or_else(|| " last daemon state: unreachable".to_owned());
    SdkError::new(
        SdkErrorCode::DaemonReadinessTimeout,
        format!(
            "daemon did not become ready within {} ms; retry or run `zornmesh daemon` explicitly;{detail}",
            timeout.as_millis()
        ),
    )
}

fn wait_for_readiness(options: &ConnectOptions, uid: u32) -> Result<(), SdkError> {
    let started_at = Instant::now();

    loop {
        match local::connect_trusted_socket(&options.socket_path, uid) {
            Ok(_stream) => return Ok(()),
            Err(error) if error.code() == LocalErrorCode::DaemonUnreachable => {
                let elapsed = started_at.elapsed();
                if elapsed >= options.connect_timeout {
                    return Err(readiness_timeout(options.connect_timeout, Some(error)));
                }

                let remaining = options.connect_timeout.saturating_sub(elapsed);
                thread::sleep(options.retry_delay.min(remaining));
            }
            Err(error) => return Err(SdkError::from(error)),
        }
    }
}
