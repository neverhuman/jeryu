//! Git smart-protocol (wire version 2) request parsing and capability advertisement.

use crate::error::{GitdError, Result};
use crate::pktline::{PktLine, decode_all, encode_str, flush};

/// Capabilities Jeryu advertises in Phase 1.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilitySet {
    /// Server agent string.
    pub agent: String,
    /// Whether `ls-refs` is supported.
    pub ls_refs: bool,
    /// Whether `fetch` is supported.
    pub fetch: bool,
    /// Whether server supports object-info extension.
    pub object_info: bool,
}

impl Default for CapabilitySet {
    fn default() -> Self {
        Self {
            agent: "jeryu_gitd/0.1 phase1".to_string(),
            ls_refs: true,
            fetch: true,
            object_info: false,
        }
    }
}

impl CapabilitySet {
    /// Encode a protocol v2 capability advertisement.
    #[must_use]
    pub fn advertise(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(encode_str("version 2\n"));
        out.extend(encode_str(&format!("agent={}\n", self.agent)));
        if self.ls_refs {
            out.extend(encode_str("ls-refs=unborn\n"));
        }
        if self.fetch {
            out.extend(encode_str("fetch=shallow wait-for-done filter\n"));
        }
        if self.object_info {
            out.extend(encode_str("object-info\n"));
        }
        out.extend(flush());
        out
    }
}

/// Parsed protocol v2 command request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct V2Request {
    /// Command such as `ls-refs` or `fetch`.
    pub command: String,
    /// Capability or argument lines following the command.
    pub lines: Vec<String>,
}

/// Parse a protocol v2 request body.
pub fn parse_request(input: &[u8]) -> Result<V2Request> {
    let packets = decode_all(input)?;
    let mut command = None;
    let mut lines = Vec::new();
    for packet in packets {
        match packet {
            PktLine::Data(data) => {
                let text = String::from_utf8(data)
                    .map_err(|_| GitdError::Protocol("v2 request line is not utf8".to_string()))?;
                let text = text.trim_end_matches('\n').to_string();
                if let Some(rest) = text.strip_prefix("command=") {
                    command = Some(rest.to_string());
                } else if !text.is_empty() {
                    lines.push(text);
                }
            }
            PktLine::Flush | PktLine::Delim | PktLine::ResponseEnd => {}
        }
    }
    let command = command
        .ok_or_else(|| GitdError::Protocol("protocol v2 request missing command".to_string()))?;
    Ok(V2Request { command, lines })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pktline;

    #[test]
    fn smart_protocol_advertises_core_caps() {
        let ad = CapabilitySet::default().advertise();
        let text = String::from_utf8_lossy(&ad);
        assert!(text.contains("version 2"));
        assert!(text.contains("ls-refs"));
        assert!(text.contains("fetch"));
    }

    #[test]
    fn smart_protocol_parses_command() {
        let mut body = pktline::encode_str("command=ls-refs\n");
        body.extend(pktline::flush());
        body.extend(pktline::encode_str("peel\n"));
        body.extend(pktline::flush());
        let req = parse_request(&body).unwrap_or_else(|err| panic!("parse failed: {err}"));
        assert_eq!(req.command, "ls-refs");
        assert_eq!(req.lines, vec!["peel"]);
    }
}
