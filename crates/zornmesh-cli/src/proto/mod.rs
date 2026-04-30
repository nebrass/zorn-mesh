#![doc = "Protocol encoding for the zornmesh workspace scaffold."]

use std::{
    fmt,
    io::{self, Read, Write},
};

use crate::core::{
    CoordinationOutcome, CoordinationOutcomeKind, CoordinationStage, DeliveryOutcome, Envelope,
    EnvelopeError, NackReasonCategory,
};

const MAGIC: &[u8; 2] = b"ZM";
pub const ENVELOPE_WIRE_VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

const CLIENT_SUBSCRIBE: u8 = 1;
const CLIENT_PUBLISH: u8 = 2;
const CLIENT_ACK: u8 = 3;
const CLIENT_NACK: u8 = 4;
const SERVER_SEND_RESULT: u8 = 101;
const SERVER_DELIVERY: u8 = 102;
const SERVER_DELIVERY_OUTCOME: u8 = 103;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientFrame {
    Subscribe {
        pattern: String,
    },
    Publish {
        envelope: Box<Envelope>,
    },
    Ack {
        delivery_id: String,
    },
    Nack {
        delivery_id: String,
        reason: NackReasonCategory,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerFrame {
    SendResult(SendResultFrame),
    Delivery {
        delivery_id: String,
        envelope: Envelope,
        attempt: u32,
    },
    DeliveryOutcome(DeliveryOutcomeFrame),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendResultFrame {
    status: FrameStatus,
    code: String,
    message: String,
    outcome: CoordinationOutcome,
    durable_outcome: Option<CoordinationOutcome>,
}

impl SendResultFrame {
    pub fn new(
        status: FrameStatus,
        code: impl Into<String>,
        message: impl Into<String>,
        outcome: CoordinationOutcome,
        durable_outcome: Option<CoordinationOutcome>,
    ) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
            outcome,
            durable_outcome,
        }
    }

    pub const fn status(&self) -> FrameStatus {
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryOutcomeFrame {
    outcome: DeliveryOutcome,
}

impl DeliveryOutcomeFrame {
    pub fn new(
        delivery_id: impl Into<String>,
        outcome: CoordinationOutcome,
        reason: Option<NackReasonCategory>,
    ) -> Self {
        Self {
            outcome: DeliveryOutcome::new(delivery_id, outcome, reason),
        }
    }

    pub fn from_delivery_outcome(outcome: DeliveryOutcome) -> Self {
        Self { outcome }
    }

    pub const fn outcome(&self) -> &DeliveryOutcome {
        &self.outcome
    }

    pub fn delivery_id(&self) -> &str {
        self.outcome.delivery_id()
    }

    pub const fn reason(&self) -> Option<NackReasonCategory> {
        self.outcome.reason()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameStatus {
    Accepted,
    Rejected,
    ValidationFailed,
}

pub fn encode_envelope(envelope: &Envelope) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&ENVELOPE_WIRE_VERSION.to_be_bytes());
    put_envelope_fields(&mut bytes, envelope);
    bytes
}

pub fn decode_envelope(bytes: &[u8]) -> Result<Envelope, ProtoError> {
    let mut cursor = Cursor::new(bytes);
    cursor.expect_magic()?;
    let version = cursor.take_u16("version")?;
    if version != ENVELOPE_WIRE_VERSION {
        return Err(ProtoError::UnsupportedVersion(version));
    }

    let envelope = cursor.take_envelope()?;
    cursor.expect_end()?;
    Ok(envelope)
}

pub fn write_client_frame<W: Write>(writer: &mut W, frame: &ClientFrame) -> Result<(), ProtoError> {
    let mut body = Vec::new();
    body.extend_from_slice(MAGIC);
    body.extend_from_slice(&ENVELOPE_WIRE_VERSION.to_be_bytes());
    match frame {
        ClientFrame::Subscribe { pattern } => {
            body.push(CLIENT_SUBSCRIBE);
            put_bytes(&mut body, pattern.as_bytes());
        }
        ClientFrame::Publish { envelope } => {
            body.push(CLIENT_PUBLISH);
            put_envelope_fields(&mut body, envelope);
        }
        ClientFrame::Ack { delivery_id } => {
            body.push(CLIENT_ACK);
            put_bytes(&mut body, delivery_id.as_bytes());
        }
        ClientFrame::Nack {
            delivery_id,
            reason,
        } => {
            body.push(CLIENT_NACK);
            put_bytes(&mut body, delivery_id.as_bytes());
            put_bytes(&mut body, reason.as_str().as_bytes());
        }
    }
    write_frame_body(writer, &body)
}

pub fn read_client_frame<R: Read>(reader: &mut R) -> Result<ClientFrame, ProtoError> {
    let body = read_frame_body(reader)?;
    let mut cursor = Cursor::new(&body);
    cursor.expect_magic()?;
    let version = cursor.take_u16("version")?;
    if version != ENVELOPE_WIRE_VERSION {
        return Err(ProtoError::UnsupportedVersion(version));
    }

    let kind = cursor.take_u8("frame_type")?;
    let frame = match kind {
        CLIENT_SUBSCRIBE => ClientFrame::Subscribe {
            pattern: cursor.take_string("pattern")?,
        },
        CLIENT_PUBLISH => ClientFrame::Publish {
            envelope: Box::new(cursor.take_envelope()?),
        },
        CLIENT_ACK => ClientFrame::Ack {
            delivery_id: cursor.take_string("delivery_id")?,
        },
        CLIENT_NACK => ClientFrame::Nack {
            delivery_id: cursor.take_string("delivery_id")?,
            reason: cursor.take_nack_reason("nack_reason")?,
        },
        other => return Err(ProtoError::UnknownFrameType(other)),
    };
    cursor.expect_end()?;
    Ok(frame)
}

pub fn write_server_frame<W: Write>(writer: &mut W, frame: &ServerFrame) -> Result<(), ProtoError> {
    let mut body = Vec::new();
    body.extend_from_slice(MAGIC);
    body.extend_from_slice(&ENVELOPE_WIRE_VERSION.to_be_bytes());
    match frame {
        ServerFrame::SendResult(result) => {
            body.push(SERVER_SEND_RESULT);
            body.push(match result.status {
                FrameStatus::Accepted => 1,
                FrameStatus::Rejected => 2,
                FrameStatus::ValidationFailed => 3,
            });
            put_bytes(&mut body, result.code.as_bytes());
            put_bytes(&mut body, result.message.as_bytes());
            put_outcome(&mut body, &result.outcome);
            put_optional_outcome(&mut body, result.durable_outcome.as_ref());
        }
        ServerFrame::Delivery {
            delivery_id,
            envelope,
            attempt,
        } => {
            body.push(SERVER_DELIVERY);
            put_bytes(&mut body, delivery_id.as_bytes());
            body.extend_from_slice(&attempt.to_be_bytes());
            put_envelope_fields(&mut body, envelope);
        }
        ServerFrame::DeliveryOutcome(outcome) => {
            body.push(SERVER_DELIVERY_OUTCOME);
            put_bytes(&mut body, outcome.delivery_id().as_bytes());
            put_outcome(&mut body, outcome.outcome().outcome());
            put_optional_nack_reason(&mut body, outcome.reason());
        }
    }
    write_frame_body(writer, &body)
}

pub fn read_server_frame<R: Read>(reader: &mut R) -> Result<ServerFrame, ProtoError> {
    let body = read_frame_body(reader)?;
    let mut cursor = Cursor::new(&body);
    cursor.expect_magic()?;
    let version = cursor.take_u16("version")?;
    if version != ENVELOPE_WIRE_VERSION {
        return Err(ProtoError::UnsupportedVersion(version));
    }

    let kind = cursor.take_u8("frame_type")?;
    let frame = match kind {
        SERVER_SEND_RESULT => {
            let status = match cursor.take_u8("status")? {
                1 => FrameStatus::Accepted,
                2 => FrameStatus::Rejected,
                3 => FrameStatus::ValidationFailed,
                other => return Err(ProtoError::UnknownStatus(other)),
            };
            ServerFrame::SendResult(SendResultFrame::new(
                status,
                cursor.take_string("code")?,
                cursor.take_string("message")?,
                cursor.take_outcome()?,
                cursor.take_optional_outcome()?,
            ))
        }
        SERVER_DELIVERY => ServerFrame::Delivery {
            delivery_id: cursor.take_string("delivery_id")?,
            attempt: cursor.take_u32("attempt")?,
            envelope: cursor.take_envelope()?,
        },
        SERVER_DELIVERY_OUTCOME => {
            let delivery_id = cursor.take_string("delivery_id")?;
            let outcome = cursor.take_outcome()?;
            let reason = cursor.take_optional_nack_reason()?;
            ServerFrame::DeliveryOutcome(DeliveryOutcomeFrame::new(delivery_id, outcome, reason))
        }
        other => return Err(ProtoError::UnknownFrameType(other)),
    };
    cursor.expect_end()?;
    Ok(frame)
}

fn write_frame_body<W: Write>(writer: &mut W, body: &[u8]) -> Result<(), ProtoError> {
    if body.len() > MAX_FRAME_BYTES {
        return Err(ProtoError::FrameTooLarge {
            actual: body.len(),
            limit: MAX_FRAME_BYTES,
        });
    }
    let len = u32::try_from(body.len()).map_err(|_| ProtoError::FrameTooLarge {
        actual: body.len(),
        limit: MAX_FRAME_BYTES,
    })?;
    writer
        .write_all(&len.to_be_bytes())
        .map_err(ProtoError::from_io)?;
    writer.write_all(body).map_err(ProtoError::from_io)
}

fn read_frame_body<R: Read>(reader: &mut R) -> Result<Vec<u8>, ProtoError> {
    let mut len = [0_u8; 4];
    read_exact(reader, &mut len, "frame_length")?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(ProtoError::FrameTooLarge {
            actual: len,
            limit: MAX_FRAME_BYTES,
        });
    }

    let mut body = vec![0; len];
    read_exact(reader, &mut body, "frame_body")?;
    Ok(body)
}

fn read_exact<R: Read>(
    reader: &mut R,
    bytes: &mut [u8],
    field: &'static str,
) -> Result<(), ProtoError> {
    reader.read_exact(bytes).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            ProtoError::Truncated(field)
        } else {
            ProtoError::from_io(error)
        }
    })
}

fn put_envelope_fields(output: &mut Vec<u8>, envelope: &Envelope) {
    put_bytes(output, envelope.source_agent().as_bytes());
    put_bytes(output, envelope.subject().as_bytes());
    output.extend_from_slice(&envelope.timestamp_unix_ms().to_be_bytes());
    put_bytes(output, envelope.correlation_id().as_bytes());
    put_bytes(
        output,
        envelope.payload_metadata().content_type().as_bytes(),
    );
    put_bytes(output, envelope.trace_context().traceparent().as_bytes());
    put_optional_string(output, envelope.trace_context().tracestate());
    put_bytes(output, envelope.payload());
}

fn put_bytes(output: &mut Vec<u8>, value: &[u8]) {
    let len = u32::try_from(value.len()).expect("zornmesh protocol fields fit in u32");
    output.extend_from_slice(&len.to_be_bytes());
    output.extend_from_slice(value);
}

fn put_bool(output: &mut Vec<u8>, value: bool) {
    output.push(u8::from(value));
}

fn put_outcome(output: &mut Vec<u8>, outcome: &CoordinationOutcome) {
    put_bytes(output, outcome.kind().as_str().as_bytes());
    put_bytes(output, outcome.stage().as_str().as_bytes());
    put_bytes(output, outcome.code().as_bytes());
    put_bytes(output, outcome.message().as_bytes());
    put_bool(output, outcome.retryable());
    put_bool(output, outcome.terminal());
    output.extend_from_slice(&outcome.delivery_attempts().to_be_bytes());
}

fn put_optional_outcome(output: &mut Vec<u8>, outcome: Option<&CoordinationOutcome>) {
    match outcome {
        Some(outcome) => {
            put_bool(output, true);
            put_outcome(output, outcome);
        }
        None => put_bool(output, false),
    }
}

fn put_optional_string(output: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            put_bool(output, true);
            put_bytes(output, value.as_bytes());
        }
        None => put_bool(output, false),
    }
}

fn put_optional_nack_reason(output: &mut Vec<u8>, reason: Option<NackReasonCategory>) {
    match reason {
        Some(reason) => {
            put_bool(output, true);
            put_bytes(output, reason.as_str().as_bytes());
        }
        None => put_bool(output, false),
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn expect_magic(&mut self) -> Result<(), ProtoError> {
        let magic = self.take_exact(MAGIC.len(), "magic")?;
        if magic != MAGIC {
            return Err(ProtoError::InvalidMagic);
        }
        Ok(())
    }

    fn take_envelope(&mut self) -> Result<Envelope, ProtoError> {
        let source_agent = self.take_string("source_agent")?;
        let subject = self.take_string("subject")?;
        let timestamp_unix_ms = self.take_u64("timestamp_unix_ms")?;
        let correlation_id = self.take_string("correlation_id")?;
        let content_type = self.take_string("payload_content_type")?;
        let traceparent = self.take_string("traceparent")?;
        let tracestate = self.take_optional_string("tracestate")?;
        let payload = self.take_bytes("payload")?;

        Envelope::with_trace_context(
            source_agent,
            subject,
            payload,
            timestamp_unix_ms,
            correlation_id,
            content_type,
            traceparent,
            tracestate.as_deref(),
        )
        .map_err(ProtoError::InvalidEnvelope)
    }

    fn take_u8(&mut self, field: &'static str) -> Result<u8, ProtoError> {
        Ok(self.take_exact(1, field)?[0])
    }

    fn take_u16(&mut self, field: &'static str) -> Result<u16, ProtoError> {
        let bytes = self.take_exact(2, field)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn take_u32(&mut self, field: &'static str) -> Result<u32, ProtoError> {
        let bytes = self.take_exact(4, field)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn take_u64(&mut self, field: &'static str) -> Result<u64, ProtoError> {
        let bytes = self.take_exact(8, field)?;
        Ok(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn take_bool(&mut self, field: &'static str) -> Result<bool, ProtoError> {
        match self.take_u8(field)? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(ProtoError::InvalidBoolean(field, other)),
        }
    }

    fn take_string(&mut self, field: &'static str) -> Result<String, ProtoError> {
        let bytes = self.take_bytes(field)?;
        String::from_utf8(bytes).map_err(|_| ProtoError::InvalidUtf8(field))
    }

    fn take_bytes(&mut self, field: &'static str) -> Result<Vec<u8>, ProtoError> {
        let len = self.take_u32(field)? as usize;
        Ok(self.take_exact(len, field)?.to_vec())
    }

    fn take_outcome(&mut self) -> Result<CoordinationOutcome, ProtoError> {
        let kind = self.take_outcome_kind("outcome_kind")?;
        let stage = self.take_stage("outcome_stage")?;
        let code = self.take_string("outcome_code")?;
        let message = self.take_string("outcome_message")?;
        let retryable = self.take_bool("outcome_retryable")?;
        let terminal = self.take_bool("outcome_terminal")?;
        let delivery_attempts = self.take_u32("outcome_delivery_attempts")?;
        Ok(CoordinationOutcome::new(
            kind,
            stage,
            code,
            message,
            retryable,
            terminal,
            delivery_attempts,
        ))
    }

    fn take_optional_outcome(&mut self) -> Result<Option<CoordinationOutcome>, ProtoError> {
        if self.take_bool("has_durable_outcome")? {
            Ok(Some(self.take_outcome()?))
        } else {
            Ok(None)
        }
    }

    fn take_optional_string(&mut self, field: &'static str) -> Result<Option<String>, ProtoError> {
        if self.take_bool(field)? {
            Ok(Some(self.take_string(field)?))
        } else {
            Ok(None)
        }
    }

    fn take_outcome_kind(
        &mut self,
        field: &'static str,
    ) -> Result<CoordinationOutcomeKind, ProtoError> {
        let value = self.take_string(field)?;
        CoordinationOutcomeKind::from_wire(&value).ok_or(ProtoError::UnknownOutcomeKind(value))
    }

    fn take_stage(&mut self, field: &'static str) -> Result<CoordinationStage, ProtoError> {
        let value = self.take_string(field)?;
        CoordinationStage::from_wire(&value).ok_or(ProtoError::UnknownOutcomeStage(value))
    }

    fn take_nack_reason(&mut self, field: &'static str) -> Result<NackReasonCategory, ProtoError> {
        let value = self.take_string(field)?;
        NackReasonCategory::from_wire(&value).ok_or(ProtoError::UnknownNackReason(value))
    }

    fn take_optional_nack_reason(&mut self) -> Result<Option<NackReasonCategory>, ProtoError> {
        if self.take_bool("has_nack_reason")? {
            Ok(Some(self.take_nack_reason("nack_reason")?))
        } else {
            Ok(None)
        }
    }

    fn take_exact(&mut self, len: usize, field: &'static str) -> Result<&'a [u8], ProtoError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(ProtoError::LengthOverflow(field))?;
        if end > self.bytes.len() {
            return Err(ProtoError::Truncated(field));
        }
        let slice = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    fn expect_end(&self) -> Result<(), ProtoError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(ProtoError::TrailingBytes)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtoError {
    InvalidMagic,
    UnsupportedVersion(u16),
    UnknownFrameType(u8),
    UnknownStatus(u8),
    UnknownOutcomeKind(String),
    UnknownOutcomeStage(String),
    UnknownNackReason(String),
    InvalidBoolean(&'static str, u8),
    Truncated(&'static str),
    LengthOverflow(&'static str),
    InvalidUtf8(&'static str),
    InvalidEnvelope(EnvelopeError),
    FrameTooLarge { actual: usize, limit: usize },
    Io(io::ErrorKind, String),
    TrailingBytes,
}

impl ProtoError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidEnvelope(error) => error.code(),
            Self::FrameTooLarge { .. } => "E_PAYLOAD_LIMIT",
            Self::Io(_, _) => "E_DAEMON_IO",
            _ => "E_PROTOCOL",
        }
    }

    pub const fn io_kind(&self) -> Option<io::ErrorKind> {
        match self {
            Self::Io(kind, _) => Some(*kind),
            _ => None,
        }
    }

    fn from_io(value: io::Error) -> Self {
        Self::Io(value.kind(), value.to_string())
    }
}

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic => f.write_str("invalid zornmesh frame magic"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported zornmesh frame version {version}")
            }
            Self::UnknownFrameType(kind) => write!(f, "unknown zornmesh frame type {kind}"),
            Self::UnknownStatus(status) => write!(f, "unknown zornmesh result status {status}"),
            Self::UnknownOutcomeKind(kind) => write!(f, "unknown zornmesh outcome kind {kind}"),
            Self::UnknownOutcomeStage(stage) => {
                write!(f, "unknown zornmesh outcome stage {stage}")
            }
            Self::UnknownNackReason(reason) => {
                write!(f, "unknown zornmesh NACK reason {reason}")
            }
            Self::InvalidBoolean(field, value) => {
                write!(
                    f,
                    "zornmesh frame field {field} has invalid boolean {value}"
                )
            }
            Self::Truncated(field) => write!(f, "truncated zornmesh frame field {field}"),
            Self::LengthOverflow(field) => {
                write!(f, "zornmesh frame field {field} length overflowed")
            }
            Self::InvalidUtf8(field) => {
                write!(f, "zornmesh frame field {field} is not valid UTF-8")
            }
            Self::InvalidEnvelope(error) => write!(f, "invalid zornmesh envelope: {error}"),
            Self::FrameTooLarge { actual, limit } => {
                write!(
                    f,
                    "zornmesh frame is {actual} bytes; maximum is {limit} bytes"
                )
            }
            Self::Io(_, error) => write!(f, "zornmesh frame I/O failed: {error}"),
            Self::TrailingBytes => f.write_str("zornmesh frame contains trailing bytes"),
        }
    }
}

impl std::error::Error for ProtoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidEnvelope(error) => Some(error),
            _ => None,
        }
    }
}
