//! Append-only cache receipts.

use crate::cache::{now_nanos, CacheKey};
use crate::digest::{digest_bytes, Digest};
use crate::error::Result;
use crate::ids::Actor;
use crate::policy::{CacheDisposition, PolicyDecision, TrustTier};
use std::fmt::{Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Receipt action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheAction {
    /// Cache read attempt.
    Read,
    /// Cache write attempt.
    Write,
    /// Cache promotion attempt.
    Promote,
    /// Quarantine event.
    Quarantine,
    /// False hit or poison event.
    PoisonDetected,
    /// Safe cache miss event.
    SafeMiss,
    /// Policy denial event.
    Denied,
}

impl Display for CacheAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            CacheAction::Read => "read",
            CacheAction::Write => "write",
            CacheAction::Promote => "promote",
            CacheAction::Quarantine => "quarantine",
            CacheAction::PoisonDetected => "poison-detected",
            CacheAction::SafeMiss => "safe-miss",
            CacheAction::Denied => "denied",
        };
        f.write_str(value)
    }
}

/// A signed-in-spirit receipt record for every cache decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheReceipt {
    /// Receipt ID.
    pub id: Digest,
    /// Timestamp.
    pub timestamp_nanos: u128,
    /// Actor.
    pub actor: Actor,
    /// Action.
    pub action: CacheAction,
    /// Cache key digest.
    pub cache_key_digest: Digest,
    /// Optional object digest.
    pub object_digest: Option<Digest>,
    /// Decision disposition.
    pub disposition: CacheDisposition,
    /// Cache law.
    pub law: String,
    /// Reason.
    pub reason: String,
    /// Trust tier.
    pub trust_tier: TrustTier,
}

impl CacheReceipt {
    /// Creates a receipt.
    pub fn new(
        actor: Actor,
        action: CacheAction,
        key: &CacheKey,
        object_digest: Option<Digest>,
        decision: &PolicyDecision,
        trust_tier: TrustTier,
    ) -> Self {
        let timestamp_nanos = now_nanos();
        let cache_key_digest = key.digest();
        let seed = format!(
            "ts={timestamp_nanos}\nactor={actor}\naction={action}\nkey={cache_key_digest}\nobject={:?}\ndisposition={}\nlaw={}\nreason={}\ntrust={}\n",
            object_digest.as_ref().map(Digest::as_str),
            decision.disposition,
            decision.law,
            decision.reason,
            trust_tier
        );
        let id = digest_bytes(seed.as_bytes());
        Self {
            id,
            timestamp_nanos,
            actor,
            action,
            cache_key_digest,
            object_digest,
            disposition: decision.disposition,
            law: decision.law.clone(),
            reason: decision.reason.clone(),
            trust_tier,
        }
    }

    /// Serializes as stable JSON.
    pub fn to_json(&self) -> String {
        let object = self
            .object_digest
            .as_ref()
            .map(|d| format!("\"{}\"", escape(d.as_str())))
            .unwrap_or_else(|| "null".to_string());
        format!(
            concat!(
                "{{\n",
                "  \"id\": \"{}\",\n",
                "  \"timestamp_nanos\": {},\n",
                "  \"actor\": \"{}\",\n",
                "  \"action\": \"{}\",\n",
                "  \"cache_key_digest\": \"{}\",\n",
                "  \"object_digest\": {},\n",
                "  \"disposition\": \"{}\",\n",
                "  \"law\": \"{}\",\n",
                "  \"reason\": \"{}\",\n",
                "  \"trust_tier\": \"{}\"\n",
                "}}"
            ),
            escape(self.id.as_str()),
            self.timestamp_nanos,
            escape(self.actor.as_str()),
            self.action,
            escape(self.cache_key_digest.as_str()),
            object,
            self.disposition,
            escape(&self.law),
            escape(&self.reason),
            self.trust_tier
        )
    }
}

/// Receipt log writer.
#[derive(Debug, Clone)]
pub struct ReceiptLog {
    root: PathBuf,
}

impl ReceiptLog {
    /// Opens a receipt log.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Appends a receipt to both individual file and NDJSON log.
    pub fn append(&self, receipt: &CacheReceipt) -> Result<()> {
        let json = receipt.to_json();
        fs::write(
            self.root.join(format!("{}.json", receipt.id.as_str())),
            &json,
        )?;
        let mut log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("log.ndjson"))?;
        log.write_all(compact_json(&json).as_bytes())?;
        log.write_all(b"\n")?;
        Ok(())
    }
}

fn compact_json(json: &str) -> String {
    json.lines().map(str::trim).collect::<Vec<_>>().join(" ")
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}
