#![doc = "Durable store crate boundary for zornmesh storage work."]

use std::{
    collections::HashMap,
    fmt,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use zornmesh_core::{Envelope, REDACTION_MARKER};

pub const CRATE_BOUNDARY: &str = "zornmesh-store";
pub const MAX_SUBSCRIPTION_IDENTITY_BYTES: usize = 256;
pub const DURABLE_SUBSCRIPTION_LOG_VERSION: u32 = 1;
pub const EVIDENCE_STORE_SCHEMA_VERSION: u32 = 1;
pub const EVIDENCE_INDEX_CORRELATION_ID: &str = "idx_evidence_correlation_id";
pub const EVIDENCE_INDEX_TRACE_ID: &str = "idx_evidence_trace_id";
pub const EVIDENCE_INDEX_AGENT_ID: &str = "idx_evidence_agent_id";
pub const EVIDENCE_INDEX_SUBJECT: &str = "idx_evidence_subject";
pub const EVIDENCE_INDEX_DELIVERY_STATE: &str = "idx_evidence_delivery_state";
pub const EVIDENCE_INDEX_FAILURE_CATEGORY: &str = "idx_evidence_failure_category";
pub const EVIDENCE_INDEX_TIMESTAMP: &str = "idx_evidence_timestamp";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceStoreErrorCode {
    Validation,
    Io,
    Corrupt,
    FutureSchema,
    MigrationLocked,
}

impl EvidenceStoreErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "E_EVIDENCE_VALIDATION",
            Self::Io => "E_EVIDENCE_IO",
            Self::Corrupt => "E_EVIDENCE_CORRUPT",
            Self::FutureSchema => "E_EVIDENCE_FUTURE_SCHEMA",
            Self::MigrationLocked => "E_EVIDENCE_MIGRATION_LOCKED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceStoreError {
    code: EvidenceStoreErrorCode,
    message: String,
}

impl EvidenceStoreError {
    fn new(code: EvidenceStoreErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> EvidenceStoreErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for EvidenceStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for EvidenceStoreError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadLetterFailureCategory {
    NoEligibleRecipient,
    TtlExpired,
    RetryExhausted,
    ValidationTerminal,
    DeliveryFailed,
    Timeout,
    Unknown,
}

impl DeadLetterFailureCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoEligibleRecipient => "no_eligible_recipient",
            Self::TtlExpired => "ttl_expired",
            Self::RetryExhausted => "retry_exhausted",
            Self::ValidationTerminal => "validation_terminal",
            Self::DeliveryFailed => "delivery_failed",
            Self::Timeout => "timeout",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "no_eligible_recipient" => Some(Self::NoEligibleRecipient),
            "ttl_expired" => Some(Self::TtlExpired),
            "retry_exhausted" => Some(Self::RetryExhausted),
            "validation_terminal" => Some(Self::ValidationTerminal),
            "delivery_failed" => Some(Self::DeliveryFailed),
            "timeout" => Some(Self::Timeout),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceEnvelopeInput {
    envelope: Envelope,
    message_id: String,
    trace_id: String,
    delivery_state: String,
    target: Option<String>,
    parent_message_id: Option<String>,
}

impl EvidenceEnvelopeInput {
    pub fn new(
        envelope: Envelope,
        message_id: impl Into<String>,
        trace_id: impl Into<String>,
        delivery_state: impl Into<String>,
    ) -> Result<Self, EvidenceStoreError> {
        let message_id = message_id.into();
        let trace_id = trace_id.into();
        let delivery_state = delivery_state.into();
        validate_evidence_field("message ID", &message_id)?;
        validate_evidence_field("trace ID", &trace_id)?;
        validate_evidence_field("delivery state", &delivery_state)?;
        Ok(Self {
            envelope,
            message_id,
            trace_id,
            delivery_state,
            target: None,
            parent_message_id: None,
        })
    }

    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        let target = target.into();
        if !target.trim().is_empty() {
            self.target = Some(target);
        }
        self
    }

    pub fn with_parent_message_id(mut self, parent_message_id: impl Into<String>) -> Self {
        let parent_message_id = parent_message_id.into();
        if !parent_message_id.trim().is_empty() {
            self.parent_message_id = Some(parent_message_id);
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceStateTransitionInput {
    daemon_sequence: u64,
    message_id: String,
    actor: String,
    action: String,
    capability_or_subject: String,
    correlation_id: String,
    trace_id: String,
    state_from: String,
    state_to: String,
    outcome_details: String,
}

impl EvidenceStateTransitionInput {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        daemon_sequence: u64,
        message_id: impl Into<String>,
        actor: impl Into<String>,
        action: impl Into<String>,
        capability_or_subject: impl Into<String>,
        correlation_id: impl Into<String>,
        trace_id: impl Into<String>,
        state_from: impl Into<String>,
        state_to: impl Into<String>,
        outcome_details: impl Into<String>,
    ) -> Result<Self, EvidenceStoreError> {
        if daemon_sequence == 0 {
            return Err(EvidenceStoreError::new(
                EvidenceStoreErrorCode::Validation,
                "daemon sequence must be greater than zero",
            ));
        }
        let input = Self {
            daemon_sequence,
            message_id: message_id.into(),
            actor: actor.into(),
            action: action.into(),
            capability_or_subject: capability_or_subject.into(),
            correlation_id: correlation_id.into(),
            trace_id: trace_id.into(),
            state_from: state_from.into(),
            state_to: state_to.into(),
            outcome_details: outcome_details.into(),
        };
        validate_evidence_field("message ID", &input.message_id)?;
        validate_evidence_field("actor", &input.actor)?;
        validate_evidence_field("action", &input.action)?;
        validate_evidence_field("capability or subject", &input.capability_or_subject)?;
        validate_evidence_field("correlation ID", &input.correlation_id)?;
        validate_evidence_field("trace ID", &input.trace_id)?;
        validate_evidence_field("state from", &input.state_from)?;
        validate_evidence_field("state to", &input.state_to)?;
        validate_evidence_field("outcome details", &input.outcome_details)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceDeadLetterInput {
    envelope: Envelope,
    message_id: String,
    trace_id: String,
    terminal_state: String,
    failure_category: DeadLetterFailureCategory,
    safe_details: String,
    intended_target: Option<String>,
    attempt_count: u32,
    last_failure_category: DeadLetterFailureCategory,
    first_attempted_unix_ms: u64,
    last_attempted_unix_ms: u64,
    terminal_unix_ms: u64,
}

impl EvidenceDeadLetterInput {
    pub fn new(
        envelope: Envelope,
        message_id: impl Into<String>,
        trace_id: impl Into<String>,
        terminal_state: impl Into<String>,
        failure_category: DeadLetterFailureCategory,
        safe_details: impl Into<String>,
    ) -> Result<Self, EvidenceStoreError> {
        let message_id = message_id.into();
        let trace_id = trace_id.into();
        let terminal_state = terminal_state.into();
        let safe_details = safe_details.into();
        validate_evidence_field("message ID", &message_id)?;
        validate_evidence_field("trace ID", &trace_id)?;
        validate_evidence_field("terminal state", &terminal_state)?;
        validate_evidence_field("safe details", &safe_details)?;
        let timestamp = envelope.timestamp_unix_ms();
        Ok(Self {
            envelope,
            message_id,
            trace_id,
            terminal_state,
            failure_category,
            safe_details,
            intended_target: None,
            attempt_count: 1,
            last_failure_category: failure_category,
            first_attempted_unix_ms: timestamp,
            last_attempted_unix_ms: timestamp,
            terminal_unix_ms: timestamp,
        })
    }

    pub fn with_intended_target(mut self, target: impl Into<String>) -> Self {
        let target = target.into();
        if !target.trim().is_empty() {
            self.intended_target = Some(target);
        }
        self
    }

    pub const fn with_attempt_count(mut self, attempt_count: u32) -> Self {
        self.attempt_count = attempt_count;
        self
    }

    pub const fn with_last_failure_category(mut self, category: DeadLetterFailureCategory) -> Self {
        self.last_failure_category = category;
        self
    }

    pub const fn with_timing(
        mut self,
        first_attempted_unix_ms: u64,
        last_attempted_unix_ms: u64,
        terminal_unix_ms: u64,
    ) -> Self {
        self.first_attempted_unix_ms = first_attempted_unix_ms;
        self.last_attempted_unix_ms = last_attempted_unix_ms;
        self.terminal_unix_ms = terminal_unix_ms;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceEnvelopeRecord {
    daemon_sequence: u64,
    message_id: String,
    source_agent: String,
    target_or_subject: String,
    subject: String,
    timestamp_unix_ms: u64,
    correlation_id: String,
    trace_id: String,
    span_id: String,
    parent_message_id: Option<String>,
    delivery_state: String,
    payload_len: usize,
    payload_content_type: String,
}

impl EvidenceEnvelopeRecord {
    pub const fn daemon_sequence(&self) -> u64 {
        self.daemon_sequence
    }

    pub fn message_id(&self) -> &str {
        &self.message_id
    }

    pub fn source_agent(&self) -> &str {
        &self.source_agent
    }

    pub fn target_or_subject(&self) -> &str {
        &self.target_or_subject
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub const fn timestamp_unix_ms(&self) -> u64 {
        self.timestamp_unix_ms
    }

    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    pub fn span_id(&self) -> &str {
        &self.span_id
    }

    pub fn parent_message_id(&self) -> Option<&str> {
        self.parent_message_id.as_deref()
    }

    pub fn delivery_state(&self) -> &str {
        &self.delivery_state
    }

    pub const fn payload_len(&self) -> usize {
        self.payload_len
    }

    pub fn payload_content_type(&self) -> &str {
        &self.payload_content_type
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceAuditEntry {
    daemon_sequence: u64,
    message_id: String,
    previous_audit_hash: String,
    current_audit_hash: String,
    actor: String,
    action: String,
    capability_or_subject: String,
    correlation_id: String,
    trace_id: String,
    state_from: Option<String>,
    state_to: String,
    outcome_details: String,
}

impl EvidenceAuditEntry {
    pub const fn daemon_sequence(&self) -> u64 {
        self.daemon_sequence
    }

    pub fn message_id(&self) -> &str {
        &self.message_id
    }

    pub fn previous_audit_hash(&self) -> &str {
        &self.previous_audit_hash
    }

    pub fn current_audit_hash(&self) -> &str {
        &self.current_audit_hash
    }

    pub fn actor(&self) -> &str {
        &self.actor
    }

    pub fn action(&self) -> &str {
        &self.action
    }

    pub fn capability_or_subject(&self) -> &str {
        &self.capability_or_subject
    }

    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    pub fn state_from(&self) -> Option<&str> {
        self.state_from.as_deref()
    }

    pub fn state_to(&self) -> &str {
        &self.state_to
    }

    pub fn outcome_details(&self) -> &str {
        &self.outcome_details
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceDeadLetterRecord {
    daemon_sequence: u64,
    message_id: String,
    source_agent: String,
    intended_target: Option<String>,
    subject: String,
    correlation_id: String,
    trace_id: String,
    terminal_state: String,
    failure_category: DeadLetterFailureCategory,
    safe_details: String,
    attempt_count: u32,
    last_failure_category: DeadLetterFailureCategory,
    first_attempted_unix_ms: u64,
    last_attempted_unix_ms: u64,
    terminal_unix_ms: u64,
    payload_len: usize,
    payload_content_type: String,
}

impl EvidenceDeadLetterRecord {
    pub const fn daemon_sequence(&self) -> u64 {
        self.daemon_sequence
    }

    pub fn message_id(&self) -> &str {
        &self.message_id
    }

    pub fn source_agent(&self) -> &str {
        &self.source_agent
    }

    pub fn intended_target(&self) -> Option<&str> {
        self.intended_target.as_deref()
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    pub fn terminal_state(&self) -> &str {
        &self.terminal_state
    }

    pub const fn failure_category(&self) -> DeadLetterFailureCategory {
        self.failure_category
    }

    pub fn safe_details(&self) -> &str {
        &self.safe_details
    }

    pub const fn attempt_count(&self) -> u32 {
        self.attempt_count
    }

    pub const fn last_failure_category(&self) -> DeadLetterFailureCategory {
        self.last_failure_category
    }

    pub const fn first_attempted_unix_ms(&self) -> u64 {
        self.first_attempted_unix_ms
    }

    pub const fn last_attempted_unix_ms(&self) -> u64 {
        self.last_attempted_unix_ms
    }

    pub const fn terminal_unix_ms(&self) -> u64 {
        self.terminal_unix_ms
    }

    pub const fn payload_len(&self) -> usize {
        self.payload_len
    }

    pub fn payload_content_type(&self) -> &str {
        &self.payload_content_type
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceCommit {
    envelope: EvidenceEnvelopeRecord,
    audit_entry: EvidenceAuditEntry,
}

impl EvidenceCommit {
    pub const fn envelope(&self) -> &EvidenceEnvelopeRecord {
        &self.envelope
    }

    pub const fn audit_entry(&self) -> &EvidenceAuditEntry {
        &self.audit_entry
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceDeadLetterCommit {
    dead_letter: EvidenceDeadLetterRecord,
    audit_entry: EvidenceAuditEntry,
}

impl EvidenceDeadLetterCommit {
    pub const fn dead_letter(&self) -> &EvidenceDeadLetterRecord {
        &self.dead_letter
    }

    pub const fn audit_entry(&self) -> &EvidenceAuditEntry {
        &self.audit_entry
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvidenceQuery {
    correlation_id: Option<String>,
    trace_id: Option<String>,
    agent_id: Option<String>,
    subject: Option<String>,
    delivery_state: Option<String>,
    time_window: Option<(u64, u64)>,
}

impl EvidenceQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn correlation_id(mut self, value: impl Into<String>) -> Self {
        self.correlation_id = Some(value.into());
        self
    }

    pub fn trace_id(mut self, value: impl Into<String>) -> Self {
        self.trace_id = Some(value.into());
        self
    }

    pub fn agent_id(mut self, value: impl Into<String>) -> Self {
        self.agent_id = Some(value.into());
        self
    }

    pub fn subject(mut self, value: impl Into<String>) -> Self {
        self.subject = Some(value.into());
        self
    }

    pub fn delivery_state(mut self, value: impl Into<String>) -> Self {
        self.delivery_state = Some(value.into());
        self
    }

    pub const fn time_window(mut self, start_unix_ms: u64, end_unix_ms: u64) -> Self {
        self.time_window = Some((start_unix_ms, end_unix_ms));
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeadLetterQuery {
    correlation_id: Option<String>,
    trace_id: Option<String>,
    agent_id: Option<String>,
    subject: Option<String>,
    failure_category: Option<DeadLetterFailureCategory>,
    time_window: Option<(u64, u64)>,
}

impl DeadLetterQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn correlation_id(mut self, value: impl Into<String>) -> Self {
        self.correlation_id = Some(value.into());
        self
    }

    pub fn trace_id(mut self, value: impl Into<String>) -> Self {
        self.trace_id = Some(value.into());
        self
    }

    pub fn agent_id(mut self, value: impl Into<String>) -> Self {
        self.agent_id = Some(value.into());
        self
    }

    pub fn subject(mut self, value: impl Into<String>) -> Self {
        self.subject = Some(value.into());
        self
    }

    pub const fn failure_category(mut self, value: DeadLetterFailureCategory) -> Self {
        self.failure_category = Some(value);
        self
    }

    pub const fn time_window(mut self, start_unix_ms: u64, end_unix_ms: u64) -> Self {
        self.time_window = Some((start_unix_ms, end_unix_ms));
        self
    }
}

pub trait EvidenceStore {
    fn persist_accepted_envelope(
        &self,
        input: EvidenceEnvelopeInput,
    ) -> Result<EvidenceCommit, EvidenceStoreError>;

    fn persist_state_transition(
        &self,
        input: EvidenceStateTransitionInput,
    ) -> Result<EvidenceAuditEntry, EvidenceStoreError>;

    fn persist_dead_letter(
        &self,
        input: EvidenceDeadLetterInput,
    ) -> Result<EvidenceDeadLetterCommit, EvidenceStoreError>;

    fn query_envelopes(&self, query: EvidenceQuery) -> Vec<EvidenceEnvelopeRecord>;

    fn query_dead_letters(&self, query: DeadLetterQuery) -> Vec<EvidenceDeadLetterRecord>;

    fn get_envelope(
        &self,
        message_id: &str,
    ) -> Result<Option<EvidenceEnvelopeRecord>, EvidenceStoreError>;

    fn audit_entries(&self) -> Vec<EvidenceAuditEntry>;

    fn next_daemon_sequence(&self) -> u64;

    fn index_names(&self) -> Vec<&'static str>;
}

#[derive(Debug, Clone)]
pub struct FileEvidenceStore {
    inner: Arc<Mutex<FileEvidenceInner>>,
}

#[derive(Debug)]
struct FileEvidenceInner {
    path: PathBuf,
    envelopes: Vec<EvidenceEnvelopeRecord>,
    message_index: HashMap<String, usize>,
    dead_letters: Vec<EvidenceDeadLetterRecord>,
    dead_letter_index: HashMap<String, usize>,
    audit_entries: Vec<EvidenceAuditEntry>,
    next_daemon_sequence: u64,
    last_audit_hash: String,
}

impl FileEvidenceStore {
    pub fn open_evidence(path: impl AsRef<Path>) -> Result<Self, EvidenceStoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|error| {
                EvidenceStoreError::new(
                    EvidenceStoreErrorCode::Io,
                    format!("create evidence store parent dir failed: {error}"),
                )
            })?;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).map_err(
                |error| {
                    EvidenceStoreError::new(
                        EvidenceStoreErrorCode::Io,
                        format!("secure evidence store parent dir failed: {error}"),
                    )
                },
            )?;
        }
        if Self::migration_lock_path(&path).exists() {
            return Err(EvidenceStoreError::new(
                EvidenceStoreErrorCode::MigrationLocked,
                "evidence store migration lock is held by another worker",
            ));
        }

        let should_write_header = match std::fs::metadata(&path) {
            Ok(metadata) => metadata.len() == 0,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(error) => {
                return Err(EvidenceStoreError::new(
                    EvidenceStoreErrorCode::Io,
                    format!("stat evidence store failed: {error}"),
                ));
            }
        };

        let inner = Self::replay(&path)?;
        if should_write_header {
            write_evidence_line(&path, &EvidenceLogRecord::Schema.encode())?;
        }

        Ok(Self {
            inner: Arc::new(Mutex::new(FileEvidenceInner { path, ..inner })),
        })
    }

    pub fn migration_lock_path(path: &Path) -> PathBuf {
        PathBuf::from(format!("{}.migration.lock", path.display()))
    }

    fn replay(path: &Path) -> Result<FileEvidenceInner, EvidenceStoreError> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(FileEvidenceInner::empty(path.to_path_buf()));
            }
            Err(error) => {
                return Err(EvidenceStoreError::new(
                    EvidenceStoreErrorCode::Io,
                    format!("open evidence store failed: {error}"),
                ));
            }
        };

        let mut inner = FileEvidenceInner::empty(path.to_path_buf());
        let reader = BufReader::new(file);
        for (line_number, line) in reader.lines().enumerate() {
            let line = line.map_err(|error| {
                EvidenceStoreError::new(
                    EvidenceStoreErrorCode::Io,
                    format!("read evidence line {line_number}: {error}"),
                )
            })?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let record = EvidenceLogRecord::parse(trimmed).ok_or_else(|| {
                EvidenceStoreError::new(
                    EvidenceStoreErrorCode::Corrupt,
                    format!("evidence line {line_number} is corrupt"),
                )
            })?;
            if matches!(record, EvidenceLogRecord::FutureSchema) {
                return Err(EvidenceStoreError::new(
                    EvidenceStoreErrorCode::FutureSchema,
                    format!("evidence line {line_number} uses a future schema version"),
                ));
            }
            inner.apply(record).map_err(|message| {
                EvidenceStoreError::new(
                    EvidenceStoreErrorCode::Corrupt,
                    format!("evidence line {line_number}: {message}"),
                )
            })?;
        }
        Ok(inner)
    }
}

impl FileEvidenceInner {
    fn empty(path: PathBuf) -> Self {
        Self {
            path,
            envelopes: Vec::new(),
            message_index: HashMap::new(),
            dead_letters: Vec::new(),
            dead_letter_index: HashMap::new(),
            audit_entries: Vec::new(),
            next_daemon_sequence: 1,
            last_audit_hash: "0".to_owned(),
        }
    }

    fn apply(&mut self, record: EvidenceLogRecord) -> Result<(), String> {
        match record {
            EvidenceLogRecord::Schema => Ok(()),
            EvidenceLogRecord::FutureSchema => {
                Err("future schema marker reached replay".to_owned())
            }
            EvidenceLogRecord::Accepted { envelope, audit } => {
                let envelope = *envelope;
                let audit = *audit;
                if envelope.daemon_sequence != self.next_daemon_sequence {
                    return Err(format!(
                        "daemon sequence {} does not match expected {}",
                        envelope.daemon_sequence, self.next_daemon_sequence
                    ));
                }
                if self.message_index.contains_key(&envelope.message_id) {
                    return Err(format!("duplicate message ID '{}'", envelope.message_id));
                }
                validate_audit_hash(&self.last_audit_hash, &audit)?;
                self.message_index
                    .insert(envelope.message_id.clone(), self.envelopes.len());
                self.envelopes.push(envelope);
                self.last_audit_hash = audit.current_audit_hash.clone();
                self.audit_entries.push(audit);
                self.next_daemon_sequence += 1;
                Ok(())
            }
            EvidenceLogRecord::Transition { audit } => {
                let audit = *audit;
                let Some(index) = self.message_index.get(audit.message_id()).copied() else {
                    return Err(format!(
                        "state transition references unknown message ID '{}'",
                        audit.message_id()
                    ));
                };
                validate_audit_hash(&self.last_audit_hash, &audit)?;
                self.envelopes[index].delivery_state = audit.state_to.clone();
                self.last_audit_hash = audit.current_audit_hash.clone();
                self.audit_entries.push(audit);
                Ok(())
            }
            EvidenceLogRecord::DeadLetter { dead_letter, audit } => {
                let dead_letter = *dead_letter;
                let audit = *audit;
                let Some(index) = self.message_index.get(dead_letter.message_id()).copied() else {
                    return Err(format!(
                        "dead letter references unknown message ID '{}'",
                        dead_letter.message_id()
                    ));
                };
                if self
                    .dead_letter_index
                    .contains_key(dead_letter.message_id())
                {
                    return Err(format!(
                        "duplicate dead letter for message ID '{}'",
                        dead_letter.message_id()
                    ));
                }
                if self.envelopes[index].daemon_sequence != dead_letter.daemon_sequence {
                    return Err(format!(
                        "dead letter daemon sequence {} does not match message ID '{}'",
                        dead_letter.daemon_sequence,
                        dead_letter.message_id()
                    ));
                }
                validate_audit_hash(&self.last_audit_hash, &audit)?;
                self.envelopes[index].delivery_state = dead_letter.terminal_state.clone();
                let dead_letter_index = self.dead_letters.len();
                self.dead_letter_index
                    .insert(dead_letter.message_id.clone(), dead_letter_index);
                self.dead_letters.push(dead_letter);
                self.last_audit_hash = audit.current_audit_hash.clone();
                self.audit_entries.push(audit);
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionPolicy {
    max_age_ms: Option<u64>,
    max_envelope_count: Option<usize>,
}

impl RetentionPolicy {
    pub fn new(
        max_age_ms: Option<u64>,
        max_envelope_count: Option<usize>,
    ) -> Result<Self, EvidenceStoreError> {
        if let Some(age) = max_age_ms
            && age == 0
        {
            return Err(EvidenceStoreError::new(
                EvidenceStoreErrorCode::Validation,
                "retention max-age-ms must be greater than zero",
            ));
        }
        if let Some(count) = max_envelope_count
            && count == 0
        {
            return Err(EvidenceStoreError::new(
                EvidenceStoreErrorCode::Validation,
                "retention max-count must be greater than zero",
            ));
        }
        if max_age_ms.is_none() && max_envelope_count.is_none() {
            return Err(EvidenceStoreError::new(
                EvidenceStoreErrorCode::Validation,
                "retention policy requires at least one of max-age-ms or max-count",
            ));
        }
        Ok(Self {
            max_age_ms,
            max_envelope_count,
        })
    }

    pub const fn max_age_ms(&self) -> Option<u64> {
        self.max_age_ms
    }

    pub const fn max_envelope_count(&self) -> Option<usize> {
        self.max_envelope_count
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionCheckpoint {
    pub sequence_start: u64,
    pub sequence_end: u64,
    pub prior_audit_hash: String,
    pub last_audit_hash: String,
    pub purge_reason: &'static str,
    pub purged_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionReport {
    pub purgeable_envelope_ids: Vec<String>,
    pub purgeable_dead_letter_ids: Vec<String>,
    pub retained_envelope_count: usize,
    pub retained_dead_letter_count: usize,
    pub retention_checkpoint: Option<RetentionCheckpoint>,
    pub now_unix_ms: u64,
}

impl FileEvidenceStore {
    pub fn plan_retention(&self, policy: &RetentionPolicy, now_unix_ms: u64) -> RetentionReport {
        let inner = self
            .inner
            .lock()
            .expect("evidence store mutex not poisoned");

        let age_threshold = policy
            .max_age_ms
            .map(|age| now_unix_ms.saturating_sub(age));

        let mut envelopes_sorted: Vec<&EvidenceEnvelopeRecord> = inner.envelopes.iter().collect();
        envelopes_sorted.sort_by_key(|record| record.daemon_sequence);

        let mut purgeable_ids: Vec<String> = Vec::new();
        for record in &envelopes_sorted {
            if let Some(threshold) = age_threshold
                && record.timestamp_unix_ms < threshold
            {
                purgeable_ids.push(record.message_id.clone());
            }
        }
        if let Some(max_count) = policy.max_envelope_count
            && envelopes_sorted.len() > max_count
        {
            let extra = envelopes_sorted.len() - max_count;
            for record in envelopes_sorted.iter().take(extra) {
                if !purgeable_ids.contains(&record.message_id) {
                    purgeable_ids.push(record.message_id.clone());
                }
            }
        }

        let purgeable_set: std::collections::HashSet<String> =
            purgeable_ids.iter().cloned().collect();

        let mut purgeable_dead_letters: Vec<String> = Vec::new();
        for record in &inner.dead_letters {
            if let Some(threshold) = age_threshold
                && record.terminal_unix_ms < threshold
            {
                purgeable_dead_letters.push(record.message_id.clone());
            }
        }

        let mut affected_audit: Vec<&EvidenceAuditEntry> = inner
            .audit_entries
            .iter()
            .filter(|audit| purgeable_set.contains(&audit.message_id))
            .collect();
        affected_audit.sort_by_key(|audit| audit.daemon_sequence);

        let retention_checkpoint = if affected_audit.is_empty() {
            None
        } else {
            let first = affected_audit.first().expect("non-empty");
            let last = affected_audit.last().expect("non-empty");
            Some(RetentionCheckpoint {
                sequence_start: first.daemon_sequence,
                sequence_end: last.daemon_sequence,
                prior_audit_hash: first.previous_audit_hash.clone(),
                last_audit_hash: last.current_audit_hash.clone(),
                purge_reason: if policy.max_age_ms.is_some() {
                    "max_age_exceeded"
                } else {
                    "max_count_exceeded"
                },
                purged_count: affected_audit.len(),
            })
        };

        let retained_envelope_count = inner
            .envelopes
            .iter()
            .filter(|record| !purgeable_set.contains(&record.message_id))
            .count();
        let purgeable_dead_letter_set: std::collections::HashSet<String> =
            purgeable_dead_letters.iter().cloned().collect();
        let retained_dead_letter_count = inner
            .dead_letters
            .iter()
            .filter(|record| !purgeable_dead_letter_set.contains(&record.message_id))
            .count();

        RetentionReport {
            purgeable_envelope_ids: purgeable_ids,
            purgeable_dead_letter_ids: purgeable_dead_letters,
            retained_envelope_count,
            retained_dead_letter_count,
            retention_checkpoint,
            now_unix_ms,
        }
    }
}

impl EvidenceStore for FileEvidenceStore {
    fn persist_accepted_envelope(
        &self,
        input: EvidenceEnvelopeInput,
    ) -> Result<EvidenceCommit, EvidenceStoreError> {
        let mut inner = self
            .inner
            .lock()
            .expect("evidence store mutex not poisoned");
        if inner.message_index.contains_key(&input.message_id) {
            return Err(EvidenceStoreError::new(
                EvidenceStoreErrorCode::Validation,
                format!("message ID '{}' already exists", input.message_id),
            ));
        }
        let daemon_sequence = inner.next_daemon_sequence;
        let payload_metadata = input.envelope.payload_metadata();
        let envelope = EvidenceEnvelopeRecord {
            daemon_sequence,
            message_id: input.message_id.clone(),
            source_agent: input.envelope.source_agent().to_owned(),
            target_or_subject: input
                .target
                .clone()
                .unwrap_or_else(|| input.envelope.subject().to_owned()),
            subject: input.envelope.subject().to_owned(),
            timestamp_unix_ms: input.envelope.timestamp_unix_ms(),
            correlation_id: input.envelope.correlation_id().to_owned(),
            trace_id: input.trace_id.clone(),
            span_id: input.envelope.trace_context().span_id().to_owned(),
            parent_message_id: input.parent_message_id.clone(),
            delivery_state: input.delivery_state.clone(),
            payload_len: payload_metadata.payload_len(),
            payload_content_type: REDACTION_MARKER.to_owned(),
        };
        let audit = build_audit_entry(AuditBuildInput {
            previous_hash: &inner.last_audit_hash,
            daemon_sequence,
            message_id: &input.message_id,
            actor: input.envelope.source_agent(),
            action: "accepted_envelope",
            capability_or_subject: input.envelope.subject(),
            correlation_id: input.envelope.correlation_id(),
            trace_id: &input.trace_id,
            state_from: None,
            state_to: &input.delivery_state,
            outcome_details: "accepted for durable processing",
        });
        let record = EvidenceLogRecord::Accepted {
            envelope: Box::new(envelope.clone()),
            audit: Box::new(audit.clone()),
        };
        write_evidence_line(&inner.path, &record.encode())?;
        let envelope_index = inner.envelopes.len();
        inner
            .message_index
            .insert(input.message_id.clone(), envelope_index);
        inner.envelopes.push(envelope.clone());
        inner.last_audit_hash = audit.current_audit_hash.clone();
        inner.audit_entries.push(audit.clone());
        inner.next_daemon_sequence += 1;
        Ok(EvidenceCommit {
            envelope,
            audit_entry: audit,
        })
    }

    fn persist_state_transition(
        &self,
        input: EvidenceStateTransitionInput,
    ) -> Result<EvidenceAuditEntry, EvidenceStoreError> {
        let mut inner = self
            .inner
            .lock()
            .expect("evidence store mutex not poisoned");
        let Some(envelope_index) = inner.message_index.get(&input.message_id).copied() else {
            return Err(EvidenceStoreError::new(
                EvidenceStoreErrorCode::Validation,
                format!("message ID '{}' is unknown", input.message_id),
            ));
        };
        if inner.envelopes[envelope_index].daemon_sequence != input.daemon_sequence {
            return Err(EvidenceStoreError::new(
                EvidenceStoreErrorCode::Validation,
                format!(
                    "daemon sequence {} does not match message ID '{}'",
                    input.daemon_sequence, input.message_id
                ),
            ));
        }
        let audit = build_audit_entry(AuditBuildInput {
            previous_hash: &inner.last_audit_hash,
            daemon_sequence: input.daemon_sequence,
            message_id: &input.message_id,
            actor: &input.actor,
            action: &input.action,
            capability_or_subject: &input.capability_or_subject,
            correlation_id: &input.correlation_id,
            trace_id: &input.trace_id,
            state_from: Some(&input.state_from),
            state_to: &input.state_to,
            outcome_details: &input.outcome_details,
        });
        let record = EvidenceLogRecord::Transition {
            audit: Box::new(audit.clone()),
        };
        write_evidence_line(&inner.path, &record.encode())?;
        inner.envelopes[envelope_index].delivery_state = audit.state_to.clone();
        inner.last_audit_hash = audit.current_audit_hash.clone();
        inner.audit_entries.push(audit.clone());
        Ok(audit)
    }

    fn persist_dead_letter(
        &self,
        input: EvidenceDeadLetterInput,
    ) -> Result<EvidenceDeadLetterCommit, EvidenceStoreError> {
        if input.first_attempted_unix_ms > input.last_attempted_unix_ms
            || input.last_attempted_unix_ms > input.terminal_unix_ms
        {
            return Err(EvidenceStoreError::new(
                EvidenceStoreErrorCode::Validation,
                "dead letter timing must be ordered first_attempted <= last_attempted <= terminal",
            ));
        }

        let mut inner = self
            .inner
            .lock()
            .expect("evidence store mutex not poisoned");
        let Some(envelope_index) = inner.message_index.get(&input.message_id).copied() else {
            return Err(EvidenceStoreError::new(
                EvidenceStoreErrorCode::Validation,
                format!("message ID '{}' is unknown", input.message_id),
            ));
        };
        if inner.dead_letter_index.contains_key(&input.message_id) {
            return Err(EvidenceStoreError::new(
                EvidenceStoreErrorCode::Validation,
                format!(
                    "message ID '{}' already has a dead letter",
                    input.message_id
                ),
            ));
        }

        let envelope_record = inner.envelopes[envelope_index].clone();
        if input.envelope.source_agent() != envelope_record.source_agent()
            || input.envelope.subject() != envelope_record.subject()
            || input.envelope.correlation_id() != envelope_record.correlation_id()
        {
            return Err(EvidenceStoreError::new(
                EvidenceStoreErrorCode::Validation,
                format!(
                    "dead letter envelope metadata does not match message ID '{}'",
                    input.message_id
                ),
            ));
        }
        let state_from = envelope_record.delivery_state().to_owned();
        let payload_metadata = input.envelope.payload_metadata();
        let safe_details = redact_sensitive(&input.safe_details);
        let dead_letter = EvidenceDeadLetterRecord {
            daemon_sequence: envelope_record.daemon_sequence(),
            message_id: input.message_id.clone(),
            source_agent: input.envelope.source_agent().to_owned(),
            intended_target: input.intended_target.clone(),
            subject: input.envelope.subject().to_owned(),
            correlation_id: input.envelope.correlation_id().to_owned(),
            trace_id: input.trace_id.clone(),
            terminal_state: input.terminal_state.clone(),
            failure_category: input.failure_category,
            safe_details: safe_details.clone(),
            attempt_count: input.attempt_count,
            last_failure_category: input.last_failure_category,
            first_attempted_unix_ms: input.first_attempted_unix_ms,
            last_attempted_unix_ms: input.last_attempted_unix_ms,
            terminal_unix_ms: input.terminal_unix_ms,
            payload_len: payload_metadata.payload_len(),
            payload_content_type: REDACTION_MARKER.to_owned(),
        };
        let audit = build_audit_entry(AuditBuildInput {
            previous_hash: &inner.last_audit_hash,
            daemon_sequence: envelope_record.daemon_sequence(),
            message_id: &input.message_id,
            actor: input.envelope.source_agent(),
            action: "dead_lettered",
            capability_or_subject: input.envelope.subject(),
            correlation_id: input.envelope.correlation_id(),
            trace_id: &input.trace_id,
            state_from: Some(&state_from),
            state_to: &input.terminal_state,
            outcome_details: &safe_details,
        });
        let record = EvidenceLogRecord::DeadLetter {
            dead_letter: Box::new(dead_letter.clone()),
            audit: Box::new(audit.clone()),
        };
        write_evidence_line(&inner.path, &record.encode())?;
        inner.envelopes[envelope_index].delivery_state = audit.state_to.clone();
        let dead_letter_index = inner.dead_letters.len();
        inner
            .dead_letter_index
            .insert(input.message_id.clone(), dead_letter_index);
        inner.dead_letters.push(dead_letter.clone());
        inner.last_audit_hash = audit.current_audit_hash.clone();
        inner.audit_entries.push(audit.clone());
        Ok(EvidenceDeadLetterCommit {
            dead_letter,
            audit_entry: audit,
        })
    }

    fn query_envelopes(&self, query: EvidenceQuery) -> Vec<EvidenceEnvelopeRecord> {
        let inner = self
            .inner
            .lock()
            .expect("evidence store mutex not poisoned");
        inner
            .envelopes
            .iter()
            .filter(|record| evidence_matches_query(record, &query))
            .cloned()
            .collect()
    }

    fn query_dead_letters(&self, query: DeadLetterQuery) -> Vec<EvidenceDeadLetterRecord> {
        let inner = self
            .inner
            .lock()
            .expect("evidence store mutex not poisoned");
        inner
            .dead_letters
            .iter()
            .filter(|record| dead_letter_matches_query(record, &query))
            .cloned()
            .collect()
    }

    fn get_envelope(
        &self,
        message_id: &str,
    ) -> Result<Option<EvidenceEnvelopeRecord>, EvidenceStoreError> {
        validate_evidence_field("message ID", message_id)?;
        let inner = self
            .inner
            .lock()
            .expect("evidence store mutex not poisoned");
        Ok(inner
            .message_index
            .get(message_id)
            .map(|index| inner.envelopes[*index].clone()))
    }

    fn audit_entries(&self) -> Vec<EvidenceAuditEntry> {
        self.inner
            .lock()
            .expect("evidence store mutex not poisoned")
            .audit_entries
            .clone()
    }

    fn next_daemon_sequence(&self) -> u64 {
        self.inner
            .lock()
            .expect("evidence store mutex not poisoned")
            .next_daemon_sequence
    }

    fn index_names(&self) -> Vec<&'static str> {
        vec![
            EVIDENCE_INDEX_CORRELATION_ID,
            EVIDENCE_INDEX_TRACE_ID,
            EVIDENCE_INDEX_AGENT_ID,
            EVIDENCE_INDEX_SUBJECT,
            EVIDENCE_INDEX_DELIVERY_STATE,
            EVIDENCE_INDEX_FAILURE_CATEGORY,
            EVIDENCE_INDEX_TIMESTAMP,
        ]
    }
}

fn dead_letter_matches_query(record: &EvidenceDeadLetterRecord, query: &DeadLetterQuery) -> bool {
    if query
        .correlation_id
        .as_deref()
        .is_some_and(|value| record.correlation_id() != value)
    {
        return false;
    }
    if query
        .trace_id
        .as_deref()
        .is_some_and(|value| record.trace_id() != value)
    {
        return false;
    }
    if query.agent_id.as_deref().is_some_and(|value| {
        record.source_agent() != value && record.intended_target() != Some(value)
    }) {
        return false;
    }
    if query
        .subject
        .as_deref()
        .is_some_and(|value| record.subject() != value)
    {
        return false;
    }
    if query
        .failure_category
        .is_some_and(|value| record.failure_category() != value)
    {
        return false;
    }
    if query.time_window.is_some_and(|(start, end)| {
        record.terminal_unix_ms() < start || record.terminal_unix_ms() > end
    }) {
        return false;
    }
    true
}

fn evidence_matches_query(record: &EvidenceEnvelopeRecord, query: &EvidenceQuery) -> bool {
    if query
        .correlation_id
        .as_deref()
        .is_some_and(|value| record.correlation_id() != value)
    {
        return false;
    }
    if query
        .trace_id
        .as_deref()
        .is_some_and(|value| record.trace_id() != value)
    {
        return false;
    }
    if query
        .agent_id
        .as_deref()
        .is_some_and(|value| record.source_agent() != value && record.target_or_subject() != value)
    {
        return false;
    }
    if query
        .subject
        .as_deref()
        .is_some_and(|value| record.subject() != value)
    {
        return false;
    }
    if query
        .delivery_state
        .as_deref()
        .is_some_and(|value| record.delivery_state() != value)
    {
        return false;
    }
    if query.time_window.is_some_and(|(start, end)| {
        record.timestamp_unix_ms() < start || record.timestamp_unix_ms() > end
    }) {
        return false;
    }
    true
}

fn validate_evidence_field(name: &str, value: &str) -> Result<(), EvidenceStoreError> {
    if value.trim().is_empty() {
        return Err(EvidenceStoreError::new(
            EvidenceStoreErrorCode::Validation,
            format!("{name} must not be empty"),
        ));
    }
    if value.contains('\n') || value.contains('\r') {
        return Err(EvidenceStoreError::new(
            EvidenceStoreErrorCode::Validation,
            format!("{name} must not contain line breaks"),
        ));
    }
    Ok(())
}

struct AuditBuildInput<'a> {
    previous_hash: &'a str,
    daemon_sequence: u64,
    message_id: &'a str,
    actor: &'a str,
    action: &'a str,
    capability_or_subject: &'a str,
    correlation_id: &'a str,
    trace_id: &'a str,
    state_from: Option<&'a str>,
    state_to: &'a str,
    outcome_details: &'a str,
}

fn build_audit_entry(input: AuditBuildInput<'_>) -> EvidenceAuditEntry {
    let outcome_details = redact_sensitive(input.outcome_details);
    let state_from = input.state_from.map(ToOwned::to_owned);
    let current_audit_hash = audit_hash(&AuditHashInput {
        previous_hash: input.previous_hash,
        daemon_sequence: input.daemon_sequence,
        message_id: input.message_id,
        actor: input.actor,
        action: input.action,
        capability_or_subject: input.capability_or_subject,
        correlation_id: input.correlation_id,
        trace_id: input.trace_id,
        state_from: state_from.as_deref(),
        state_to: input.state_to,
        outcome_details: &outcome_details,
    });
    EvidenceAuditEntry {
        daemon_sequence: input.daemon_sequence,
        message_id: input.message_id.to_owned(),
        previous_audit_hash: input.previous_hash.to_owned(),
        current_audit_hash,
        actor: input.actor.to_owned(),
        action: input.action.to_owned(),
        capability_or_subject: input.capability_or_subject.to_owned(),
        correlation_id: input.correlation_id.to_owned(),
        trace_id: input.trace_id.to_owned(),
        state_from,
        state_to: input.state_to.to_owned(),
        outcome_details,
    }
}

fn validate_audit_hash(
    expected_previous_hash: &str,
    audit: &EvidenceAuditEntry,
) -> Result<(), String> {
    if audit.previous_audit_hash() != expected_previous_hash {
        return Err(format!(
            "audit previous hash '{}' does not match expected '{}'",
            audit.previous_audit_hash(),
            expected_previous_hash
        ));
    }
    let expected_current_hash = audit_hash(&AuditHashInput {
        previous_hash: audit.previous_audit_hash(),
        daemon_sequence: audit.daemon_sequence(),
        message_id: audit.message_id(),
        actor: audit.actor(),
        action: audit.action(),
        capability_or_subject: audit.capability_or_subject(),
        correlation_id: audit.correlation_id(),
        trace_id: audit.trace_id(),
        state_from: audit.state_from(),
        state_to: audit.state_to(),
        outcome_details: audit.outcome_details(),
    });
    if audit.current_audit_hash() != expected_current_hash {
        return Err(format!(
            "audit current hash '{}' does not match computed '{}'",
            audit.current_audit_hash(),
            expected_current_hash
        ));
    }
    Ok(())
}

struct AuditHashInput<'a> {
    previous_hash: &'a str,
    daemon_sequence: u64,
    message_id: &'a str,
    actor: &'a str,
    action: &'a str,
    capability_or_subject: &'a str,
    correlation_id: &'a str,
    trace_id: &'a str,
    state_from: Option<&'a str>,
    state_to: &'a str,
    outcome_details: &'a str,
}

fn audit_hash(input: &AuditHashInput<'_>) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for part in [
        input.previous_hash,
        &input.daemon_sequence.to_string(),
        input.message_id,
        input.actor,
        input.action,
        input.capability_or_subject,
        input.correlation_id,
        input.trace_id,
        input.state_from.unwrap_or(""),
        input.state_to,
        input.outcome_details,
    ] {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    if hash == 0 {
        "1".to_owned()
    } else {
        format!("{hash:016x}")
    }
}

fn redact_sensitive(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("password") || lower.contains("token") || lower.contains("secret") {
        REDACTION_MARKER.to_owned()
    } else {
        value.to_owned()
    }
}

fn write_evidence_line(path: &Path, line: &str) -> Result<(), EvidenceStoreError> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| {
            EvidenceStoreError::new(
                EvidenceStoreErrorCode::Io,
                format!("open evidence store for append failed: {error}"),
            )
        })?;
    file.write_all(line.as_bytes()).map_err(|error| {
        EvidenceStoreError::new(
            EvidenceStoreErrorCode::Io,
            format!("append evidence line failed: {error}"),
        )
    })?;
    file.write_all(b"\n").map_err(|error| {
        EvidenceStoreError::new(
            EvidenceStoreErrorCode::Io,
            format!("append evidence newline failed: {error}"),
        )
    })?;
    file.sync_all().map_err(|error| {
        EvidenceStoreError::new(
            EvidenceStoreErrorCode::Io,
            format!("fsync evidence store failed: {error}"),
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EvidenceLogRecord {
    Schema,
    FutureSchema,
    Accepted {
        envelope: Box<EvidenceEnvelopeRecord>,
        audit: Box<EvidenceAuditEntry>,
    },
    Transition {
        audit: Box<EvidenceAuditEntry>,
    },
    DeadLetter {
        dead_letter: Box<EvidenceDeadLetterRecord>,
        audit: Box<EvidenceAuditEntry>,
    },
}

impl EvidenceLogRecord {
    fn encode(&self) -> String {
        match self {
            Self::Schema => format!("v{EVIDENCE_STORE_SCHEMA_VERSION}|schema|evidence-store"),
            Self::FutureSchema => unreachable!("future schema markers are never encoded"),
            Self::Accepted { envelope, audit } => [
                format!("v{EVIDENCE_STORE_SCHEMA_VERSION}"),
                "accepted".to_owned(),
                envelope.daemon_sequence().to_string(),
                encode_field(envelope.message_id()),
                encode_field(envelope.source_agent()),
                encode_field(envelope.target_or_subject()),
                encode_field(envelope.subject()),
                envelope.timestamp_unix_ms().to_string(),
                encode_field(envelope.correlation_id()),
                encode_field(envelope.trace_id()),
                encode_field(envelope.span_id()),
                encode_field(envelope.parent_message_id().unwrap_or("")),
                encode_field(envelope.delivery_state()),
                envelope.payload_len().to_string(),
                encode_field(envelope.payload_content_type()),
                encode_field(audit.previous_audit_hash()),
                encode_field(audit.current_audit_hash()),
                encode_field(audit.actor()),
                encode_field(audit.action()),
                encode_field(audit.capability_or_subject()),
                encode_field(audit.state_from().unwrap_or("")),
                encode_field(audit.state_to()),
                encode_field(audit.outcome_details()),
            ]
            .join("|"),
            Self::Transition { audit } => [
                format!("v{EVIDENCE_STORE_SCHEMA_VERSION}"),
                "transition".to_owned(),
                audit.daemon_sequence().to_string(),
                encode_field(audit.message_id()),
                encode_field(audit.actor()),
                encode_field(audit.action()),
                encode_field(audit.capability_or_subject()),
                encode_field(audit.correlation_id()),
                encode_field(audit.trace_id()),
                encode_field(audit.state_from().unwrap_or("")),
                encode_field(audit.state_to()),
                encode_field(audit.outcome_details()),
                encode_field(audit.previous_audit_hash()),
                encode_field(audit.current_audit_hash()),
            ]
            .join("|"),
            Self::DeadLetter { dead_letter, audit } => [
                format!("v{EVIDENCE_STORE_SCHEMA_VERSION}"),
                "dead_letter".to_owned(),
                dead_letter.daemon_sequence().to_string(),
                encode_field(dead_letter.message_id()),
                encode_field(dead_letter.source_agent()),
                encode_field(dead_letter.intended_target().unwrap_or("")),
                encode_field(dead_letter.subject()),
                encode_field(dead_letter.correlation_id()),
                encode_field(dead_letter.trace_id()),
                encode_field(dead_letter.terminal_state()),
                encode_field(dead_letter.failure_category().as_str()),
                encode_field(dead_letter.safe_details()),
                dead_letter.attempt_count().to_string(),
                encode_field(dead_letter.last_failure_category().as_str()),
                dead_letter.first_attempted_unix_ms().to_string(),
                dead_letter.last_attempted_unix_ms().to_string(),
                dead_letter.terminal_unix_ms().to_string(),
                dead_letter.payload_len().to_string(),
                encode_field(dead_letter.payload_content_type()),
                encode_field(audit.previous_audit_hash()),
                encode_field(audit.current_audit_hash()),
                encode_field(audit.actor()),
                encode_field(audit.action()),
                encode_field(audit.capability_or_subject()),
                encode_field(audit.state_from().unwrap_or("")),
                encode_field(audit.state_to()),
                encode_field(audit.outcome_details()),
            ]
            .join("|"),
        }
    }

    fn parse(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split('|').collect();
        let prefix = parts.first()?;
        if !prefix.starts_with('v') {
            return None;
        }
        let version: u32 = prefix.trim_start_matches('v').parse().ok()?;
        if version > EVIDENCE_STORE_SCHEMA_VERSION {
            return Some(Self::FutureSchema);
        }
        if version != EVIDENCE_STORE_SCHEMA_VERSION {
            return None;
        }
        match *parts.get(1)? {
            "schema" if parts.get(2) == Some(&"evidence-store") => Some(Self::Schema),
            "accepted" => Self::parse_accepted(&parts),
            "transition" => Self::parse_transition(&parts),
            "dead_letter" => Self::parse_dead_letter(&parts),
            _ => None,
        }
    }

    fn parse_accepted(parts: &[&str]) -> Option<Self> {
        if parts.len() != 22 && parts.len() != 23 {
            return None;
        }
        let has_span_id = parts.len() == 23;
        let offset = usize::from(has_span_id);
        let envelope = EvidenceEnvelopeRecord {
            daemon_sequence: parts.get(2)?.parse().ok()?,
            message_id: decode_field(parts.get(3)?)?,
            source_agent: decode_field(parts.get(4)?)?,
            target_or_subject: decode_field(parts.get(5)?)?,
            subject: decode_field(parts.get(6)?)?,
            timestamp_unix_ms: parts.get(7)?.parse().ok()?,
            correlation_id: decode_field(parts.get(8)?)?,
            trace_id: decode_field(parts.get(9)?)?,
            span_id: if has_span_id {
                decode_field(parts.get(10)?)?
            } else {
                "unavailable".to_owned()
            },
            parent_message_id: decode_optional_field(parts.get(10 + offset)?)?,
            delivery_state: decode_field(parts.get(11 + offset)?)?,
            payload_len: parts.get(12 + offset)?.parse().ok()?,
            payload_content_type: decode_field(parts.get(13 + offset)?)?,
        };
        let audit = EvidenceAuditEntry {
            daemon_sequence: envelope.daemon_sequence,
            message_id: envelope.message_id.clone(),
            previous_audit_hash: decode_field(parts.get(14 + offset)?)?,
            current_audit_hash: decode_field(parts.get(15 + offset)?)?,
            actor: decode_field(parts.get(16 + offset)?)?,
            action: decode_field(parts.get(17 + offset)?)?,
            capability_or_subject: decode_field(parts.get(18 + offset)?)?,
            correlation_id: envelope.correlation_id.clone(),
            trace_id: envelope.trace_id.clone(),
            state_from: decode_optional_field(parts.get(19 + offset)?)?,
            state_to: decode_field(parts.get(20 + offset)?)?,
            outcome_details: decode_field(parts.get(21 + offset)?)?,
        };
        Some(Self::Accepted {
            envelope: Box::new(envelope),
            audit: Box::new(audit),
        })
    }

    fn parse_transition(parts: &[&str]) -> Option<Self> {
        if parts.len() != 14 {
            return None;
        }
        Some(Self::Transition {
            audit: Box::new(EvidenceAuditEntry {
                daemon_sequence: parts.get(2)?.parse().ok()?,
                message_id: decode_field(parts.get(3)?)?,
                actor: decode_field(parts.get(4)?)?,
                action: decode_field(parts.get(5)?)?,
                capability_or_subject: decode_field(parts.get(6)?)?,
                correlation_id: decode_field(parts.get(7)?)?,
                trace_id: decode_field(parts.get(8)?)?,
                state_from: decode_optional_field(parts.get(9)?)?,
                state_to: decode_field(parts.get(10)?)?,
                outcome_details: decode_field(parts.get(11)?)?,
                previous_audit_hash: decode_field(parts.get(12)?)?,
                current_audit_hash: decode_field(parts.get(13)?)?,
            }),
        })
    }

    fn parse_dead_letter(parts: &[&str]) -> Option<Self> {
        if parts.len() != 27 {
            return None;
        }
        let failure_category =
            DeadLetterFailureCategory::from_wire(&decode_field(parts.get(10)?)?)?;
        let last_failure_category =
            DeadLetterFailureCategory::from_wire(&decode_field(parts.get(13)?)?)?;
        let dead_letter = EvidenceDeadLetterRecord {
            daemon_sequence: parts.get(2)?.parse().ok()?,
            message_id: decode_field(parts.get(3)?)?,
            source_agent: decode_field(parts.get(4)?)?,
            intended_target: decode_optional_field(parts.get(5)?)?,
            subject: decode_field(parts.get(6)?)?,
            correlation_id: decode_field(parts.get(7)?)?,
            trace_id: decode_field(parts.get(8)?)?,
            terminal_state: decode_field(parts.get(9)?)?,
            failure_category,
            safe_details: decode_field(parts.get(11)?)?,
            attempt_count: parts.get(12)?.parse().ok()?,
            last_failure_category,
            first_attempted_unix_ms: parts.get(14)?.parse().ok()?,
            last_attempted_unix_ms: parts.get(15)?.parse().ok()?,
            terminal_unix_ms: parts.get(16)?.parse().ok()?,
            payload_len: parts.get(17)?.parse().ok()?,
            payload_content_type: decode_field(parts.get(18)?)?,
        };
        let audit = EvidenceAuditEntry {
            daemon_sequence: dead_letter.daemon_sequence,
            message_id: dead_letter.message_id.clone(),
            previous_audit_hash: decode_field(parts.get(19)?)?,
            current_audit_hash: decode_field(parts.get(20)?)?,
            actor: decode_field(parts.get(21)?)?,
            action: decode_field(parts.get(22)?)?,
            capability_or_subject: decode_field(parts.get(23)?)?,
            correlation_id: dead_letter.correlation_id.clone(),
            trace_id: dead_letter.trace_id.clone(),
            state_from: decode_optional_field(parts.get(24)?)?,
            state_to: decode_field(parts.get(25)?)?,
            outcome_details: decode_field(parts.get(26)?)?,
        };
        Some(Self::DeadLetter {
            dead_letter: Box::new(dead_letter),
            audit: Box::new(audit),
        })
    }
}

fn encode_field(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push(nibble_to_hex(byte >> 4));
        encoded.push(nibble_to_hex(byte & 0x0f));
    }
    encoded
}

fn nibble_to_hex(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        10..=15 => char::from(b'a' + (value - 10)),
        _ => unreachable!("nibble is masked to four bits"),
    }
}

fn decode_optional_field(value: &str) -> Option<Option<String>> {
    let decoded = decode_field(value)?;
    Some((!decoded.is_empty()).then_some(decoded))
}

fn decode_field(value: &str) -> Option<String> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = hex_to_nibble(pair[0])?;
        let low = hex_to_nibble(pair[1])?;
        bytes.push((high << 4) | low);
    }
    String::from_utf8(bytes).ok()
}

fn hex_to_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

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
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|error| {
                SubscriptionStoreError::new(
                    SubscriptionStoreErrorCode::Io,
                    format!("create durable store parent dir failed: {error}"),
                )
            })?;
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
                format!("subscription identity '{identity}' has a different scope than provided"),
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
            Self::Ack { identity, sequence } => {
                format!("v{DURABLE_SUBSCRIPTION_LOG_VERSION}|ack|{identity}|{sequence}")
            }
            Self::Retry { identity } => {
                format!("v{DURABLE_SUBSCRIPTION_LOG_VERSION}|retry|{identity}")
            }
            Self::Retention {
                identity,
                min_retained,
            } => format!("v{DURABLE_SUBSCRIPTION_LOG_VERSION}|retention|{identity}|{min_retained}"),
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
