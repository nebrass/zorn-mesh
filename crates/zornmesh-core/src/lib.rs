#![doc = "Core domain types for the zornmesh workspace scaffold."]

use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const AGENT_CARD_PROFILE_VERSION: &str = "agentcard.v1";
pub const MAX_AGENT_CARD_DISPLAY_NAME_BYTES: usize = 256;
pub const MAX_AGENT_CARD_STABLE_ID_BYTES: usize = 256;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCardInput {
    pub profile_version: String,
    pub stable_id: String,
    pub display_name: String,
    pub transport: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentCardTransport {
    Unix,
    Tcp,
    InProcess,
}

impl AgentCardTransport {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unix => "unix",
            Self::Tcp => "tcp",
            Self::InProcess => "in_process",
        }
    }

    fn from_canonical(value: &str) -> Option<Self> {
        match value {
            "unix" => Some(Self::Unix),
            "tcp" => Some(Self::Tcp),
            "in_process" => Some(Self::InProcess),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCard {
    profile_version: String,
    stable_id: String,
    canonical_stable_id: String,
    display_name: String,
    raw_display_name: String,
    transport: AgentCardTransport,
    raw_transport: String,
    source: String,
}

impl AgentCard {
    pub fn from_input(input: AgentCardInput) -> Result<Self, AgentCardError> {
        let AgentCardInput {
            profile_version,
            stable_id,
            display_name,
            transport,
            source,
        } = input;

        if profile_version.trim() != AGENT_CARD_PROFILE_VERSION {
            return Err(AgentCardError::new(
                AgentCardErrorCode::UnsupportedVersion,
                format!(
                    "AgentCard profile version '{}' is not supported (expected '{AGENT_CARD_PROFILE_VERSION}')",
                    profile_version
                ),
            ));
        }
        if stable_id.trim().is_empty() {
            return Err(AgentCardError::new(
                AgentCardErrorCode::MissingStableId,
                "AgentCard stable_id must not be empty",
            ));
        }
        if stable_id.len() > MAX_AGENT_CARD_STABLE_ID_BYTES {
            return Err(AgentCardError::new(
                AgentCardErrorCode::MissingStableId,
                format!(
                    "AgentCard stable_id is {} bytes; maximum is {MAX_AGENT_CARD_STABLE_ID_BYTES}",
                    stable_id.len()
                ),
            ));
        }
        let trimmed_display = display_name.trim();
        if trimmed_display.is_empty() {
            return Err(AgentCardError::new(
                AgentCardErrorCode::MissingDisplayName,
                "AgentCard display_name must not be empty",
            ));
        }
        if trimmed_display.len() > MAX_AGENT_CARD_DISPLAY_NAME_BYTES {
            return Err(AgentCardError::new(
                AgentCardErrorCode::MissingDisplayName,
                format!(
                    "AgentCard display_name is {} bytes; maximum is {MAX_AGENT_CARD_DISPLAY_NAME_BYTES}",
                    trimmed_display.len()
                ),
            ));
        }
        let canonical_transport = transport.trim().to_ascii_lowercase();
        if canonical_transport.is_empty() {
            return Err(AgentCardError::new(
                AgentCardErrorCode::MissingTransport,
                "AgentCard transport must not be empty",
            ));
        }
        let Some(transport_kind) = AgentCardTransport::from_canonical(&canonical_transport) else {
            return Err(AgentCardError::new(
                AgentCardErrorCode::UnsupportedTransport,
                format!("AgentCard transport '{transport}' is not supported"),
            ));
        };
        if source.trim().is_empty() {
            return Err(AgentCardError::new(
                AgentCardErrorCode::MissingSource,
                "AgentCard source must not be empty",
            ));
        }

        Ok(Self {
            profile_version: AGENT_CARD_PROFILE_VERSION.to_owned(),
            canonical_stable_id: stable_id.to_ascii_lowercase(),
            stable_id,
            display_name: trimmed_display.to_owned(),
            raw_display_name: display_name,
            transport: transport_kind,
            raw_transport: transport,
            source,
        })
    }

    pub fn profile_version(&self) -> &str {
        &self.profile_version
    }

    pub fn stable_id(&self) -> &str {
        &self.stable_id
    }

    pub fn canonical_stable_id(&self) -> &str {
        &self.canonical_stable_id
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn raw_display_name(&self) -> &str {
        &self.raw_display_name
    }

    pub const fn transport(&self) -> AgentCardTransport {
        self.transport
    }

    pub fn raw_transport(&self) -> &str {
        &self.raw_transport
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn is_compatible_with(&self, other: &AgentCard) -> bool {
        self.canonical_stable_id == other.canonical_stable_id
            && self.profile_version == other.profile_version
            && self.transport == other.transport
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentCardErrorCode {
    UnsupportedVersion,
    MissingStableId,
    MissingDisplayName,
    MissingTransport,
    MissingSource,
    UnsupportedTransport,
    Conflict,
}

impl AgentCardErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedVersion => "E_AGENT_CARD_UNSUPPORTED_VERSION",
            Self::MissingStableId => "E_AGENT_CARD_MISSING_STABLE_ID",
            Self::MissingDisplayName => "E_AGENT_CARD_MISSING_DISPLAY_NAME",
            Self::MissingTransport => "E_AGENT_CARD_MISSING_TRANSPORT",
            Self::MissingSource => "E_AGENT_CARD_MISSING_SOURCE",
            Self::UnsupportedTransport => "E_AGENT_CARD_UNSUPPORTED_TRANSPORT",
            Self::Conflict => "E_AGENT_CARD_CONFLICT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCardError {
    code: AgentCardErrorCode,
    message: String,
}

impl AgentCardError {
    pub fn new(code: AgentCardErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> AgentCardErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for AgentCardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for AgentCardError {}

pub const MAX_CAPABILITY_ID_BYTES: usize = 128;
pub const MAX_CAPABILITY_VERSION_BYTES: usize = 32;
pub const MAX_CAPABILITY_SUMMARY_BYTES: usize = 512;
pub const MAX_CAPABILITY_SCHEMA_REF_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityDirection {
    Offered,
    Consumed,
    Both,
}

impl CapabilityDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Offered => "offered",
            Self::Consumed => "consumed",
            Self::Both => "both",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilitySchemaDialect {
    TypeBox,
    JsonSchema,
}

impl CapabilitySchemaDialect {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TypeBox => "typebox",
            Self::JsonSchema => "json_schema",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityDescriptor {
    capability_id: String,
    version: String,
    direction: CapabilityDirection,
    summary: String,
    schema_dialect: CapabilitySchemaDialect,
    schema_ref: String,
}

impl CapabilityDescriptor {
    pub fn builder(
        capability_id: impl Into<String>,
        version: impl Into<String>,
        direction: CapabilityDirection,
    ) -> CapabilityDescriptorBuilder {
        CapabilityDescriptorBuilder {
            capability_id: capability_id.into(),
            version: version.into(),
            direction,
            summary: String::new(),
            schema_dialect: CapabilitySchemaDialect::TypeBox,
            schema_ref: String::new(),
        }
    }

    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub const fn direction(&self) -> CapabilityDirection {
        self.direction
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub const fn schema_dialect(&self) -> CapabilitySchemaDialect {
        self.schema_dialect
    }

    pub fn schema_ref(&self) -> &str {
        &self.schema_ref
    }
}

#[derive(Debug, Clone)]
pub struct CapabilityDescriptorBuilder {
    capability_id: String,
    version: String,
    direction: CapabilityDirection,
    summary: String,
    schema_dialect: CapabilitySchemaDialect,
    schema_ref: String,
}

impl CapabilityDescriptorBuilder {
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = summary.into();
        self
    }

    pub fn with_schema_ref(
        mut self,
        dialect: CapabilitySchemaDialect,
        schema_ref: impl Into<String>,
    ) -> Self {
        self.schema_dialect = dialect;
        self.schema_ref = schema_ref.into();
        self
    }

    pub fn build(self) -> Result<CapabilityDescriptor, CapabilityDescriptorError> {
        if self.capability_id.trim().is_empty() {
            return Err(CapabilityDescriptorError::new(
                CapabilityDescriptorErrorCode::InvalidId,
                "capability_id must not be empty",
            ));
        }
        if self.capability_id.len() > MAX_CAPABILITY_ID_BYTES {
            return Err(CapabilityDescriptorError::new(
                CapabilityDescriptorErrorCode::InvalidId,
                format!(
                    "capability_id is {} bytes; maximum is {MAX_CAPABILITY_ID_BYTES}",
                    self.capability_id.len()
                ),
            ));
        }
        if !self
            .capability_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return Err(CapabilityDescriptorError::new(
                CapabilityDescriptorErrorCode::InvalidId,
                "capability_id must be ASCII alphanumeric with '.', '-', '_' separators",
            ));
        }
        if self.version.trim().is_empty() || self.version.len() > MAX_CAPABILITY_VERSION_BYTES {
            return Err(CapabilityDescriptorError::new(
                CapabilityDescriptorErrorCode::InvalidVersion,
                "capability version must be non-empty and within size limit",
            ));
        }
        if self.schema_ref.trim().is_empty() {
            return Err(CapabilityDescriptorError::new(
                CapabilityDescriptorErrorCode::InvalidSchema,
                "capability schema_ref must not be empty",
            ));
        }
        if self.schema_ref.len() > MAX_CAPABILITY_SCHEMA_REF_BYTES {
            return Err(CapabilityDescriptorError::new(
                CapabilityDescriptorErrorCode::InvalidSchema,
                format!(
                    "capability schema_ref is {} bytes; maximum is {MAX_CAPABILITY_SCHEMA_REF_BYTES}",
                    self.schema_ref.len()
                ),
            ));
        }
        if self.summary.len() > MAX_CAPABILITY_SUMMARY_BYTES {
            return Err(CapabilityDescriptorError::new(
                CapabilityDescriptorErrorCode::InvalidId,
                format!(
                    "capability summary is {} bytes; maximum is {MAX_CAPABILITY_SUMMARY_BYTES}",
                    self.summary.len()
                ),
            ));
        }
        Ok(CapabilityDescriptor {
            capability_id: self.capability_id,
            version: self.version,
            direction: self.direction,
            summary: self.summary,
            schema_dialect: self.schema_dialect,
            schema_ref: self.schema_ref,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityDescriptorErrorCode {
    InvalidId,
    InvalidVersion,
    InvalidSchema,
}

impl CapabilityDescriptorErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidId => "E_CAPABILITY_INVALID_ID",
            Self::InvalidVersion => "E_CAPABILITY_INVALID_VERSION",
            Self::InvalidSchema => "E_CAPABILITY_INVALID_SCHEMA",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityDescriptorError {
    code: CapabilityDescriptorErrorCode,
    message: String,
}

impl CapabilityDescriptorError {
    pub fn new(code: CapabilityDescriptorErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> CapabilityDescriptorErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for CapabilityDescriptorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for CapabilityDescriptorError {}
