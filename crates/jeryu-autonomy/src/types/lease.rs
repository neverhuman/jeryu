//! Canonical object 2: the Capability Lease — a signed, time-boxed grant
//! authorizing an agent to perform scoped actions, with a pre-flight `permits`
//! check and a minimal glob matcher for `denied_paths`.

use super::schema_tag::{CapabilityLeaseTag, SchemaTag};
use crate::signing::Signature;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LeaseScope {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_write_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityLease {
    #[serde(rename = "schema")]
    pub schema: SchemaTag<CapabilityLeaseTag>,
    pub id: String,
    pub intent_id: String,
    pub agent_id: String,
    pub scope: LeaseScope,
    pub ttl_seconds: u32,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub policy_sha: String,
    pub signature: Signature,
}

impl CapabilityLease {
    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }

    /// Pre-flight check: may this lease perform `action` touching `paths`, run
    /// by `agent_id`, at time `now`?
    ///
    /// Refuses on: expired, agent_id mismatch, action not in allowed_actions,
    /// action explicitly in denied_actions, or any path matching denied_paths.
    pub fn permits(
        &self,
        action: &str,
        agent_id: &str,
        paths: &[&str],
        now: DateTime<Utc>,
    ) -> Result<(), LeaseDenied> {
        if self.is_expired_at(now) {
            return Err(LeaseDenied::Expired {
                expired_at: self.expires_at,
                now,
            });
        }
        if self.agent_id != agent_id {
            return Err(LeaseDenied::AgentIdMismatch {
                lease_agent: self.agent_id.clone(),
                request_agent: agent_id.to_string(),
            });
        }
        if self.scope.denied_actions.iter().any(|a| a == action) {
            return Err(LeaseDenied::ActionDenied(action.to_string()));
        }
        if !self.scope.allowed_actions.is_empty()
            && !self.scope.allowed_actions.iter().any(|a| a == action)
        {
            return Err(LeaseDenied::ActionNotAllowed(action.to_string()));
        }
        for p in paths {
            for denied in &self.scope.denied_paths {
                if path_matches_glob(denied, p) {
                    return Err(LeaseDenied::PathDenied {
                        path: (*p).to_string(),
                        pattern: denied.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseDenied {
    Expired {
        expired_at: DateTime<Utc>,
        now: DateTime<Utc>,
    },
    AgentIdMismatch {
        lease_agent: String,
        request_agent: String,
    },
    ActionDenied(String),
    ActionNotAllowed(String),
    PathDenied {
        path: String,
        pattern: String,
    },
}

impl std::fmt::Display for LeaseDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LeaseDenied::Expired { expired_at, now } => {
                write!(f, "lease expired at {expired_at}; now {now}")
            }
            LeaseDenied::AgentIdMismatch {
                lease_agent,
                request_agent,
            } => write!(
                f,
                "lease was issued to '{lease_agent}'; request came from '{request_agent}'"
            ),
            LeaseDenied::ActionDenied(a) => write!(f, "action '{a}' explicitly denied by lease"),
            LeaseDenied::ActionNotAllowed(a) => {
                write!(f, "action '{a}' not in lease's allowed_actions allowlist")
            }
            LeaseDenied::PathDenied { path, pattern } => {
                write!(f, "path '{path}' matches denied pattern '{pattern}'")
            }
        }
    }
}

impl std::error::Error for LeaseDenied {}

/// Minimal glob matcher for lease denied_paths: `*` within segment, `**`
/// across segments. Anchored at root (no `/` prefix needed).
fn path_matches_glob(pattern: &str, path: &str) -> bool {
    glob_inner(pattern.as_bytes(), 0, path.as_bytes(), 0)
}

fn glob_inner(p: &[u8], pi: usize, s: &[u8], si: usize) -> bool {
    let mut pi = pi;
    let mut si = si;
    while pi < p.len() {
        if p[pi] == b'*' {
            let double = pi + 1 < p.len() && p[pi + 1] == b'*';
            if double {
                pi += 2;
                if pi < p.len() && p[pi] == b'/' {
                    pi += 1;
                }
                if pi >= p.len() {
                    return true;
                }
                for try_si in si..=s.len() {
                    if glob_inner(p, pi, s, try_si) {
                        return true;
                    }
                }
                return false;
            } else {
                pi += 1;
                if pi >= p.len() {
                    return !s[si..].contains(&b'/');
                }
                let limit = s[si..]
                    .iter()
                    .position(|c| *c == b'/')
                    .map(|n| si + n)
                    .unwrap_or(s.len());
                for try_si in si..=limit {
                    if glob_inner(p, pi, s, try_si) {
                        return true;
                    }
                }
                return false;
            }
        } else if si < s.len() && p[pi] == s[si] {
            pi += 1;
            si += 1;
        } else {
            return false;
        }
    }
    si == s.len()
}
