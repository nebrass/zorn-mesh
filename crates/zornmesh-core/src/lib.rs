#![doc = "Core domain types for the zornmesh workspace scaffold."]

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    source_agent: AgentRef,
    subject: Subject,
    payload: Vec<u8>,
}

impl Envelope {
    pub fn new(
        source_agent: impl Into<String>,
        subject: impl Into<String>,
        payload: impl Into<Vec<u8>>,
    ) -> Result<Self, EnvelopeError> {
        Ok(Self {
            source_agent: AgentRef::new(source_agent)?,
            subject: Subject::new(subject)?,
            payload: payload.into(),
        })
    }

    pub fn source_agent(&self) -> &str {
        self.source_agent.as_str()
    }

    pub fn subject(&self) -> &str {
        self.subject.as_str()
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
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
        if value.trim().is_empty() {
            return Err(EnvelopeError::EmptySubject);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvelopeError {
    EmptySourceAgent,
    EmptySubject,
}

impl fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySourceAgent => f.write_str("envelope source agent must not be empty"),
            Self::EmptySubject => f.write_str("envelope subject must not be empty"),
        }
    }
}

impl std::error::Error for EnvelopeError {}
