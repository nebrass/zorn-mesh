#![doc = "Protocol encoding for the zornmesh workspace scaffold."]

use std::{
    fmt,
    io::{self, Read, Write},
};

use zornmesh_core::{Envelope, EnvelopeError};

const MAGIC: &[u8; 2] = b"ZM";
pub const ENVELOPE_WIRE_VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

const CLIENT_SUBSCRIBE: u8 = 1;
const CLIENT_PUBLISH: u8 = 2;
const SERVER_SEND_RESULT: u8 = 101;
const SERVER_DELIVERY: u8 = 102;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientFrame {
    Subscribe { pattern: String },
    Publish { envelope: Envelope },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerFrame {
    SendResult(SendResultFrame),
    Delivery { envelope: Envelope, attempt: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendResultFrame {
    status: FrameStatus,
    code: String,
    message: String,
}

impl SendResultFrame {
    pub fn new(status: FrameStatus, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
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
            envelope: cursor.take_envelope()?,
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
        }
        ServerFrame::Delivery { envelope, attempt } => {
            body.push(SERVER_DELIVERY);
            body.extend_from_slice(&attempt.to_be_bytes());
            put_envelope_fields(&mut body, envelope);
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
            ))
        }
        SERVER_DELIVERY => ServerFrame::Delivery {
            attempt: cursor.take_u32("attempt")?,
            envelope: cursor.take_envelope()?,
        },
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
    put_bytes(output, envelope.payload());
}

fn put_bytes(output: &mut Vec<u8>, value: &[u8]) {
    let len = u32::try_from(value.len()).expect("zornmesh protocol fields fit in u32");
    output.extend_from_slice(&len.to_be_bytes());
    output.extend_from_slice(value);
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
        let payload = self.take_bytes("payload")?;

        Envelope::with_metadata(
            source_agent,
            subject,
            payload,
            timestamp_unix_ms,
            correlation_id,
            content_type,
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

    fn take_string(&mut self, field: &'static str) -> Result<String, ProtoError> {
        let bytes = self.take_bytes(field)?;
        String::from_utf8(bytes).map_err(|_| ProtoError::InvalidUtf8(field))
    }

    fn take_bytes(&mut self, field: &'static str) -> Result<Vec<u8>, ProtoError> {
        let len = self.take_u32(field)? as usize;
        Ok(self.take_exact(len, field)?.to_vec())
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
