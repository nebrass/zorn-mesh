#![doc = "Core domain types for the zornmesh workspace scaffold."]

use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_SUBJECT_BYTES: usize = 256;
pub const MAX_SUBJECT_LEVELS: usize = 8;
pub const MAX_ENVELOPE_PAYLOAD_BYTES: usize = 64 * 1024;

static NEXT_CORRELATION_ID: AtomicU64 = AtomicU64::new(1);

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
