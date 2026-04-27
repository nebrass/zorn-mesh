#![doc = "Local zornmesh daemon lifecycle and rendezvous runtime."]

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use zornmesh_broker::{Broker, BrokerError, BrokerErrorCode};
use zornmesh_proto::{
    ClientFrame, FrameStatus, ProtoError, SendResultFrame, ServerFrame, read_client_frame,
    write_server_frame,
};
use zornmesh_rpc::local::{
    self, ENV_SHUTDOWN_BUDGET_MS, LocalError, LocalErrorCode, connect_trusted_socket,
};

pub const CRATE_BOUNDARY: &str = "zornmesh-daemon";
const DEFAULT_SHUTDOWN_BUDGET: Duration = Duration::from_secs(10);
const MAX_SHUTDOWN_BUDGET: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaemonBoundary;

impl DaemonBoundary {
    pub const fn name(self) -> &'static str {
        CRATE_BOUNDARY
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonState {
    Ready,
    Draining,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownOutcome {
    Clean,
    BudgetExceeded,
}

impl ShutdownOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::BudgetExceeded => "shutdown-budget-exceeded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownReport {
    pub state: DaemonState,
    pub outcome: ShutdownOutcome,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonErrorCode {
    ExistingOwner,
    LocalTrustUnsafe,
    ElevatedPrivilege,
    DaemonUnreachable,
    InvalidConfig,
    Io,
}

impl DaemonErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExistingOwner => "E_DAEMON_ALREADY_RUNNING",
            Self::LocalTrustUnsafe => "E_LOCAL_TRUST_UNSAFE",
            Self::ElevatedPrivilege => "E_ELEVATED_PRIVILEGE",
            Self::DaemonUnreachable => "E_DAEMON_UNREACHABLE",
            Self::InvalidConfig => "E_INVALID_CONFIG",
            Self::Io => "E_DAEMON_IO",
        }
    }
}

impl From<LocalErrorCode> for DaemonErrorCode {
    fn from(value: LocalErrorCode) -> Self {
        match value {
            LocalErrorCode::ExistingOwner => Self::ExistingOwner,
            LocalErrorCode::LocalTrustUnsafe => Self::LocalTrustUnsafe,
            LocalErrorCode::ElevatedPrivilege => Self::ElevatedPrivilege,
            LocalErrorCode::DaemonUnreachable => Self::DaemonUnreachable,
            LocalErrorCode::InvalidConfig => Self::InvalidConfig,
            LocalErrorCode::Io => Self::Io,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonError {
    code: DaemonErrorCode,
    message: String,
}

impl DaemonError {
    fn new(code: DaemonErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> DaemonErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<LocalError> for DaemonError {
    fn from(value: LocalError) -> Self {
        Self::new(value.code().into(), value.message())
    }
}

impl std::fmt::Display for DaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for DaemonError {}

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    socket_path: PathBuf,
    shutdown_budget: Duration,
    allow_elevated_for_tests: bool,
    effective_uid_for_tests: Option<u32>,
}

impl DaemonConfig {
    pub fn from_env() -> Result<Self, DaemonError> {
        let shutdown_budget = match std::env::var(ENV_SHUTDOWN_BUDGET_MS) {
            Ok(raw) => parse_shutdown_budget(&raw)?,
            Err(std::env::VarError::NotPresent) => DEFAULT_SHUTDOWN_BUDGET,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(DaemonError::new(
                    DaemonErrorCode::InvalidConfig,
                    "ZORN_SHUTDOWN_BUDGET_MS must be valid UTF-8",
                ));
            }
        };

        Ok(Self {
            socket_path: local::resolve_socket_path_from_env()?,
            shutdown_budget,
            allow_elevated_for_tests: false,
            effective_uid_for_tests: None,
        })
    }

    pub fn for_socket_path(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            shutdown_budget: DEFAULT_SHUTDOWN_BUDGET,
            allow_elevated_for_tests: false,
            effective_uid_for_tests: None,
        }
    }

    pub fn for_test(socket_path: impl Into<PathBuf>) -> Self {
        Self::for_socket_path(socket_path)
    }

    pub fn with_socket_path(mut self, socket_path: impl Into<PathBuf>) -> Self {
        self.socket_path = socket_path.into();
        self
    }

    pub fn with_shutdown_budget(mut self, shutdown_budget: Duration) -> Self {
        self.shutdown_budget = shutdown_budget.min(MAX_SHUTDOWN_BUDGET);
        self
    }

    pub fn allow_elevated_for_tests(mut self, allow: bool) -> Self {
        self.allow_elevated_for_tests = allow;
        self
    }

    pub fn with_effective_uid_for_tests(mut self, uid: u32) -> Self {
        self.effective_uid_for_tests = Some(uid);
        self
    }

    fn effective_uid(&self) -> Result<u32, DaemonError> {
        match self.effective_uid_for_tests {
            Some(uid) => Ok(uid),
            None => Ok(local::effective_uid()?),
        }
    }
}

fn parse_shutdown_budget(raw: &str) -> Result<Duration, DaemonError> {
    let millis = raw.parse::<u64>().map_err(|error| {
        DaemonError::new(
            DaemonErrorCode::InvalidConfig,
            format!("ZORN_SHUTDOWN_BUDGET_MS must be milliseconds: {error}"),
        )
    })?;
    Ok(Duration::from_millis(millis).min(MAX_SHUTDOWN_BUDGET))
}

#[derive(Debug)]
pub struct DaemonRuntime {
    socket_path: PathBuf,
    lock_path: PathBuf,
    _lock_file: File,
    listener: Option<UnixListener>,
    broker: Broker,
    state: DaemonState,
    readiness_line: String,
    shutdown_budget: Duration,
}

impl DaemonRuntime {
    pub fn start(config: DaemonConfig) -> Result<Self, DaemonError> {
        let uid = config.effective_uid()?;
        if uid == 0 && !config.allow_elevated_for_tests {
            return Err(DaemonError::new(
                DaemonErrorCode::ElevatedPrivilege,
                "local daemon must not run with elevated privileges; run it as the invoking user",
            ));
        }

        local::ensure_private_parent(&config.socket_path, uid)?;
        prepare_existing_socket(&config.socket_path, uid)?;
        let lock_path = lock_path_for(&config.socket_path);
        let lock_file = acquire_lock(&lock_path, &config.socket_path, uid)?;
        prepare_existing_socket(&config.socket_path, uid)?;

        let listener = UnixListener::bind(&config.socket_path).map_err(|error| {
            DaemonError::new(
                DaemonErrorCode::Io,
                format!("failed to bind local daemon socket: {error}"),
            )
        })?;
        fs::set_permissions(&config.socket_path, fs::Permissions::from_mode(0o600)).map_err(
            |error| {
                DaemonError::new(
                    DaemonErrorCode::Io,
                    format!("failed to secure local daemon socket: {error}"),
                )
            },
        )?;
        listener.set_nonblocking(true).map_err(|error| {
            DaemonError::new(
                DaemonErrorCode::Io,
                format!("failed to configure daemon listener: {error}"),
            )
        })?;
        local::validate_socket_trust(&config.socket_path, uid)?;

        Ok(Self {
            readiness_line: format!("zorn: state=ready socket={}", config.socket_path.display()),
            socket_path: config.socket_path,
            lock_path,
            _lock_file: lock_file,
            listener: Some(listener),
            broker: Broker::new(),
            state: DaemonState::Ready,
            shutdown_budget: config.shutdown_budget,
        })
    }

    pub const fn state(&self) -> DaemonState {
        self.state
    }

    pub fn readiness_line(&self) -> &str {
        &self.readiness_line
    }

    pub fn accept_once(&self) -> Result<bool, DaemonError> {
        let listener = self.listener.as_ref().ok_or_else(|| {
            DaemonError::new(
                DaemonErrorCode::DaemonUnreachable,
                "daemon is draining and no longer accepts new connections",
            )
        })?;

        match listener.accept() {
            Ok((stream, _addr)) => {
                let broker = self.broker.clone();
                thread::spawn(move || handle_client(stream, broker));
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(false),
            Err(error) => Err(DaemonError::new(
                DaemonErrorCode::Io,
                format!("failed to accept local daemon connection: {error}"),
            )),
        }
    }

    pub fn shutdown_with_in_flight(
        &mut self,
        in_flight: usize,
    ) -> Result<ShutdownReport, DaemonError> {
        self.state = DaemonState::Draining;
        self.listener = None;
        remove_if_exists(&self.socket_path)?;

        let outcome = if in_flight == 0 {
            ShutdownOutcome::Clean
        } else {
            let _budget = self.shutdown_budget;
            ShutdownOutcome::BudgetExceeded
        };
        let reason = match outcome {
            ShutdownOutcome::Clean => "clean shutdown".to_owned(),
            ShutdownOutcome::BudgetExceeded => {
                format!(
                    "shutdown-budget-exceeded after {} ms: {in_flight} work item(s) unfinished",
                    self.shutdown_budget.as_millis()
                )
            }
        };

        Ok(ShutdownReport {
            state: self.state,
            outcome,
            reason,
        })
    }
}

impl Drop for DaemonRuntime {
    fn drop(&mut self) {
        self.listener = None;
        let _ = fs::remove_file(&self.socket_path);
        let _ = fs::remove_file(&self.lock_path);
    }
}

pub fn run_foreground(config: DaemonConfig) -> Result<ShutdownReport, DaemonError> {
    let mut daemon = DaemonRuntime::start(config)?;
    println!("{}", daemon.readiness_line());

    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::signal::SIGTERM, Arc::clone(&shutdown))
        .map_err(|error| {
            DaemonError::new(
                DaemonErrorCode::Io,
                format!("failed to install SIGTERM handler: {error}"),
            )
        })?;
    signal_hook::flag::register(signal_hook::consts::signal::SIGINT, Arc::clone(&shutdown))
        .map_err(|error| {
            DaemonError::new(
                DaemonErrorCode::Io,
                format!("failed to install SIGINT handler: {error}"),
            )
        })?;

    while !shutdown.load(Ordering::Relaxed) {
        let _accepted = daemon.accept_once()?;
        thread::sleep(Duration::from_millis(10));
    }

    let report = daemon.shutdown_with_in_flight(0)?;
    println!("zorn: state=draining outcome={}", report.outcome.as_str());
    Ok(report)
}

fn handle_client(mut stream: UnixStream, broker: Broker) {
    match read_client_frame(&mut stream) {
        Ok(ClientFrame::Subscribe { pattern }) => handle_subscribe(stream, broker, pattern),
        Ok(ClientFrame::Publish { envelope }) => handle_publish(&mut stream, broker, envelope),
        Err(error) if is_connect_probe_close(&error) => {}
        Err(error) => {
            let _ = write_proto_error(&mut stream, &error);
        }
    }
}

fn handle_subscribe(mut stream: UnixStream, broker: Broker, pattern: String) {
    let (delivery_tx, delivery_rx) = mpsc::channel();
    let _subscription = match broker.subscribe(pattern, delivery_tx) {
        Ok(subscription) => subscription,
        Err(error) => {
            let _ = write_broker_error(&mut stream, &error);
            return;
        }
    };

    if write_result(
        &mut stream,
        FrameStatus::Accepted,
        "ACCEPTED",
        "subscription accepted",
    )
    .is_err()
    {
        return;
    }

    let _ = stream.set_nonblocking(true);
    loop {
        match delivery_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(delivery) => {
                let frame = ServerFrame::Delivery {
                    envelope: delivery.envelope().clone(),
                    attempt: delivery.attempt(),
                };
                if write_server_frame(&mut stream, &frame).is_err() {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if client_has_closed(&mut stream) {
            break;
        }
    }
}

fn handle_publish(stream: &mut UnixStream, broker: Broker, envelope: zornmesh_core::Envelope) {
    match broker.publish(envelope) {
        Ok(delivery_attempts) => {
            let _ = write_result(
                stream,
                FrameStatus::Accepted,
                "ACCEPTED",
                format!("accepted for routing; delivery_attempts={delivery_attempts}"),
            );
        }
        Err(error) => {
            let _ = write_broker_error(stream, &error);
        }
    }
}

fn write_proto_error(stream: &mut UnixStream, error: &ProtoError) -> Result<(), ProtoError> {
    write_result(
        stream,
        FrameStatus::ValidationFailed,
        error.code(),
        error.to_string(),
    )
}

fn write_broker_error(stream: &mut UnixStream, error: &BrokerError) -> Result<(), ProtoError> {
    let status = match error.code() {
        BrokerErrorCode::SubjectValidation => FrameStatus::ValidationFailed,
        BrokerErrorCode::SubscriptionCap => FrameStatus::Rejected,
    };
    write_result(stream, status, error.code().as_str(), error.message())
}

fn write_result(
    stream: &mut UnixStream,
    status: FrameStatus,
    code: impl Into<String>,
    message: impl Into<String>,
) -> Result<(), ProtoError> {
    write_server_frame(
        stream,
        &ServerFrame::SendResult(SendResultFrame::new(status, code, message)),
    )
}

fn is_connect_probe_close(error: &ProtoError) -> bool {
    matches!(error, ProtoError::Truncated("frame_length"))
}

fn client_has_closed(stream: &mut UnixStream) -> bool {
    let mut probe = [0_u8; 1];
    match stream.read(&mut probe) {
        Ok(0) => true,
        Ok(_) => false,
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => false,
        Err(_) => true,
    }
}

fn prepare_existing_socket(path: &Path, uid: u32) -> Result<(), DaemonError> {
    if !path.exists() {
        return Ok(());
    }

    local::validate_socket_trust(path, uid)?;
    if connect_trusted_socket(path, uid).is_ok() {
        return Err(DaemonError::new(
            DaemonErrorCode::ExistingOwner,
            "local daemon already owns the trusted socket",
        ));
    }

    remove_if_exists(path)
}

fn lock_path_for(socket_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", socket_path.display()))
}

fn acquire_lock(lock_path: &Path, socket_path: &Path, uid: u32) -> Result<File, DaemonError> {
    loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(mut file) => {
                fs::set_permissions(lock_path, fs::Permissions::from_mode(0o600)).map_err(
                    |error| {
                        DaemonError::new(
                            DaemonErrorCode::Io,
                            format!("failed to secure daemon ownership lock: {error}"),
                        )
                    },
                )?;
                writeln!(file, "{}", std::process::id()).map_err(|error| {
                    DaemonError::new(
                        DaemonErrorCode::Io,
                        format!("failed to write daemon ownership lock: {error}"),
                    )
                })?;
                return Ok(file);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                validate_lock_trust(lock_path, uid)?;
                if local::socket_accepts_connections(socket_path, uid) {
                    return Err(DaemonError::new(
                        DaemonErrorCode::ExistingOwner,
                        "local daemon already owns the trusted socket",
                    ));
                }
                if lock_pid_is_alive(lock_path)? {
                    return Err(DaemonError::new(
                        DaemonErrorCode::ExistingOwner,
                        "local daemon startup is already in progress for this user",
                    ));
                }
                remove_if_exists(lock_path)?;
            }
            Err(error) => {
                return Err(DaemonError::new(
                    DaemonErrorCode::Io,
                    format!("failed to acquire daemon ownership lock: {error}"),
                ));
            }
        }
    }
}

fn validate_lock_trust(lock_path: &Path, uid: u32) -> Result<(), DaemonError> {
    let metadata = fs::symlink_metadata(lock_path).map_err(|error| {
        DaemonError::new(
            DaemonErrorCode::Io,
            format!("failed to inspect daemon ownership lock: {error}"),
        )
    })?;

    if metadata.uid() != uid || metadata.permissions().mode() & 0o077 != 0 {
        return Err(DaemonError::new(
            DaemonErrorCode::LocalTrustUnsafe,
            "daemon ownership lock must be private to the current user",
        ));
    }

    Ok(())
}

fn lock_pid_is_alive(lock_path: &Path) -> Result<bool, DaemonError> {
    let raw = fs::read_to_string(lock_path).map_err(|error| {
        DaemonError::new(
            DaemonErrorCode::Io,
            format!("failed to read daemon ownership lock: {error}"),
        )
    })?;
    let pid = raw.trim().parse::<u32>().map_err(|error| {
        DaemonError::new(
            DaemonErrorCode::LocalTrustUnsafe,
            format!("daemon ownership lock has invalid pid: {error}"),
        )
    })?;
    Ok(PathBuf::from("/proc").join(pid.to_string()).exists())
}

fn remove_if_exists(path: &Path) -> Result<(), DaemonError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(DaemonError::new(
            DaemonErrorCode::Io,
            format!("failed to remove daemon runtime file: {error}"),
        )),
    }
}
