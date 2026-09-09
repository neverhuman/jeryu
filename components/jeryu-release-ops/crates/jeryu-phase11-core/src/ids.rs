//! Identifier, digest, version, and enumeration domain types.

use std::fmt::{self, Display};

use crate::validation::{ValidationError, validate_slug};

/// Phase 11 tenant identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TenantId(String);

impl TenantId {
    /// Construct a validated tenant id.
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        validate_slug("tenant_id", &value)?;
        Ok(Self(value))
    }

    /// Borrow the tenant id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable digest wrapper. This is not a cryptographic replacement; it is an internal deterministic identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Digest(String);

impl Digest {
    /// Build a digest from a stable byte sequence using a deterministic FNV-1a variant.
    pub fn from_bytes(prefix: &str, bytes: &[u8]) -> Self {
        let mut state: u64 = 0xcbf29ce484222325;
        for b in bytes {
            state ^= u64::from(*b);
            state = state.wrapping_mul(0x100000001b3);
        }
        Self(format!("{}:{:016x}", prefix, state))
    }

    /// Build a digest from text.
    pub fn from_text(prefix: &str, text: &str) -> Self {
        Self::from_bytes(prefix, text.as_bytes())
    }

    /// Borrow the stable digest string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A named semantic version used by lifecycle plans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl Version {
    /// Construct a version.
    pub fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parse a dotted `MAJOR.MINOR.PATCH` version.
    pub fn parse(input: &str) -> Result<Self, ValidationError> {
        let mut parts = input.split('.');
        let major = parse_u16("major", parts.next())?;
        let minor = parse_u16("minor", parts.next())?;
        let patch = parse_u16("patch", parts.next())?;
        if parts.next().is_some() {
            return Err(ValidationError::new("version", "too many components"));
        }
        Ok(Self::new(major, minor, patch))
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

fn parse_u16(field: &'static str, value: Option<&str>) -> Result<u16, ValidationError> {
    let raw = value.ok_or_else(|| ValidationError::new(field, "missing"))?;
    raw.parse::<u16>()
        .map_err(|_| ValidationError::new(field, "not a u16"))
}

/// Upgrade rollout ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UpgradeRing {
    Dev,
    Canary,
    Internal,
    Enterprise,
    Global,
}

impl UpgradeRing {
    /// Ordered list of all rings.
    pub fn ordered() -> [Self; 5] {
        [
            Self::Dev,
            Self::Canary,
            Self::Internal,
            Self::Enterprise,
            Self::Global,
        ]
    }

    /// Stable string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Canary => "canary",
            Self::Internal => "internal",
            Self::Enterprise => "enterprise",
            Self::Global => "global",
        }
    }
}

/// Export format for compliance artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Markdown,
    Bundle,
}

impl ExportFormat {
    /// Stable string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Markdown => "markdown",
            Self::Bundle => "bundle",
        }
    }
}
