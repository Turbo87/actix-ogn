use bytes::{Buf, BufMut, BytesMut};
use encoding_rs::WINDOWS_1252;
use std::{cmp, fmt, io, str};
use tokio_util::codec::{Decoder, Encoder};

fn utf8(buf: &[u8]) -> String {
    let utf8_attempt = str::from_utf8(buf);
    if let Ok(utf8_value) = utf8_attempt {
        // Probably plain ASCII text or a station that uses UTF-8 for name encoding
        utf8_value.to_string()
    } else {
        // Some stations use CP 1252 for encoding their name, so try that as fallback when the
        // message contained a sequence of bytes that is not valid UTF-8. This always succeeds.
        let (s, _) = WINDOWS_1252.decode_without_bom_handling(buf);
        s.to_string()
    }
}

/// A simple [`Decoder`] and [`Encoder`] implementation which splits up OGN messages by \r\n.
/// It was derived from tokio-util's LinesCodec.
pub struct OGNCodec {
    next_index: usize,
    is_discarding: bool,
}

impl OGNCodec {
    pub fn new() -> OGNCodec {
        OGNCodec {
            next_index: 0,
            is_discarding: false,
        }
    }

    pub fn max_length(&self) -> usize {
        // An arbitrary but sane line limit to avoid memory exhaustion when no \r\n is ever received
        4096
    }
}

impl Decoder for OGNCodec {
    type Item = String;
    type Error = OGNCodecError;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<String>, OGNCodecError> {
        loop {
            // Find start of \r\n in up to the next 256 characters
            let mut crlf_offset: Option<usize> = None;
            let read_to = cmp::min(self.max_length().saturating_add(2), buf.len());
            if read_to == 0 {
                return Ok(None);
            }
            for i in (self.next_index + 1)..read_to {
                if buf[i - 1] == b'\r' && buf[i] == b'\n' {
                    crlf_offset = Some(i - 1);
                    break;
                }
            }

            match (self.is_discarding, crlf_offset) {
                (true, Some(offset)) => {
                    // (via LinesCodec)
                    buf.advance(offset + 2);
                    self.is_discarding = false;
                    self.next_index = 0;
                }
                (true, None) => {
                    // (via LinesCodec)
                    buf.advance(read_to);
                    self.next_index = 0;
                    if buf.is_empty() {
                        return Ok(None);
                    }
                }
                (false, Some(offset)) => {
                    // Found a line!
                    self.next_index = 0;
                    let line = buf.split_to(offset + 2);
                    let line = &line[..line.len() - 2];
                    return Ok(Some(utf8(line).to_string()));
                }
                (false, None) if buf.len() > self.max_length() => {
                    // (via LinesCodec)
                    self.is_discarding = true;
                    return Err(OGNCodecError::MaxLineLengthExceeded);
                }
                (false, None) => {
                    // We didn't find a line or reach the length limit, so the next call will
                    // resume searching at the current offset minus one to account for a trailing
                    // \r character.
                    self.next_index = read_to - 1;
                    return Ok(None);
                }
            }
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<String>, OGNCodecError> {
        Ok(match self.decode(buf)? {
            Some(frame) => Some(frame),
            None => {
                // No terminating newline - return remaining data, if any
                if buf.is_empty() || buf == &b"\r"[..] {
                    None
                } else {
                    self.next_index = 0;
                    Some(utf8(buf).to_string())
                }
            }
        })
    }
}

impl<T> Encoder<T> for OGNCodec
where
    T: AsRef<str>,
{
    type Error = OGNCodecError;

    fn encode(&mut self, line: T, buf: &mut BytesMut) -> Result<(), OGNCodecError> {
        let line = line.as_ref();
        buf.reserve(line.len() + 2);
        buf.put(line.as_bytes());
        buf.put_u8(b'\r');
        buf.put_u8(b'\n');
        Ok(())
    }
}

#[derive(Debug)]
pub enum OGNCodecError {
    /// The maximum line length was exceeded.
    MaxLineLengthExceeded,
    /// An IO error occurred.
    Io(io::Error),
}

impl fmt::Display for OGNCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OGNCodecError::MaxLineLengthExceeded => write!(f, "max line length exceeded"),
            OGNCodecError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl From<io::Error> for OGNCodecError {
    fn from(e: io::Error) -> OGNCodecError {
        OGNCodecError::Io(e)
    }
}

impl std::error::Error for OGNCodecError {}
