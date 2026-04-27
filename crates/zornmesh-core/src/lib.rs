#![doc = "Core domain types for the zornmesh workspace scaffold."]

use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_SUBJECT_BYTES: usize = 256;
pub const MAX_SUBJECT_LEVELS: usize = 8;
pub const MAX_ENVELOPE_PAYLOAD_BYTES: usize = 64 * 1024;
pub const COORDINATION_CONTRACT_VERSION: &str = "zornmesh.coordination.v1";
pub const ENVELOPE_SCHEMA_VERSION: &str = "zornmesh.envelope.v1";
pub const ERROR_CONTRACT_VERSION: &str = "zornmesh.error.v1";
pub const DELIVERY_STATE_TAXONOMY_VERSION: &str = "zornmesh.delivery-state.v1";

static NEXT_CORRELATION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinationOutcomeKind {
    Accepted,
    DurableAccepted,
    Acknowledged,
    Rejected,
    Failed,
    TimedOut,
    Retryable,
    Terminal,
}

impl CoordinationOutcomeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::DurableAccepted => "durable_accepted",
            Self::Acknowledged => "acknowledged",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Retryable => "retryable",
            Self::Terminal => "terminal",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "accepted" => Some(Self::Accepted),
            "durable_accepted" => Some(Self::DurableAccepted),
            "acknowledged" => Some(Self::Acknowledged),
            "rejected" => Some(Self::Rejected),
            "failed" => Some(Self::Failed),
            "timed_out" => Some(Self::TimedOut),
            "retryable" => Some(Self::Retryable),
            "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinationStage {
    Transport,
    Durable,
    Delivery,
    Protocol,
}

impl CoordinationStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transport => "transport",
            Self::Durable => "durable",
            Self::Delivery => "delivery",
            Self::Protocol => "protocol",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "transport" => Some(Self::Transport),
            "durable" => Some(Self::Durable),
            "delivery" => Some(Self::Delivery),
            "protocol" => Some(Self::Protocol),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinationOutcome {
    kind: CoordinationOutcomeKind,
    stage: CoordinationStage,
    code: String,
    message: String,
    retryable: bool,
    terminal: bool,
    delivery_attempts: u32,
}

impl CoordinationOutcome {
    pub fn new(
        kind: CoordinationOutcomeKind,
        stage: CoordinationStage,
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        terminal: bool,
        delivery_attempts: u32,
    ) -> Self {
        Self {
            kind,
            stage,
            code: code.into(),
            message: message.into(),
            retryable,
            terminal,
            delivery_attempts,
        }
    }

    pub fn accepted(message: impl Into<String>, delivery_attempts: u32) -> Self {
        Self::new(
            CoordinationOutcomeKind::Accepted,
            CoordinationStage::Transport,
            "ACCEPTED",
            message,
            false,
            false,
            delivery_attempts,
        )
    }

    pub fn durable_accepted(message: impl Into<String>, delivery_attempts: u32) -> Self {
        Self::new(
            CoordinationOutcomeKind::DurableAccepted,
            CoordinationStage::Durable,
            "DURABLE_ACCEPTED",
            message,
            false,
            false,
            delivery_attempts,
        )
    }

    pub fn persistence_unavailable() -> Self {
        Self::new(
            CoordinationOutcomeKind::Failed,
            CoordinationStage::Durable,
            "E_PERSISTENCE_UNAVAILABLE",
            "durable coordination state is unavailable for the in-memory broker",
            false,
            true,
            0,
        )
    }

    pub fn acknowledged(message: impl Into<String>, delivery_attempts: u32) -> Self {
        Self::new(
            CoordinationOutcomeKind::Acknowledged,
            CoordinationStage::Delivery,
            "ACKNOWLEDGED",
            message,
            false,
            true,
            delivery_attempts,
        )
    }

    pub fn rejected(message: impl Into<String>, delivery_attempts: u32) -> Self {
        Self::new(
            CoordinationOutcomeKind::Rejected,
            CoordinationStage::Delivery,
            "REJECTED",
            message,
            false,
            true,
            delivery_attempts,
        )
    }

    pub fn failed(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self::new(
            CoordinationOutcomeKind::Failed,
            CoordinationStage::Delivery,
            code,
            message,
            retryable,
            true,
            0,
        )
    }

    pub const fn version(&self) -> &'static str {
        COORDINATION_CONTRACT_VERSION
    }

    pub const fn kind(&self) -> CoordinationOutcomeKind {
        self.kind
    }

    pub const fn stage(&self) -> CoordinationStage {
        self.stage
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn retryable(&self) -> bool {
        self.retryable
    }

    pub const fn terminal(&self) -> bool {
        self.terminal
    }

    pub const fn delivery_attempts(&self) -> u32 {
        self.delivery_attempts
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Validation,
    Authorization,
    Reachability,
    Timeout,
    PayloadLimit,
    Protocol,
    PersistenceUnavailable,
    Conflict,
    Internal,
}

impl ErrorCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Authorization => "authorization",
            Self::Reachability => "reachability",
            Self::Timeout => "timeout",
            Self::PayloadLimit => "payload_limit",
            Self::Protocol => "protocol",
            Self::PersistenceUnavailable => "persistence_unavailable",
            Self::Conflict => "conflict",
            Self::Internal => "internal",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "validation" => Some(Self::Validation),
            "authorization" => Some(Self::Authorization),
            "reachability" => Some(Self::Reachability),
            "timeout" => Some(Self::Timeout),
            "payload_limit" => Some(Self::PayloadLimit),
            "protocol" => Some(Self::Protocol),
            "persistence_unavailable" => Some(Self::PersistenceUnavailable),
            "conflict" => Some(Self::Conflict),
            "internal" => Some(Self::Internal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductError {
    code: String,
    category: ErrorCategory,
    retryable: bool,
    safe_details: String,
}

impl ProductError {
    pub fn new(
        code: impl Into<String>,
        category: ErrorCategory,
        retryable: bool,
        safe_details: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            category,
            retryable,
            safe_details: safe_details.into(),
        }
    }

    pub const fn version(&self) -> &'static str {
        ERROR_CONTRACT_VERSION
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    pub const fn retryable(&self) -> bool {
        self.retryable
    }

    pub fn safe_details(&self) -> &str {
        &self.safe_details
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NackReasonCategory {
    Validation,
    Authorization,
    Processing,
    Timeout,
    PayloadLimit,
    Backpressure,
    Transient,
    Policy,
    Unknown,
}

impl NackReasonCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Authorization => "authorization",
            Self::Processing => "processing",
            Self::Timeout => "timeout",
            Self::PayloadLimit => "payload_limit",
            Self::Backpressure => "backpressure",
            Self::Transient => "transient",
            Self::Policy => "policy",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "validation" => Some(Self::Validation),
            "authorization" => Some(Self::Authorization),
            "processing" => Some(Self::Processing),
            "timeout" => Some(Self::Timeout),
            "payload_limit" => Some(Self::PayloadLimit),
            "backpressure" => Some(Self::Backpressure),
            "transient" => Some(Self::Transient),
            "policy" => Some(Self::Policy),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryOutcome {
    delivery_id: String,
    outcome: CoordinationOutcome,
    reason: Option<NackReasonCategory>,
}

impl DeliveryOutcome {
    pub fn acknowledged(delivery_id: impl Into<String>) -> Self {
        let delivery_id = delivery_id.into();
        Self {
            outcome: CoordinationOutcome::acknowledged(
                format!("delivery {delivery_id} acknowledged"),
                1,
            ),
            delivery_id,
            reason: None,
        }
    }

    pub fn rejected(delivery_id: impl Into<String>, reason: NackReasonCategory) -> Self {
        let delivery_id = delivery_id.into();
        Self {
            outcome: CoordinationOutcome::rejected(
                format!("delivery {delivery_id} rejected with reason {}", reason.as_str()),
                1,
            ),
            delivery_id,
            reason: Some(reason),
        }
    }

    pub fn new(
        delivery_id: impl Into<String>,
        outcome: CoordinationOutcome,
        reason: Option<NackReasonCategory>,
    ) -> Self {
        Self {
            delivery_id: delivery_id.into(),
            outcome,
            reason,
        }
    }

    pub fn delivery_id(&self) -> &str {
        &self.delivery_id
    }

    pub const fn kind(&self) -> CoordinationOutcomeKind {
        self.outcome.kind()
    }

    pub const fn stage(&self) -> CoordinationStage {
        self.outcome.stage()
    }

    pub const fn reason(&self) -> Option<NackReasonCategory> {
        self.reason
    }

    pub fn outcome(&self) -> &CoordinationOutcome {
        &self.outcome
    }

    pub fn code(&self) -> &str {
        self.outcome.code()
    }

    pub fn message(&self) -> &str {
        self.outcome.message()
    }

    pub const fn retryable(&self) -> bool {
        self.outcome.retryable()
    }

    pub const fn terminal(&self) -> bool {
        self.outcome.terminal()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    source_agent: AgentRef,
    subject: Subject,
    timestamp_unix_ms: u64,
    correlation_id: String,
    payload_metadata: PayloadMetadata,
    payload: Vec<u8>,
}

impl Envelope {
    pub fn new(
        source_agent: impl Into<String>,
        subject: impl Into<String>,
        payload: impl Into<Vec<u8>>,
    ) -> Result<Self, EnvelopeError> {
        let timestamp_unix_ms = current_unix_ms();
        let correlation_id = format!(
            "corr-{timestamp_unix_ms}-{}",
            NEXT_CORRELATION_ID.fetch_add(1, Ordering::Relaxed)
        );
        Self::with_metadata(
            source_agent,
            subject,
            payload,
            timestamp_unix_ms,
            correlation_id,
            "application/octet-stream",
        )
    }

    pub fn with_metadata(
        source_agent: impl Into<String>,
        subject: impl Into<String>,
        payload: impl Into<Vec<u8>>,
        timestamp_unix_ms: u64,
        correlation_id: impl Into<String>,
        content_type: impl Into<String>,
    ) -> Result<Self, EnvelopeError> {
        let payload = payload.into();
        if payload.len() > MAX_ENVELOPE_PAYLOAD_BYTES {
            return Err(EnvelopeError::PayloadTooLarge {
                actual: payload.len(),
                limit: MAX_ENVELOPE_PAYLOAD_BYTES,
            });
        }

        let correlation_id = correlation_id.into();
        if correlation_id.trim().is_empty() {
            return Err(EnvelopeError::EmptyCorrelationId);
        }

        Ok(Self {
            source_agent: AgentRef::new(source_agent)?,
            subject: Subject::new(subject)?,
            timestamp_unix_ms,
            correlation_id,
            payload_metadata: PayloadMetadata::new(content_type, payload.len())?,
            payload,
        })
    }

    pub fn source_agent(&self) -> &str {
        self.source_agent.as_str()
    }

    pub fn subject(&self) -> &str {
        self.subject.as_str()
    }

    pub const fn timestamp_unix_ms(&self) -> u64 {
        self.timestamp_unix_ms
    }

    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    pub const fn payload_metadata(&self) -> &PayloadMetadata {
        &self.payload_metadata
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadMetadata {
    content_type: String,
    payload_len: usize,
}

impl PayloadMetadata {
    pub fn new(content_type: impl Into<String>, payload_len: usize) -> Result<Self, EnvelopeError> {
        let content_type = content_type.into();
        if content_type.trim().is_empty() {
            return Err(EnvelopeError::EmptyPayloadContentType);
        }

        Ok(Self {
            content_type,
            payload_len,
        })
    }

    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    pub const fn payload_len(&self) -> usize {
        self.payload_len
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRef(String);

impl AgentRef {
    pub fn new(value: impl Into<String>) -> Result<Self, EnvelopeError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(EnvelopeError::EmptySourceAgent);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject(String);

impl Subject {
    pub fn new(value: impl Into<String>) -> Result<Self, EnvelopeError> {
        let value = value.into();
        validate_subject(&value).map_err(|error| match error {
            SubjectValidationError::Empty => EnvelopeError::EmptySubject,
            other => EnvelopeError::InvalidSubject(other),
        })?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn validate_subject(value: &str) -> Result<(), SubjectValidationError> {
    validate_subject_like(value, false)
}

pub fn validate_subject_pattern(value: &str) -> Result<(), SubjectValidationError> {
    validate_subject_like(value, true)
}

fn validate_subject_like(value: &str, allow_wildcards: bool) -> Result<(), SubjectValidationError> {
    if value.trim().is_empty() {
        return Err(SubjectValidationError::Empty);
    }
    if value.len() > MAX_SUBJECT_BYTES {
        return Err(SubjectValidationError::TooLong {
            actual: value.len(),
            limit: MAX_SUBJECT_BYTES,
        });
    }
    if value == "zorn" || value.starts_with("zorn.") {
        return Err(SubjectValidationError::ReservedPrefix);
    }

    let levels = value.split('.').collect::<Vec<_>>();
    if levels.len() > MAX_SUBJECT_LEVELS {
        return Err(SubjectValidationError::TooManyLevels {
            actual: levels.len(),
            limit: MAX_SUBJECT_LEVELS,
        });
    }

    for (index, level) in levels.iter().enumerate() {
        if level.is_empty() {
            return Err(SubjectValidationError::EmptyLevel);
        }

        let contains_wildcard = level.contains('*') || level.contains('>');
        if !allow_wildcards && contains_wildcard {
            return Err(SubjectValidationError::InvalidWildcardSyntax);
        }
        if allow_wildcards {
            match *level {
                "*" => {}
                ">" if index + 1 == levels.len() => {}
                ">" => return Err(SubjectValidationError::InvalidWildcardSyntax),
                _ if contains_wildcard => {
                    return Err(SubjectValidationError::InvalidWildcardSyntax);
                }
                _ => {}
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubjectValidationError {
    Empty,
    TooLong { actual: usize, limit: usize },
    TooManyLevels { actual: usize, limit: usize },
    EmptyLevel,
    ReservedPrefix,
    InvalidWildcardSyntax,
}

impl SubjectValidationError {
    pub const fn code(&self) -> &'static str {
        "E_SUBJECT_VALIDATION"
    }
}

impl fmt::Display for SubjectValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("subject must not be empty"),
            Self::TooLong { actual, limit } => {
                write!(f, "subject is {actual} bytes; maximum is {limit} bytes")
            }
            Self::TooManyLevels { actual, limit } => {
                write!(f, "subject has {actual} levels; maximum is {limit} levels")
            }
            Self::EmptyLevel => f.write_str("subject levels must not be empty"),
            Self::ReservedPrefix => f.write_str("subject must not use reserved zorn prefixes"),
            Self::InvalidWildcardSyntax => {
                f.write_str("subject wildcard syntax is invalid for this operation")
            }
        }
    }
}

impl std::error::Error for SubjectValidationError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvelopeError {
    EmptySourceAgent,
    EmptySubject,
    InvalidSubject(SubjectValidationError),
    EmptyCorrelationId,
    EmptyPayloadContentType,
    PayloadTooLarge { actual: usize, limit: usize },
}

impl EnvelopeError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidSubject(_) | Self::EmptySubject => "E_SUBJECT_VALIDATION",
            Self::PayloadTooLarge { .. } => "E_PAYLOAD_LIMIT",
            _ => "E_ENVELOPE_VALIDATION",
        }
    }
}

impl fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySourceAgent => f.write_str("envelope source agent must not be empty"),
            Self::EmptySubject => f.write_str("envelope subject must not be empty"),
            Self::InvalidSubject(error) => write!(f, "invalid envelope subject: {error}"),
            Self::EmptyCorrelationId => f.write_str("envelope correlation ID must not be empty"),
            Self::EmptyPayloadContentType => {
                f.write_str("envelope payload content type must not be empty")
            }
            Self::PayloadTooLarge { actual, limit } => {
                write!(
                    f,
                    "envelope payload is {actual} bytes; maximum is {limit} bytes"
                )
            }
        }
    }
}

impl std::error::Error for EnvelopeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidSubject(error) => Some(error),
            _ => None,
        }
    }
}

fn current_unix_ms() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}
