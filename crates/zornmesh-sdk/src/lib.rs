#![doc = "Rust SDK entrypoints for zornmesh agents."]

pub const CRATE_BOUNDARY: &str = "zornmesh-sdk";

mod spawn;

use std::{
    io,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use zornmesh_core::{
    CoordinationOutcome, CoordinationOutcomeKind, CoordinationStage, DeliveryOutcome, Envelope,
    ErrorCategory, NackReasonCategory,
};
use zornmesh_daemon::DaemonError;
use zornmesh_proto::{
    ClientFrame, FrameStatus, ProtoError, ServerFrame, read_server_frame, write_client_frame,
};
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
    pub fn for_test_socket(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    pub fn publish(&self, envelope: &Envelope) -> SendResult {
        match self.open_stream() {
            Ok(mut stream) => {
                if let Err(error) = write_client_frame(
                    &mut stream,
                    &ClientFrame::Publish {
                        envelope: envelope.clone(),
                    },
                ) {
                    return SendResult::from_proto_error(error);
                }

                match read_server_frame(&mut stream) {
                    Ok(ServerFrame::SendResult(result)) => SendResult::from_frame(result),
                    Ok(ServerFrame::Delivery { .. } | ServerFrame::DeliveryOutcome(_)) => {
                        SendResult::rejected("E_PROTOCOL", "daemon returned delivery to publisher")
                    }
                    Err(error) => SendResult::from_proto_error(error),
                }
            }
            Err(error) if error.code() == SdkErrorCode::DaemonUnreachable => {
                SendResult::daemon_unreachable(error.to_string())
            }
            Err(error) => SendResult::rejected(error.code().as_str(), error.to_string()),
        }
    }

    pub fn subscribe(&self, pattern: impl Into<String>) -> Result<Subscription, SdkError> {
        let mut stream = self.open_stream()?;
        write_client_frame(
            &mut stream,
            &ClientFrame::Subscribe {
                pattern: pattern.into(),
            },
        )
        .map_err(SdkError::from)?;

        match read_server_frame(&mut stream).map_err(SdkError::from)? {
            ServerFrame::SendResult(result) if result.status() == FrameStatus::Accepted => {
                Ok(Subscription { stream })
            }
            ServerFrame::SendResult(result) => Err(SdkError::from_send_result(result)),
            ServerFrame::Delivery { .. } | ServerFrame::DeliveryOutcome(_) => Err(SdkError::new(
                SdkErrorCode::Protocol,
                "daemon returned delivery before subscription acceptance",
            )),
        }
    }

    fn open_stream(&self) -> Result<UnixStream, SdkError> {
        let uid = local::effective_uid().map_err(SdkError::from)?;
        local::connect_trusted_socket(&self.socket_path, uid).map_err(SdkError::from)
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
    SubjectValidation,
    SubscriptionCap,
    PayloadLimit,
    Protocol,
    PersistenceUnavailable,
    DeliveryValidation,
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
            Self::SubjectValidation => "E_SUBJECT_VALIDATION",
            Self::SubscriptionCap => "E_SUBSCRIPTION_CAP",
            Self::PayloadLimit => "E_PAYLOAD_LIMIT",
            Self::Protocol => "E_PROTOCOL",
            Self::PersistenceUnavailable => "E_PERSISTENCE_UNAVAILABLE",
            Self::DeliveryValidation => "E_DELIVERY_VALIDATION",
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

    pub const fn category(&self) -> ErrorCategory {
        error_category_for_sdk_code(self.code)
    }

    pub fn safe_details(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendStatus {
    Accepted,
    Rejected,
    DaemonUnreachable,
    ValidationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendResult {
    status: SendStatus,
    code: String,
    message: String,
    outcome: CoordinationOutcome,
    durable_outcome: Option<CoordinationOutcome>,
}

impl SendResult {
    fn from_frame(frame: zornmesh_proto::SendResultFrame) -> Self {
        let status = match frame.status() {
            FrameStatus::Accepted => SendStatus::Accepted,
            FrameStatus::Rejected => SendStatus::Rejected,
            FrameStatus::ValidationFailed => SendStatus::ValidationFailed,
        };
        Self {
            status,
            code: frame.code().to_owned(),
            message: frame.message().to_owned(),
            outcome: frame.outcome().clone(),
            durable_outcome: frame.durable_outcome().cloned(),
        }
    }

    fn daemon_unreachable(message: impl Into<String>) -> Self {
        Self {
            status: SendStatus::DaemonUnreachable,
            code: SdkErrorCode::DaemonUnreachable.as_str().to_owned(),
            message: message.into(),
            outcome: CoordinationOutcome::new(
                CoordinationOutcomeKind::Retryable,
                CoordinationStage::Transport,
                SdkErrorCode::DaemonUnreachable.as_str(),
                "daemon unreachable",
                true,
                false,
                0,
            ),
            durable_outcome: None,
        }
    }

    fn rejected(code: impl Into<String>, message: impl Into<String>) -> Self {
        let code = code.into();
        let message = message.into();
        Self {
            status: SendStatus::Rejected,
            outcome: CoordinationOutcome::new(
                CoordinationOutcomeKind::Rejected,
                CoordinationStage::Transport,
                code.clone(),
                message.clone(),
                false,
                true,
                0,
            ),
            code,
            message,
            durable_outcome: None,
        }
    }

    fn validation_failed(code: impl Into<String>, message: impl Into<String>) -> Self {
        let code = code.into();
        let message = message.into();
        Self {
            status: SendStatus::ValidationFailed,
            outcome: CoordinationOutcome::new(
                CoordinationOutcomeKind::Failed,
                CoordinationStage::Protocol,
                code.clone(),
                message.clone(),
                false,
                true,
                0,
            ),
            code,
            message,
            durable_outcome: None,
        }
    }

    fn from_proto_error(error: ProtoError) -> Self {
        match error.code() {
            "E_SUBJECT_VALIDATION" | "E_PAYLOAD_LIMIT" | "E_PROTOCOL" => {
                Self::validation_failed(error.code(), error.to_string())
            }
            code => Self::rejected(code, error.to_string()),
        }
    }

    pub const fn status(&self) -> SendStatus {
        self.status
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn outcome(&self) -> &CoordinationOutcome {
        &self.outcome
    }

    pub const fn durable_outcome(&self) -> Option<&CoordinationOutcome> {
        self.durable_outcome.as_ref()
    }

    pub const fn retryable(&self) -> bool {
        self.outcome.retryable()
    }

    pub fn error_category(&self) -> ErrorCategory {
        error_category_for_code(&self.code)
    }

    pub fn safe_details(&self) -> &str {
        &self.message
    }
}

#[derive(Debug)]
pub struct Subscription {
    stream: UnixStream,
}

impl Subscription {
    pub fn recv_delivery(&mut self, timeout: Duration) -> Result<Option<Delivery>, SdkError> {
        self.stream
            .set_read_timeout(Some(timeout))
            .map_err(|error| SdkError::new(SdkErrorCode::Io, error.to_string()))?;

        match read_server_frame(&mut self.stream) {
            Ok(ServerFrame::Delivery {
                delivery_id,
                envelope,
                attempt,
            }) => Ok(Some(Delivery {
                delivery_id,
                envelope,
                attempt,
            })),
            Ok(ServerFrame::DeliveryOutcome(result)) => Err(SdkError::from_delivery_outcome(result)),
            Ok(ServerFrame::SendResult(result)) => Err(SdkError::from_send_result(result)),
            Err(error)
                if matches!(
                    error.io_kind(),
                    Some(io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
                ) =>
            {
                Ok(None)
            }
            Err(error) => Err(SdkError::from(error)),
        }
    }

    pub fn ack(&mut self, delivery: &Delivery) -> Result<DeliveryOutcome, SdkError> {
        self.delivery_control(ClientFrame::Ack {
            delivery_id: delivery.delivery_id().to_owned(),
        })
    }

    pub fn nack(
        &mut self,
        delivery: &Delivery,
        reason: NackReasonCategory,
    ) -> Result<DeliveryOutcome, SdkError> {
        self.delivery_control(ClientFrame::Nack {
            delivery_id: delivery.delivery_id().to_owned(),
            reason,
        })
    }

    fn delivery_control(&mut self, frame: ClientFrame) -> Result<DeliveryOutcome, SdkError> {
        write_client_frame(&mut self.stream, &frame).map_err(SdkError::from)?;
        match read_server_frame(&mut self.stream).map_err(SdkError::from)? {
            ServerFrame::DeliveryOutcome(outcome) => Ok(outcome.outcome().clone()),
            ServerFrame::SendResult(result) => Err(SdkError::from_send_result(result)),
            ServerFrame::Delivery { .. } => Err(SdkError::new(
                SdkErrorCode::Protocol,
                "daemon returned delivery before ACK/NACK outcome",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    delivery_id: String,
    envelope: Envelope,
    attempt: u32,
}

impl Delivery {
    pub fn delivery_id(&self) -> &str {
        &self.delivery_id
    }

    pub const fn envelope(&self) -> &Envelope {
        &self.envelope
    }

    pub const fn attempt(&self) -> u32 {
        self.attempt
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

impl From<ProtoError> for SdkError {
    fn from(value: ProtoError) -> Self {
        let code = match value.code() {
            "E_SUBJECT_VALIDATION" => SdkErrorCode::SubjectValidation,
            "E_PAYLOAD_LIMIT" => SdkErrorCode::PayloadLimit,
            "E_DAEMON_IO" => SdkErrorCode::Io,
            _ => SdkErrorCode::Protocol,
        };
        Self::new(code, value.to_string())
    }
}

impl SdkError {
    fn from_send_result(value: zornmesh_proto::SendResultFrame) -> Self {
        let code = sdk_error_code_from_wire(value.code());
        Self::new(code, value.message())
    }

    fn from_delivery_outcome(value: zornmesh_proto::DeliveryOutcomeFrame) -> Self {
        let outcome = value.outcome();
        Self::new(sdk_error_code_from_wire(outcome.code()), outcome.message())
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

const fn error_category_for_sdk_code(code: SdkErrorCode) -> ErrorCategory {
    match code {
        SdkErrorCode::LocalTrustUnsafe
        | SdkErrorCode::InvalidConfig
        | SdkErrorCode::SubjectValidation
        | SdkErrorCode::DeliveryValidation => ErrorCategory::Validation,
        SdkErrorCode::ElevatedPrivilege => ErrorCategory::Authorization,
        SdkErrorCode::DaemonUnreachable => ErrorCategory::Reachability,
        SdkErrorCode::DaemonReadinessTimeout => ErrorCategory::Timeout,
        SdkErrorCode::PayloadLimit => ErrorCategory::PayloadLimit,
        SdkErrorCode::Protocol => ErrorCategory::Protocol,
        SdkErrorCode::PersistenceUnavailable => ErrorCategory::PersistenceUnavailable,
        SdkErrorCode::SubscriptionCap | SdkErrorCode::Io => ErrorCategory::Internal,
    }
}

fn error_category_for_code(code: &str) -> ErrorCategory {
    error_category_for_sdk_code(sdk_error_code_from_wire(code))
}

fn sdk_error_code_from_wire(code: &str) -> SdkErrorCode {
    match code {
        "E_LOCAL_TRUST_UNSAFE" => SdkErrorCode::LocalTrustUnsafe,
        "E_ELEVATED_PRIVILEGE" => SdkErrorCode::ElevatedPrivilege,
        "E_DAEMON_UNREACHABLE" => SdkErrorCode::DaemonUnreachable,
        "E_DAEMON_READINESS_TIMEOUT" => SdkErrorCode::DaemonReadinessTimeout,
        "E_INVALID_CONFIG" => SdkErrorCode::InvalidConfig,
        "E_SUBJECT_VALIDATION" => SdkErrorCode::SubjectValidation,
        "E_SUBSCRIPTION_CAP" => SdkErrorCode::SubscriptionCap,
        "E_PAYLOAD_LIMIT" => SdkErrorCode::PayloadLimit,
        "E_PROTOCOL" => SdkErrorCode::Protocol,
        "E_PERSISTENCE_UNAVAILABLE" => SdkErrorCode::PersistenceUnavailable,
        "E_DELIVERY_VALIDATION" => SdkErrorCode::DeliveryValidation,
        _ => SdkErrorCode::Io,
    }
}
