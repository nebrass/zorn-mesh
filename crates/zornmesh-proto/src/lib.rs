#![doc = "Protocol smoke encoding for the zornmesh workspace scaffold."]

use std::fmt;

use zornmesh_core::{Envelope, EnvelopeError};

const MAGIC: &[u8; 2] = b"ZM";
pub const ENVELOPE_WIRE_VERSION: u16 = 1;

pub fn encode_envelope(envelope: &Envelope) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&ENVELOPE_WIRE_VERSION.to_be_bytes());
    put_bytes(&mut bytes, envelope.source_agent().as_bytes());
    put_bytes(&mut bytes, envelope.subject().as_bytes());
    put_bytes(&mut bytes, envelope.payload());
    bytes
}

pub fn decode_envelope(bytes: &[u8]) -> Result<Envelope, ProtoError> {
    let mut cursor = Cursor::new(bytes);
    cursor.expect_magic()?;
    let version = cursor.take_u16("version")?;
    if version != ENVELOPE_WIRE_VERSION {
        return Err(ProtoError::UnsupportedVersion(version));
    }

    let source_agent = cursor.take_string("source_agent")?;
    let subject = cursor.take_string("subject")?;
    let payload = cursor.take_bytes("payload")?;
    cursor.expect_end()?;

    Envelope::new(source_agent, subject, payload).map_err(ProtoError::InvalidEnvelope)
}

fn put_bytes(output: &mut Vec<u8>, value: &[u8]) {
    let len = u32::try_from(value.len()).expect("smoke envelope fields fit in u32");
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

    fn take_u16(&mut self, field: &'static str) -> Result<u16, ProtoError> {
        let bytes = self.take_exact(2, field)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn take_string(&mut self, field: &'static str) -> Result<String, ProtoError> {
        let bytes = self.take_bytes(field)?;
        String::from_utf8(bytes).map_err(|_| ProtoError::InvalidUtf8(field))
    }

    fn take_bytes(&mut self, field: &'static str) -> Result<Vec<u8>, ProtoError> {
        let len = self.take_u32(field)? as usize;
        Ok(self.take_exact(len, field)?.to_vec())
    }

    fn take_u32(&mut self, field: &'static str) -> Result<u32, ProtoError> {
        let bytes = self.take_exact(4, field)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
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
    Truncated(&'static str),
    LengthOverflow(&'static str),
    InvalidUtf8(&'static str),
    InvalidEnvelope(EnvelopeError),
    TrailingBytes,
}

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic => f.write_str("invalid zornmesh envelope magic"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported zornmesh envelope version {version}")
            }
            Self::Truncated(field) => write!(f, "truncated zornmesh envelope field {field}"),
            Self::LengthOverflow(field) => {
                write!(f, "zornmesh envelope field {field} length overflowed")
            }
            Self::InvalidUtf8(field) => {
                write!(f, "zornmesh envelope field {field} is not valid UTF-8")
            }
            Self::InvalidEnvelope(error) => write!(f, "invalid zornmesh envelope: {error}"),
            Self::TrailingBytes => f.write_str("zornmesh envelope contains trailing bytes"),
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
