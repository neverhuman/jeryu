#![forbid(unsafe_code)]
#![doc = "Shared Phase 11 domain types, deterministic hashing, and JSON helpers."]

use std::fmt::{self, Display};
use std::time::{SystemTime, UNIX_EPOCH};

/// Phase 11 release marker.
pub const PHASE: u8 = 11;
/// Product name used in receipts.
pub const PRODUCT: &str = "Jeryu";

/// Severity level for health, policy, and replay findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Degraded,
    Critical,
    Blocked,
}

impl Severity {
    /// Returns the stable lowercase representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Degraded => "degraded",
            Severity::Critical => "critical",
            Severity::Blocked => "blocked",
        }
    }

    /// Returns true when the severity should fail closed.
    pub fn fails_closed(self) -> bool {
        matches!(self, Severity::Critical | Severity::Blocked)
    }
}

impl Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// System health state used by operations automation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Healthy,
    Degraded,
    Failing,
    Blocked,
}

impl HealthState {
    /// Derive a state from the highest severity in a report.
    pub fn from_severity(severity: Severity) -> Self {
        match severity {
            Severity::Info | Severity::Warning => HealthState::Healthy,
            Severity::Degraded => HealthState::Degraded,
            Severity::Critical => HealthState::Failing,
            Severity::Blocked => HealthState::Blocked,
        }
    }

    /// Stable representation.
    pub fn as_str(self) -> &'static str {
        match self {
            HealthState::Healthy => "healthy",
            HealthState::Degraded => "degraded",
            HealthState::Failing => "failing",
            HealthState::Blocked => "blocked",
        }
    }
}

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

/// A generic validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    field: &'static str,
    message: &'static str,
}

impl ValidationError {
    /// Create a validation error.
    pub fn new(field: &'static str, message: &'static str) -> Self {
        Self { field, message }
    }

    /// Field that failed validation.
    pub fn field(&self) -> &'static str {
        self.field
    }

    /// Message for the failure.
    pub fn message(&self) -> &'static str {
        self.message
    }
}

impl Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// A finding emitted by any Phase 11 subsystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub receipt_id: Option<String>,
}

impl Finding {
    /// Construct a finding.
    pub fn new(code: impl Into<String>, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            receipt_id: None,
        }
    }

    /// Attach a receipt id.
    pub fn with_receipt(mut self, receipt_id: impl Into<String>) -> Self {
        self.receipt_id = Some(receipt_id.into());
        self
    }

    /// Stable JSON representation.
    pub fn to_json(&self) -> String {
        let receipt = self
            .receipt_id
            .as_ref()
            .map(|id| quote(id))
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"code\":{},\"severity\":{},\"message\":{},\"receipt_id\":{}}}",
            quote(&self.code),
            quote(self.severity.as_str()),
            quote(&self.message),
            receipt
        )
    }
}

/// Decision returned by policy gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow { reason: String, receipt_id: String },
    Deny { reason: String, finding: Finding },
}

impl PolicyDecision {
    /// True if the decision allows the action.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }
}

/// A compact report with deterministic ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub name: String,
    pub state: HealthState,
    pub findings: Vec<Finding>,
}

impl Report {
    /// New report.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            state: HealthState::Healthy,
            findings: Vec::new(),
        }
    }

    /// Add a finding and update state.
    pub fn push(&mut self, finding: Finding) {
        if finding.severity > self.highest_severity() {
            self.state = HealthState::from_severity(finding.severity);
        }
        self.findings.push(finding);
        self.findings.sort_by(|a, b| a.code.cmp(&b.code));
    }

    /// Highest severity in the report.
    pub fn highest_severity(&self) -> Severity {
        self.findings
            .iter()
            .map(|f| f.severity)
            .max()
            .unwrap_or(Severity::Info)
    }

    /// Fail closed when any finding is critical or blocked.
    pub fn fails_closed(&self) -> bool {
        self.highest_severity().fails_closed()
    }

    /// Stable JSON representation.
    pub fn to_json(&self) -> String {
        let findings = self
            .findings
            .iter()
            .map(Finding::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"name\":{},\"state\":{},\"highest_severity\":{},\"findings\":[{}]}}",
            quote(&self.name),
            quote(self.state.as_str()),
            quote(self.highest_severity().as_str()),
            findings
        )
    }
}

/// Stable timestamp seconds for receipt creation.
pub fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Escape a string as JSON.
pub fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Validate slug-like identifiers.
pub fn validate_slug(field: &'static str, value: &str) -> Result<(), ValidationError> {
    if value.is_empty() {
        return Err(ValidationError::new(field, "empty"));
    }
    if value.len() > 128 {
        return Err(ValidationError::new(field, "too long"));
    }
    let ok = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !ok {
        return Err(ValidationError::new(field, "invalid character"));
    }
    Ok(())
}

/// Join values into a JSON string array.
pub fn json_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|v| quote(v))
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parse_rejects_bad_input() {
        assert_eq!(
            Version::parse("0.11.0")
                .unwrap_or_else(|_| Version::new(0, 0, 0))
                .to_string(),
            "0.11.0"
        );
        assert!(Version::parse("0.11").is_err());
        assert!(Version::parse("0.11.x").is_err());
    }

    #[test]
    fn report_fails_closed_for_blocked() {
        let mut report = Report::new("readiness");
        report.push(Finding::new(
            "tenant.broad",
            Severity::Blocked,
            "broad permission denied",
        ));
        assert!(report.fails_closed());
        assert!(report.to_json().contains("tenant.broad"));
    }

    #[test]
    fn quote_escapes_control_characters() {
        assert_eq!(quote("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }
}
