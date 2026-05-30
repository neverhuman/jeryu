//! Git pkt-line encoding and decoding.

use crate::error::{GitdError, Result};

/// A decoded pkt-line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PktLine {
    /// Flush packet `0000`.
    Flush,
    /// Delimiter packet `0001`.
    Delim,
    /// Response end packet `0002`.
    ResponseEnd,
    /// Data packet.
    Data(Vec<u8>),
}

/// Encode data as a pkt-line.
#[must_use]
pub fn encode(data: &[u8]) -> Vec<u8> {
    let len = data.len() + 4;
    let mut out = format!("{len:04x}").into_bytes();
    out.extend_from_slice(data);
    out
}

/// Encode a text pkt-line.
#[must_use]
pub fn encode_str(data: &str) -> Vec<u8> {
    encode(data.as_bytes())
}

/// Flush packet.
#[must_use]
pub fn flush() -> Vec<u8> {
    b"0000".to_vec()
}

/// Decode all pkt-lines in `input`.
pub fn decode_all(mut input: &[u8]) -> Result<Vec<PktLine>> {
    let mut lines = Vec::new();
    while !input.is_empty() {
        if input.len() < 4 {
            return Err(GitdError::Protocol("pkt-line missing length".to_string()));
        }
        let hdr = std::str::from_utf8(&input[..4])
            .map_err(|_| GitdError::Protocol("pkt-line length is not utf8".to_string()))?;
        let len = usize::from_str_radix(hdr, 16)
            .map_err(|_| GitdError::Protocol(format!("invalid pkt-line length: {hdr}")))?;
        input = &input[4..];
        match len {
            0 => lines.push(PktLine::Flush),
            1 => lines.push(PktLine::Delim),
            2 => lines.push(PktLine::ResponseEnd),
            3 => {
                return Err(GitdError::Protocol(
                    "reserved pkt-line length 0003".to_string(),
                ));
            }
            n => {
                let payload_len = n
                    .checked_sub(4)
                    .ok_or_else(|| GitdError::Protocol("pkt-line underflow".to_string()))?;
                if input.len() < payload_len {
                    return Err(GitdError::Protocol(
                        "pkt-line payload truncated".to_string(),
                    ));
                }
                lines.push(PktLine::Data(input[..payload_len].to_vec()));
                input = &input[payload_len..];
            }
        }
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pktline_round_trip() {
        let mut data = encode_str("version 2\n");
        data.extend(flush());
        let decoded = decode_all(&data).unwrap_or_else(|err| panic!("decode failed: {err}"));
        assert_eq!(
            decoded,
            vec![PktLine::Data(b"version 2\n".to_vec()), PktLine::Flush]
        );
    }
}
