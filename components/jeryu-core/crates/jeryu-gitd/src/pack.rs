//! Pack protocol adapters.

use crate::command::{
    StreamingCommand, run_capture_with_env, run_with_stdin_with_env, spawn_streaming_with_env,
};
use crate::error::{GitdError, Result};
use crate::repo::Repository;
use std::collections::BTreeSet;
use std::io::Read;

const MAIN_REF: &str = "refs/heads/main";
const MAX_RECEIVE_PACK_PRELUDE_BYTES: usize = 1024 * 1024;
const SHA1_HEX_LEN: usize = 40;

/// Git pack service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackService {
    /// `git-upload-pack`.
    UploadPack,
    /// `git-receive-pack`.
    ReceivePack,
}

impl PackService {
    /// HTTP service name.
    #[must_use]
    pub fn http_name(self) -> &'static str {
        match self {
            Self::UploadPack => "git-upload-pack",
            Self::ReceivePack => "git-receive-pack",
        }
    }

    /// Whether the service mutates the repository (receive-pack / push).
    #[must_use]
    pub fn is_write(self) -> bool {
        matches!(self, Self::ReceivePack)
    }

    /// Git subcommand.
    #[must_use]
    pub fn git_subcommand(self) -> &'static str {
        match self {
            Self::UploadPack => "upload-pack",
            Self::ReceivePack => "receive-pack",
        }
    }

    /// Parse a service name.
    #[must_use]
    pub fn parse(service: &str) -> Option<Self> {
        match service {
            "git-upload-pack" => Some(Self::UploadPack),
            "git-receive-pack" => Some(Self::ReceivePack),
            _ => None,
        }
    }
}

/// Advertise refs for smart HTTP.
pub fn advertise_refs(git_bin: &str, repo: &Repository, service: PackService) -> Result<Vec<u8>> {
    advertise_refs_with_protocol(git_bin, repo, service, None)
}

/// Advertise refs using the optional validated smart-HTTP protocol version.
pub(crate) fn advertise_refs_with_protocol(
    git_bin: &str,
    repo: &Repository,
    service: PackService,
    git_protocol: Option<&str>,
) -> Result<Vec<u8>> {
    let path = repo.path.to_string_lossy().to_string();
    let command = service.git_subcommand();
    let env = git_protocol.map(|value| [("GIT_PROTOCOL", value)]);
    let out = run_capture_with_env(
        git_bin,
        &[command, "--stateless-rpc", "--advertise-refs", &path],
        None,
        env.as_ref().map_or(&[], |values| values.as_slice()),
    )?;
    Ok(out.stdout)
}

/// Execute a stateless RPC exchange.
pub fn stateless_rpc(
    git_bin: &str,
    repo: &Repository,
    service: PackService,
    body: &[u8],
) -> Result<Vec<u8>> {
    stateless_rpc_with_protocol(git_bin, repo, service, body, None)
}

/// Execute a materialized stateless RPC using the optional validated protocol
/// version. Production pack traffic uses [`spawn_stateless_rpc`] instead.
pub(crate) fn stateless_rpc_with_protocol(
    git_bin: &str,
    repo: &Repository,
    service: PackService,
    body: &[u8],
    git_protocol: Option<&str>,
) -> Result<Vec<u8>> {
    let path = repo.path.to_string_lossy().to_string();
    let command = service.git_subcommand();
    let env = git_protocol.map(|value| [("GIT_PROTOCOL", value)]);
    let out = run_with_stdin_with_env(
        git_bin,
        &[command, "--stateless-rpc", &path],
        body,
        None,
        env.as_ref().map_or(&[], |values| values.as_slice()),
    )?;
    Ok(out.stdout)
}

/// Spawn a stateless Git RPC whose stdin/stdout will be pumped by the caller.
///
/// This is the production smart-HTTP path. Unlike [`stateless_rpc`], it does
/// not accept or return an in-memory pack buffer.
pub(crate) fn spawn_stateless_rpc(
    git_bin: &str,
    repo: &Repository,
    service: PackService,
    git_protocol: Option<&str>,
) -> Result<StreamingCommand> {
    let path = repo.path.to_string_lossy().to_string();
    let env = git_protocol.map(|value| [("GIT_PROTOCOL", value)]);
    spawn_streaming_with_env(
        git_bin,
        &[service.git_subcommand(), "--stateless-rpc", &path],
        None,
        env.as_ref().map_or(&[], |values| values.as_slice()),
    )
}

/// A receive-pack ref update command from the request prelude.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivePackCommand {
    /// Previous oid supplied by the client.
    pub old_oid: String,
    /// New oid supplied by the client.
    pub new_oid: String,
    /// Ref name supplied by the client.
    pub ref_name: String,
}

/// Parse receive-pack update commands from a stateless request body.
///
/// A shallow client may prefix the command records with `shallow <oid>`
/// declarations. The declarations remain in the original request body for Git;
/// this parser validates and skips them only for protected-ref evaluation. It
/// deliberately stops at the first flush so it never attempts to pkt-line
/// decode the packfile payload.
pub fn receive_pack_commands(mut input: &[u8]) -> Result<Vec<ReceivePackCommand>> {
    let mut commands = Vec::new();
    let mut first_command = true;
    let mut shallow_oids = BTreeSet::new();
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
            0 => break,
            1 | 2 => {
                return Err(GitdError::Protocol(
                    "unexpected control packet in receive-pack prelude".to_string(),
                ));
            }
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
                let payload = &input[..payload_len];
                input = &input[payload_len..];

                let line = std::str::from_utf8(payload).map_err(|_| {
                    GitdError::Protocol("receive-pack prelude record is not valid utf8".to_string())
                })?;
                let line = line.strip_suffix('\n').unwrap_or(line);

                if let Some(shallow_oid) = line.strip_prefix("shallow ") {
                    if !first_command {
                        return Err(GitdError::Protocol(
                            "shallow declaration follows receive-pack command".to_string(),
                        ));
                    }
                    if !is_lowercase_sha1(shallow_oid) {
                        return Err(GitdError::Protocol(
                            "invalid shallow declaration oid".to_string(),
                        ));
                    }
                    if !shallow_oids.insert(shallow_oid.to_string()) {
                        return Err(GitdError::Protocol(
                            "duplicate shallow declaration".to_string(),
                        ));
                    }
                    continue;
                }

                let command = if first_command {
                    line.split_once('\0').map_or(line, |(command, _)| command)
                } else {
                    if line.contains('\0') {
                        return Err(GitdError::Protocol(
                            "capabilities appear after first receive-pack command".to_string(),
                        ));
                    }
                    line
                };
                let mut parts = command.split(' ');
                let Some(old_oid) = parts.next() else {
                    return Err(GitdError::Protocol(
                        "receive-pack command missing old oid".to_string(),
                    ));
                };
                let Some(new_oid) = parts.next() else {
                    return Err(GitdError::Protocol(
                        "receive-pack command missing new oid".to_string(),
                    ));
                };
                let Some(ref_name) = parts.next() else {
                    return Err(GitdError::Protocol(
                        "receive-pack command missing ref name".to_string(),
                    ));
                };
                if parts.next().is_some()
                    || !is_lowercase_sha1(old_oid)
                    || !is_lowercase_sha1(new_oid)
                    || !ref_name.starts_with("refs/")
                    || ref_name.len() == "refs/".len()
                {
                    return Err(GitdError::Protocol(
                        "invalid receive-pack command record".to_string(),
                    ));
                }
                first_command = false;
                commands.push(ReceivePackCommand {
                    old_oid: old_oid.to_string(),
                    new_oid: new_oid.to_string(),
                    ref_name: ref_name.to_string(),
                });
            }
        }
    }
    Ok(commands)
}

fn is_lowercase_sha1(value: &str) -> bool {
    value.len() == SHA1_HEX_LEN
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Enforce the PR-only trunk policy before invoking Git receive-pack.
pub fn ensure_receive_pack_policy(body: &[u8]) -> Result<()> {
    for command in receive_pack_commands(body)? {
        if command.ref_name == MAIN_REF {
            return Err(GitdError::ProtectedRefDenied(
                "direct pushes to refs/heads/main are blocked; open a pull request and merge through Jeryu".to_string(),
            ));
        }
    }
    Ok(())
}

/// Read only the bounded receive-pack command prelude needed for protected-ref
/// policy. Any bytes already read beyond the flush packet are retained so the
/// child receives the request byte-for-byte.
pub(crate) fn read_receive_pack_prefix(
    reader: &mut impl Read,
    content_length: u64,
    mut prefix: Vec<u8>,
) -> Result<Vec<u8>> {
    let prefix_len = u64::try_from(prefix.len())
        .map_err(|_| GitdError::Protocol("receive-pack prefix is too large".to_string()))?;
    if prefix_len > content_length {
        return Err(GitdError::Protocol(
            "receive-pack prefix exceeds Content-Length".to_string(),
        ));
    }

    loop {
        match receive_pack_prelude_state(&prefix)? {
            PreludeState::Complete(prelude_len) => {
                if prelude_len > MAX_RECEIVE_PACK_PRELUDE_BYTES {
                    return Err(GitdError::Protocol(
                        "receive-pack command prelude exceeds limit".to_string(),
                    ));
                }
                ensure_receive_pack_policy(&prefix[..prelude_len])?;
                return Ok(prefix);
            }
            PreludeState::Need(needed) => {
                let next_len = prefix.len().checked_add(needed).ok_or_else(|| {
                    GitdError::Protocol("receive-pack prelude length overflow".to_string())
                })?;
                if next_len > MAX_RECEIVE_PACK_PRELUDE_BYTES {
                    return Err(GitdError::Protocol(
                        "receive-pack command prelude exceeds limit".to_string(),
                    ));
                }
                if u64::try_from(next_len).unwrap_or(u64::MAX) > content_length {
                    return Err(GitdError::Protocol(
                        "receive-pack command prelude is truncated".to_string(),
                    ));
                }
                let current = prefix.len();
                prefix.resize(next_len, 0);
                reader.read_exact(&mut prefix[current..]).map_err(|err| {
                    GitdError::Protocol(format!("receive-pack command prelude is truncated: {err}"))
                })?;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreludeState {
    Complete(usize),
    Need(usize),
}

fn receive_pack_prelude_state(input: &[u8]) -> Result<PreludeState> {
    let mut offset = 0usize;
    loop {
        let remaining = input.len().saturating_sub(offset);
        if remaining < 4 {
            return Ok(PreludeState::Need(4 - remaining));
        }
        let hdr = std::str::from_utf8(&input[offset..offset + 4])
            .map_err(|_| GitdError::Protocol("pkt-line length is not utf8".to_string()))?;
        let len = usize::from_str_radix(hdr, 16)
            .map_err(|_| GitdError::Protocol(format!("invalid pkt-line length: {hdr}")))?;
        match len {
            0 => return Ok(PreludeState::Complete(offset + 4)),
            1 | 2 => offset += 4,
            3 => {
                return Err(GitdError::Protocol(
                    "reserved pkt-line length 0003".to_string(),
                ));
            }
            n => {
                if n < 4 {
                    return Err(GitdError::Protocol("pkt-line underflow".to_string()));
                }
                if remaining < n {
                    return Ok(PreludeState::Need(n - remaining));
                }
                offset += n;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pktline;

    #[test]
    fn receive_pack_policy_rejects_main_before_pack_payload() {
        let mut body = pktline::encode_str(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/main\0 report-status\n",
        );
        body.extend(pktline::flush());
        body.extend(b"PACK fake bytes that are not pkt-lines");

        let err = ensure_receive_pack_policy(&body).unwrap_err();

        assert!(err.to_string().contains("direct pushes to refs/heads/main"));
    }

    #[test]
    fn receive_pack_parser_keeps_non_main_commands() {
        let mut body = pktline::encode_str(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/feature\0 report-status\n",
        );
        body.extend(pktline::encode_str(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb cccccccccccccccccccccccccccccccccccccccc refs/heads/topic\n",
        ));
        body.extend(pktline::flush());
        body.extend(b"PACK");

        let commands = receive_pack_commands(&body).expect("parse receive-pack commands");

        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].ref_name, "refs/heads/feature");
        assert_eq!(commands[1].ref_name, "refs/heads/topic");
        ensure_receive_pack_policy(&body).expect("non-main updates are allowed");
    }

    #[test]
    fn receive_pack_parser_accepts_leading_shallow_declarations() {
        let mut body = pktline::encode_str("shallow aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n");
        body.extend(pktline::encode_str(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/feature\0 report-status\n",
        ));
        body.extend(pktline::flush());
        body.extend(b"PACK");

        let commands = receive_pack_commands(&body).expect("parse shallow receive-pack");

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].ref_name, "refs/heads/feature");
        ensure_receive_pack_policy(&body).expect("shallow topic update is allowed");
    }

    #[test]
    fn receive_pack_policy_rejects_main_after_shallow_declaration() {
        let mut body = pktline::encode_str("shallow aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n");
        body.extend(pktline::encode_str(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/main\0 report-status\n",
        ));
        body.extend(pktline::flush());

        let err = ensure_receive_pack_policy(&body).expect_err("main update must be denied");

        assert!(err.to_string().contains("direct pushes to refs/heads/main"));
    }

    #[test]
    fn receive_pack_parser_rejects_hostile_shallow_declarations() {
        for (label, declarations) in [
            (
                "short",
                vec!["shallow aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"],
            ),
            (
                "uppercase",
                vec!["shallow AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n"],
            ),
            (
                "extra field",
                vec!["shallow aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa extra\n"],
            ),
            (
                "duplicate",
                vec![
                    "shallow aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
                    "shallow aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
                ],
            ),
        ] {
            let mut body = Vec::new();
            for declaration in declarations {
                body.extend(pktline::encode_str(declaration));
            }
            body.extend(pktline::encode_str(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/topic\0 report-status\n",
            ));
            body.extend(pktline::flush());

            assert!(
                receive_pack_commands(&body).is_err(),
                "{label} declaration must fail closed"
            );
        }
    }

    #[test]
    fn receive_pack_parser_rejects_late_or_unknown_prelude_records() {
        let command = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/topic\0 report-status\n";
        let mut late = pktline::encode_str(command);
        late.extend(pktline::encode_str(
            "shallow cccccccccccccccccccccccccccccccccccccccc\n",
        ));
        late.extend(pktline::flush());

        let mut unknown = pktline::encode_str("deepen 1\n");
        unknown.extend(pktline::encode_str(command));
        unknown.extend(pktline::flush());

        assert!(receive_pack_commands(&late).is_err());
        assert!(receive_pack_commands(&unknown).is_err());
    }

    #[test]
    fn receive_pack_prefix_preserves_shallow_declaration_bytes() {
        let mut body = pktline::encode_str("shallow aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n");
        body.extend(pktline::encode_str(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/feature\0 report-status\n",
        ));
        body.extend(pktline::flush());
        let prelude_len = body.len();
        body.extend(b"PACK payload remains in the reader");
        let mut reader = OneByteReader::new(&body);

        let prefix = read_receive_pack_prefix(&mut reader, body.len() as u64, Vec::new())
            .unwrap_or_else(|err| panic!("read shallow prelude: {err}"));

        assert_eq!(prefix, body[..prelude_len]);
        assert_eq!(reader.remaining(), &body[prelude_len..]);
    }

    #[test]
    fn receive_pack_prefix_reads_fragmented_prelude_without_pack_payload() {
        let mut body = pktline::encode_str(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/feature\0 report-status\n",
        );
        body.extend(pktline::flush());
        let prelude_len = body.len();
        body.extend(b"PACK payload remains in the reader");
        let mut reader = OneByteReader::new(&body);

        let prefix = read_receive_pack_prefix(&mut reader, body.len() as u64, Vec::new())
            .unwrap_or_else(|err| panic!("read prelude: {err}"));

        assert_eq!(prefix, body[..prelude_len]);
        assert_eq!(reader.remaining(), &body[prelude_len..]);
    }

    #[test]
    fn receive_pack_prefix_denies_main_before_spawn() {
        let mut body = pktline::encode_str(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/main\0 report-status\n",
        );
        body.extend(pktline::flush());
        body.extend(b"PACK");
        let mut reader = OneByteReader::new(&body);

        let err = read_receive_pack_prefix(&mut reader, body.len() as u64, Vec::new())
            .expect_err("main update must be denied");

        assert!(err.to_string().contains("direct pushes to refs/heads/main"));
    }

    #[test]
    fn receive_pack_prefix_rejects_unbounded_command_section() {
        let packet = std::iter::repeat_n(b'x', 65_531).collect::<Vec<_>>();
        let mut body = Vec::new();
        for _ in 0..17 {
            body.extend(b"ffff");
            body.extend(&packet);
        }
        let mut reader = &body[..];

        let err = read_receive_pack_prefix(&mut reader, body.len() as u64, Vec::new())
            .expect_err("oversized prelude must be rejected");

        assert!(err.to_string().contains("prelude exceeds limit"));
    }

    #[test]
    fn receive_pack_prefix_rejects_truncated_command_section() {
        let body = pktline::encode_str(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb refs/heads/feature\n",
        );
        let mut reader = &body[..];

        let err = read_receive_pack_prefix(&mut reader, body.len() as u64, Vec::new())
            .expect_err("missing flush packet must be rejected");

        assert!(err.to_string().contains("prelude is truncated"));
    }

    struct OneByteReader<'a> {
        input: &'a [u8],
        offset: usize,
    }

    impl<'a> OneByteReader<'a> {
        fn new(input: &'a [u8]) -> Self {
            Self { input, offset: 0 }
        }

        fn remaining(&self) -> &'a [u8] {
            &self.input[self.offset..]
        }
    }

    impl Read for OneByteReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.offset == self.input.len() || buffer.is_empty() {
                return Ok(0);
            }
            buffer[0] = self.input[self.offset];
            self.offset += 1;
            Ok(1)
        }
    }
}
