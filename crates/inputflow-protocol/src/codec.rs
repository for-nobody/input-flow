use std::fmt;
use std::io::{self, Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::MAX_FRAME_BYTES;

#[derive(Debug)]
pub enum CodecError {
    Io(io::Error),
    TruncatedHeader,
    EmptyFrame,
    FrameTooLarge { length: usize, maximum: usize },
    TruncatedPayload { expected: usize, received: usize },
    InvalidJson(serde_json::Error),
}

impl CodecError {
    pub const fn connection_fatal(&self) -> bool {
        matches!(
            self,
            Self::Io(_)
                | Self::TruncatedHeader
                | Self::FrameTooLarge { .. }
                | Self::TruncatedPayload { .. }
        )
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::TruncatedHeader => formatter.write_str("frame header ended before four bytes"),
            Self::EmptyFrame => formatter.write_str("frame payload length must be nonzero"),
            Self::FrameTooLarge { length, maximum } => {
                write!(formatter, "frame length {length} exceeds maximum {maximum}")
            }
            Self::TruncatedPayload { expected, received } => write!(
                formatter,
                "frame payload ended after {received} of {expected} bytes"
            ),
            Self::InvalidJson(error) => write!(formatter, "invalid JSON frame: {error}"),
        }
    }
}

impl std::error::Error for CodecError {}

pub fn read_json_frame<R, T>(reader: &mut R) -> Result<Option<T>, CodecError>
where
    R: Read,
    T: DeserializeOwned,
{
    let Some(length) = read_length(reader)? else {
        return Ok(None);
    };
    if length == 0 {
        return Err(CodecError::EmptyFrame);
    }
    if length > MAX_FRAME_BYTES {
        return Err(CodecError::FrameTooLarge {
            length,
            maximum: MAX_FRAME_BYTES,
        });
    }

    let mut payload = vec![0; length];
    let received = read_exact_count(reader, &mut payload).map_err(CodecError::Io)?;
    if received != length {
        return Err(CodecError::TruncatedPayload {
            expected: length,
            received,
        });
    }
    serde_json::from_slice(&payload)
        .map(Some)
        .map_err(CodecError::InvalidJson)
}

pub fn write_json_frame<W, T>(writer: &mut W, value: &T) -> Result<(), CodecError>
where
    W: Write,
    T: Serialize,
{
    let payload = serde_json::to_vec(value).map_err(CodecError::InvalidJson)?;
    if payload.is_empty() {
        return Err(CodecError::EmptyFrame);
    }
    if payload.len() > MAX_FRAME_BYTES {
        return Err(CodecError::FrameTooLarge {
            length: payload.len(),
            maximum: MAX_FRAME_BYTES,
        });
    }
    writer
        .write_all(&(payload.len() as u32).to_le_bytes())
        .map_err(CodecError::Io)?;
    writer.write_all(&payload).map_err(CodecError::Io)?;
    writer.flush().map_err(CodecError::Io)
}

fn read_length<R: Read>(reader: &mut R) -> Result<Option<usize>, CodecError> {
    let mut header = [0_u8; 4];
    let received = read_exact_count(reader, &mut header).map_err(CodecError::Io)?;
    match received {
        0 => Ok(None),
        4 => Ok(Some(u32::from_le_bytes(header) as usize)),
        _ => Err(CodecError::TruncatedHeader),
    }
}

fn read_exact_count<R: Read>(reader: &mut R, buffer: &mut [u8]) -> io::Result<usize> {
    let mut received = 0;
    while received < buffer.len() {
        match reader.read(&mut buffer[received..]) {
            Ok(0) => break,
            Ok(count) => received += count,
            Err(error) => return Err(error),
        }
    }
    Ok(received)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use serde_json::Value;

    use super::*;

    struct ChunkedReader {
        bytes: Cursor<Vec<u8>>,
        chunk: usize,
    }

    impl Read for ChunkedReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let limit = buffer.len().min(self.chunk);
            self.bytes.read(&mut buffer[..limit])
        }
    }

    #[test]
    fn round_trip_accepts_partial_header_and_payload_reads() {
        let original = serde_json::json!({"protocol_version": 1, "request_id": "a"});
        let mut wire = Vec::new();
        write_json_frame(&mut wire, &original).unwrap();
        let mut reader = ChunkedReader {
            bytes: Cursor::new(wire),
            chunk: 2,
        };
        let decoded: Value = read_json_frame(&mut reader).unwrap().unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn rejects_oversized_before_allocating_payload() {
        let mut reader = Cursor::new(((MAX_FRAME_BYTES + 1) as u32).to_le_bytes());
        assert!(matches!(
            read_json_frame::<_, Value>(&mut reader),
            Err(CodecError::FrameTooLarge { .. })
        ));
    }

    #[test]
    fn reports_truncated_header_and_payload() {
        let mut header = Cursor::new(vec![1, 2]);
        assert!(matches!(
            read_json_frame::<_, Value>(&mut header),
            Err(CodecError::TruncatedHeader)
        ));

        let mut payload = Cursor::new([4_u32.to_le_bytes().as_slice(), b"{}"].concat());
        assert!(matches!(
            read_json_frame::<_, Value>(&mut payload),
            Err(CodecError::TruncatedPayload {
                expected: 4,
                received: 2
            })
        ));
    }
}
