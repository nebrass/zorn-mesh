#![doc = "Durable store crate boundary for zornmesh storage work."]

use std::{
    collections::HashMap,
    fmt,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

pub const CRATE_BOUNDARY: &str = "zornmesh-store";
pub const MAX_SUBSCRIPTION_IDENTITY_BYTES: usize = 256;
pub const DURABLE_SUBSCRIPTION_LOG_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreBoundary;

impl StoreBoundary {
    pub const fn name(self) -> &'static str {
        CRATE_BOUNDARY
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableSubscriptionScope {
    consumer_agent: String,
    pattern: String,
}

impl DurableSubscriptionScope {
    pub fn new(consumer_agent: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self {
            consumer_agent: consumer_agent.into(),
            pattern: pattern.into(),
        }
    }

    pub fn consumer_agent(&self) -> &str {
        &self.consumer_agent
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableSubscriptionState {
    identity: String,
    scope: DurableSubscriptionScope,
    last_acked_sequence: u64,
    retry_count: u64,
    min_retained_sequence: u64,
}

impl DurableSubscriptionState {
    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub const fn scope(&self) -> &DurableSubscriptionScope {
        &self.scope
    }

    pub const fn last_acked_sequence(&self) -> u64 {
        self.last_acked_sequence
    }

    pub const fn retry_count(&self) -> u64 {
        self.retry_count
    }

    pub const fn min_retained_sequence(&self) -> u64 {
        self.min_retained_sequence
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeOutcome {
    Resumed {
        state: DurableSubscriptionState,
    },
    RetentionGap {
        requested_from: u64,
        min_retained: u64,
        remediation: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionStoreErrorCode {
    Validation,
    NotFound,
    Conflict,
    Io,
    Corrupt,
}

impl SubscriptionStoreErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "E_SUBSCRIPTION_VALIDATION",
            Self::NotFound => "E_SUBSCRIPTION_NOT_FOUND",
            Self::Conflict => "E_SUBSCRIPTION_CONFLICT",
            Self::Io => "E_SUBSCRIPTION_IO",
            Self::Corrupt => "E_SUBSCRIPTION_CORRUPT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionStoreError {
    code: SubscriptionStoreErrorCode,
    message: String,
}

impl SubscriptionStoreError {
    fn new(code: SubscriptionStoreErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> SubscriptionStoreErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SubscriptionStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for SubscriptionStoreError {}

pub trait DurableSubscriptionStore {
    fn create_subscription(
        &self,
        identity: impl Into<String>,
        scope: DurableSubscriptionScope,
    ) -> Result<DurableSubscriptionState, SubscriptionStoreError>;

    fn resume_subscription(
        &self,
        identity: &str,
        scope: DurableSubscriptionScope,
        requested_from_sequence: u64,
    ) -> Result<ResumeOutcome, SubscriptionStoreError>;

    fn record_ack(
        &self,
        identity: &str,
        sequence: u64,
    ) -> Result<DurableSubscriptionState, SubscriptionStoreError>;

    fn record_retry(
        &self,
        identity: &str,
    ) -> Result<DurableSubscriptionState, SubscriptionStoreError>;

    fn set_min_retained_sequence(
        &self,
        identity: &str,
        min_retained: u64,
    ) -> Result<DurableSubscriptionState, SubscriptionStoreError>;
}

#[derive(Debug)]
pub struct FileDurableStore {
    inner: Mutex<FileDurableInner>,
}

#[derive(Debug)]
struct FileDurableInner {
    path: PathBuf,
    subscriptions: HashMap<String, DurableSubscriptionState>,
}

impl FileDurableStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SubscriptionStoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    SubscriptionStoreError::new(
                        SubscriptionStoreErrorCode::Io,
                        format!("create durable store parent dir failed: {error}"),
                    )
                })?;
            }
        }
        let subscriptions = Self::replay(&path)?;
        Ok(Self {
            inner: Mutex::new(FileDurableInner {
                path,
                subscriptions,
            }),
        })
    }

    fn replay(
        path: &Path,
    ) -> Result<HashMap<String, DurableSubscriptionState>, SubscriptionStoreError> {
        let mut state: HashMap<String, DurableSubscriptionState> = HashMap::new();
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(state),
            Err(error) => {
                return Err(SubscriptionStoreError::new(
                    SubscriptionStoreErrorCode::Io,
                    format!("open durable log failed: {error}"),
                ));
            }
        };
        let reader = BufReader::new(file);
        for (line_number, line) in reader.lines().enumerate() {
            let line = line.map_err(|error| {
                SubscriptionStoreError::new(
                    SubscriptionStoreErrorCode::Io,
                    format!("read durable log line {line_number}: {error}"),
                )
            })?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let record = LogRecord::parse(trimmed).ok_or_else(|| {
                SubscriptionStoreError::new(
                    SubscriptionStoreErrorCode::Corrupt,
                    format!("durable log line {line_number} is corrupt"),
                )
            })?;
            apply_record(&mut state, record).map_err(|message| {
                SubscriptionStoreError::new(
                    SubscriptionStoreErrorCode::Corrupt,
                    format!("durable log line {line_number}: {message}"),
                )
            })?;
        }
        Ok(state)
    }

    fn append(&self, record: LogRecord) -> Result<(), SubscriptionStoreError> {
        let inner = self.inner.lock().expect("durable store mutex not poisoned");
        Self::write_record(&inner.path, record)
    }

    fn write_record(path: &Path, record: LogRecord) -> Result<(), SubscriptionStoreError> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| {
                SubscriptionStoreError::new(
                    SubscriptionStoreErrorCode::Io,
                    format!("open durable log for append failed: {error}"),
                )
            })?;
        let line = record.encode();
        file.write_all(line.as_bytes()).map_err(|error| {
            SubscriptionStoreError::new(
                SubscriptionStoreErrorCode::Io,
                format!("append durable log line failed: {error}"),
            )
        })?;
        file.write_all(b"\n").map_err(|error| {
            SubscriptionStoreError::new(
                SubscriptionStoreErrorCode::Io,
                format!("append durable log newline failed: {error}"),
            )
        })?;
        file.sync_all().map_err(|error| {
            SubscriptionStoreError::new(
                SubscriptionStoreErrorCode::Io,
                format!("fsync durable log failed: {error}"),
            )
        })?;
        Ok(())
    }
}

fn validate_identity(identity: &str) -> Result<(), SubscriptionStoreError> {
    if identity.trim().is_empty() {
        return Err(SubscriptionStoreError::new(
            SubscriptionStoreErrorCode::Validation,
            "subscription identity must not be empty",
        ));
    }
    if identity.len() > MAX_SUBSCRIPTION_IDENTITY_BYTES {
        return Err(SubscriptionStoreError::new(
            SubscriptionStoreErrorCode::Validation,
            format!(
                "subscription identity is {} bytes; maximum is {MAX_SUBSCRIPTION_IDENTITY_BYTES}",
                identity.len()
            ),
        ));
    }
    if identity.contains('\n') || identity.contains('|') {
        return Err(SubscriptionStoreError::new(
            SubscriptionStoreErrorCode::Validation,
            "subscription identity must not contain newline or pipe characters",
        ));
    }
    Ok(())
}

fn validate_scope(scope: &DurableSubscriptionScope) -> Result<(), SubscriptionStoreError> {
    if scope.consumer_agent.trim().is_empty() {
        return Err(SubscriptionStoreError::new(
            SubscriptionStoreErrorCode::Validation,
            "consumer agent must not be empty",
        ));
    }
    if scope.pattern.trim().is_empty() {
        return Err(SubscriptionStoreError::new(
            SubscriptionStoreErrorCode::Validation,
            "subscription pattern must not be empty",
        ));
    }
    if scope.consumer_agent.contains('|') || scope.pattern.contains('|') {
        return Err(SubscriptionStoreError::new(
            SubscriptionStoreErrorCode::Validation,
            "consumer agent and pattern must not contain pipe characters",
        ));
    }
    Ok(())
}

impl DurableSubscriptionStore for FileDurableStore {
    fn create_subscription(
        &self,
        identity: impl Into<String>,
        scope: DurableSubscriptionScope,
    ) -> Result<DurableSubscriptionState, SubscriptionStoreError> {
        let identity = identity.into();
        validate_identity(&identity)?;
        validate_scope(&scope)?;
        {
            let inner = self.inner.lock().expect("durable store mutex not poisoned");
            if inner.subscriptions.contains_key(&identity) {
                return Err(SubscriptionStoreError::new(
                    SubscriptionStoreErrorCode::Conflict,
                    format!("subscription identity '{identity}' already exists"),
                ));
            }
        }
        self.append(LogRecord::Create {
            identity: identity.clone(),
            consumer_agent: scope.consumer_agent.clone(),
            pattern: scope.pattern.clone(),
        })?;
        let state = DurableSubscriptionState {
            identity: identity.clone(),
            scope,
            last_acked_sequence: 0,
            retry_count: 0,
            min_retained_sequence: 0,
        };
        let mut inner = self.inner.lock().expect("durable store mutex not poisoned");
        inner.subscriptions.insert(identity, state.clone());
        Ok(state)
    }

    fn resume_subscription(
        &self,
        identity: &str,
        scope: DurableSubscriptionScope,
        requested_from_sequence: u64,
    ) -> Result<ResumeOutcome, SubscriptionStoreError> {
        validate_identity(identity)?;
        validate_scope(&scope)?;
        let inner = self.inner.lock().expect("durable store mutex not poisoned");
        let Some(state) = inner.subscriptions.get(identity).cloned() else {
            return Err(SubscriptionStoreError::new(
                SubscriptionStoreErrorCode::NotFound,
                format!("subscription identity '{identity}' is unknown"),
            ));
        };
        if state.scope != scope {
            return Err(SubscriptionStoreError::new(
                SubscriptionStoreErrorCode::Conflict,
                format!(
                    "subscription identity '{identity}' has a different scope than provided"
                ),
            ));
        }
        if requested_from_sequence > state.last_acked_sequence + 1 {
            return Ok(ResumeOutcome::RetentionGap {
                requested_from: requested_from_sequence,
                min_retained: state.min_retained_sequence,
                remediation: format!(
                    "retention covers sequences from {} but caller requested {}; reset subscription or accept retention gap",
                    state.min_retained_sequence, requested_from_sequence
                ),
            });
        }
        Ok(ResumeOutcome::Resumed { state })
    }

    fn record_ack(
        &self,
        identity: &str,
        sequence: u64,
    ) -> Result<DurableSubscriptionState, SubscriptionStoreError> {
        validate_identity(identity)?;
        {
            let inner = self.inner.lock().expect("durable store mutex not poisoned");
            let Some(state) = inner.subscriptions.get(identity) else {
                return Err(SubscriptionStoreError::new(
                    SubscriptionStoreErrorCode::NotFound,
                    format!("subscription identity '{identity}' is unknown"),
                ));
            };
            if sequence < state.last_acked_sequence {
                return Err(SubscriptionStoreError::new(
                    SubscriptionStoreErrorCode::Validation,
                    format!(
                        "ack sequence {sequence} is below recorded {} for '{identity}'",
                        state.last_acked_sequence
                    ),
                ));
            }
        }
        self.append(LogRecord::Ack {
            identity: identity.to_owned(),
            sequence,
        })?;
        let mut inner = self.inner.lock().expect("durable store mutex not poisoned");
        let state = inner.subscriptions.get_mut(identity).expect("present");
        state.last_acked_sequence = sequence;
        Ok(state.clone())
    }

    fn record_retry(
        &self,
        identity: &str,
    ) -> Result<DurableSubscriptionState, SubscriptionStoreError> {
        validate_identity(identity)?;
        {
            let inner = self.inner.lock().expect("durable store mutex not poisoned");
            if !inner.subscriptions.contains_key(identity) {
                return Err(SubscriptionStoreError::new(
                    SubscriptionStoreErrorCode::NotFound,
                    format!("subscription identity '{identity}' is unknown"),
                ));
            }
        }
        self.append(LogRecord::Retry {
            identity: identity.to_owned(),
        })?;
        let mut inner = self.inner.lock().expect("durable store mutex not poisoned");
        let state = inner.subscriptions.get_mut(identity).expect("present");
        state.retry_count += 1;
        Ok(state.clone())
    }

    fn set_min_retained_sequence(
        &self,
        identity: &str,
        min_retained: u64,
    ) -> Result<DurableSubscriptionState, SubscriptionStoreError> {
        validate_identity(identity)?;
        {
            let inner = self.inner.lock().expect("durable store mutex not poisoned");
            if !inner.subscriptions.contains_key(identity) {
                return Err(SubscriptionStoreError::new(
                    SubscriptionStoreErrorCode::NotFound,
                    format!("subscription identity '{identity}' is unknown"),
                ));
            }
        }
        self.append(LogRecord::Retention {
            identity: identity.to_owned(),
            min_retained,
        })?;
        let mut inner = self.inner.lock().expect("durable store mutex not poisoned");
        let state = inner.subscriptions.get_mut(identity).expect("present");
        state.min_retained_sequence = min_retained;
        Ok(state.clone())
    }
}

#[derive(Debug)]
enum LogRecord {
    Create {
        identity: String,
        consumer_agent: String,
        pattern: String,
    },
    Ack {
        identity: String,
        sequence: u64,
    },
    Retry {
        identity: String,
    },
    Retention {
        identity: String,
        min_retained: u64,
    },
}

impl LogRecord {
    fn encode(&self) -> String {
        match self {
            Self::Create {
                identity,
                consumer_agent,
                pattern,
            } => format!(
                "v{DURABLE_SUBSCRIPTION_LOG_VERSION}|create|{identity}|{consumer_agent}|{pattern}"
            ),
            Self::Ack { identity, sequence } => format!(
                "v{DURABLE_SUBSCRIPTION_LOG_VERSION}|ack|{identity}|{sequence}"
            ),
            Self::Retry { identity } => {
                format!("v{DURABLE_SUBSCRIPTION_LOG_VERSION}|retry|{identity}")
            }
            Self::Retention {
                identity,
                min_retained,
            } => format!(
                "v{DURABLE_SUBSCRIPTION_LOG_VERSION}|retention|{identity}|{min_retained}"
            ),
        }
    }

    fn parse(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split('|').collect();
        let prefix = parts.first()?;
        if !prefix.starts_with('v') {
            return None;
        }
        let version: u32 = prefix.trim_start_matches('v').parse().ok()?;
        if version != DURABLE_SUBSCRIPTION_LOG_VERSION {
            return None;
        }
        match *parts.get(1)? {
            "create" => Some(Self::Create {
                identity: parts.get(2)?.to_string(),
                consumer_agent: parts.get(3)?.to_string(),
                pattern: parts.get(4)?.to_string(),
            }),
            "ack" => Some(Self::Ack {
                identity: parts.get(2)?.to_string(),
                sequence: parts.get(3)?.parse().ok()?,
            }),
            "retry" => Some(Self::Retry {
                identity: parts.get(2)?.to_string(),
            }),
            "retention" => Some(Self::Retention {
                identity: parts.get(2)?.to_string(),
                min_retained: parts.get(3)?.parse().ok()?,
            }),
            _ => None,
        }
    }
}

fn apply_record(
    state: &mut HashMap<String, DurableSubscriptionState>,
    record: LogRecord,
) -> Result<(), String> {
    match record {
        LogRecord::Create {
            identity,
            consumer_agent,
            pattern,
        } => {
            if state.contains_key(&identity) {
                return Err(format!("duplicate create for '{identity}'"));
            }
            state.insert(
                identity.clone(),
                DurableSubscriptionState {
                    identity,
                    scope: DurableSubscriptionScope::new(consumer_agent, pattern),
                    last_acked_sequence: 0,
                    retry_count: 0,
                    min_retained_sequence: 0,
                },
            );
        }
        LogRecord::Ack { identity, sequence } => {
            let entry = state
                .get_mut(&identity)
                .ok_or_else(|| format!("ack before create for '{identity}'"))?;
            if sequence < entry.last_acked_sequence {
                return Err(format!(
                    "non-monotonic ack {sequence} < {} for '{identity}'",
                    entry.last_acked_sequence
                ));
            }
            entry.last_acked_sequence = sequence;
        }
        LogRecord::Retry { identity } => {
            let entry = state
                .get_mut(&identity)
                .ok_or_else(|| format!("retry before create for '{identity}'"))?;
            entry.retry_count += 1;
        }
        LogRecord::Retention {
            identity,
            min_retained,
        } => {
            let entry = state
                .get_mut(&identity)
                .ok_or_else(|| format!("retention before create for '{identity}'"))?;
            entry.min_retained_sequence = min_retained;
        }
    }
    Ok(())
}
