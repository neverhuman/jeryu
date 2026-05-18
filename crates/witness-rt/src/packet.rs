use serde::{Deserialize, Serialize};

/// A structured repair packet emitted on panic or assertion failure.
///
/// This is the universal agent-facing failure envelope. Every runtime failure
/// that passes through `witness-rt` produces one of these, either via the panic
/// hook or via the `agent_ensure!` / `agent_bail!` / `agent_expect!` macros.
///
/// # Example
///
/// ```
/// use witness_rt::RepairPacket;
///
/// let packet = RepairPacket {
///     code: "PRICE-NEGATIVE".into(),
///     message: "subtotal must be non-negative".into(),
///     file: "crates/pricing/src/lib.rs".into(),
///     line: 42,
///     column: 5,
///     cell: Some("pricing".into()),
///     cell_purpose: Some("Compute tax-aware quote totals".into()),
///     match_provenance: Some("longest-owned-path-prefix".into()),
///     matched_owned_path: Some("crates/pricing/src/".into()),
///     invariants: vec!["subtotal_cents is non-negative".into()],
///     likely_causes: vec!["discount exceeded subtotal".into()],
///     hints: vec!["check coupon validation before applying discount".into()],
///     local_commands: vec!["cargo test -p pricing".into()],
///     escalate_commands: vec![],
///     timestamp: "2026-03-31T10:00:00Z".into(),
/// };
/// assert_eq!(packet.code, "PRICE-NEGATIVE");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairPacket {
    /// Stable error code (e.g., `"PRICE-NEGATIVE"`, `"CFG-MISSING"`).
    pub code: String,

    /// Human-readable failure message.
    pub message: String,

    /// Source file where the failure occurred.
    pub file: String,

    /// Line number in the source file.
    pub line: u32,

    /// Column number in the source file.
    pub column: u32,

    /// Best-matching cell name, if a cell was registered for this path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<String>,

    /// Purpose of the owning cell.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_purpose: Option<String>,

    /// How the runtime matched this failure to a cell.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_provenance: Option<String>,

    /// The owned path prefix that matched the failing file, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_owned_path: Option<String>,

    /// Invariants that may have been violated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<String>,

    /// Likely causes of this failure.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub likely_causes: Vec<String>,

    /// Specific repair hints for the agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,

    /// Commands to run for local validation after a fix.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_commands: Vec<String>,

    /// Commands to run if boundary escalation is needed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub escalate_commands: Vec<String>,

    /// ISO-8601 timestamp of when the failure occurred.
    pub timestamp: String,
}

/// Cell registration metadata.
///
/// Register cells at startup so the panic hook can map panic locations
/// to their owning cell and enrich repair packets with context.
///
/// # Example
///
/// ```
/// use witness_rt::CellRegistration;
///
/// let cell = CellRegistration {
///     id: "pricing.quote".into(),
///     purpose: "Compute tax-aware quote totals".into(),
///     owned_paths: vec!["crates/pricing/src/".into()],
///     invariants: vec!["subtotal_cents is non-negative".into()],
///     local_commands: vec!["cargo test -p pricing".into()],
///     escalate_commands: vec![],
///     hints: vec!["keep pricing pure; no I/O".into()],
/// };
/// assert_eq!(cell.id, "pricing.quote");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellRegistration {
    /// Unique cell identifier (e.g., `"pricing.quote"`).
    pub id: String,

    /// Human-readable purpose of this cell.
    pub purpose: String,

    /// Path prefixes that this cell owns. Used for panic-location matching.
    pub owned_paths: Vec<String>,

    /// Invariants that this cell enforces.
    #[serde(default)]
    pub invariants: Vec<String>,

    /// Commands to run for local validation.
    #[serde(default)]
    pub local_commands: Vec<String>,

    /// Commands to run when boundary escalation is needed.
    #[serde(default)]
    pub escalate_commands: Vec<String>,

    /// Repair hints specific to this cell.
    #[serde(default)]
    pub hints: Vec<String>,
}

/// Configuration for the panic hook.
///
/// # Example
///
/// ```
/// use witness_rt::HookConfig;
///
/// let config = HookConfig::new("/path/to/workspace");
/// assert!(config.output_path.ends_with("last-failure.json"));
/// ```
#[derive(Debug, Clone)]
pub struct HookConfig {
    /// Path where repair packets are written on panic.
    pub output_path: String,

    /// Application name for context in repair packets.
    pub application: Option<String>,
}

impl RepairPacket {
    /// Build a [`RepairPacket`] for an assertion-style failure.
    ///
    /// Used by the `agent_ensure!`, `agent_bail!`, `agent_expect!`, and
    /// `agent_ok!` macros to centralize packet construction. Caller must
    /// pass `file` / `line` / `column` captured from
    /// `std::panic::Location::caller()` *at the macro call site*, not from
    /// inside this helper, so the location reflects user code rather than
    /// this function.
    ///
    /// All optional cell-context and escalation fields default to empty / `None`;
    /// the panic hook fills those in later when it processes the emitted
    /// packet against the cell registry.
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)] // assert packet is a flat fixed schema
    pub fn for_assert(
        code: String,
        message: String,
        file: String,
        line: u32,
        column: u32,
        hint: String,
        local_commands: Vec<String>,
        timestamp: String,
    ) -> Self {
        Self {
            code,
            message,
            file,
            line,
            column,
            cell: None,
            cell_purpose: None,
            match_provenance: None,
            matched_owned_path: None,
            invariants: vec![],
            likely_causes: vec![],
            hints: vec![hint],
            local_commands,
            escalate_commands: vec![],
            timestamp,
        }
    }
}

/// Build a [`RepairPacket`] from assertion-style inputs, emit it, and panic.
///
/// Centralizes the "capture caller, build packet, emit, panic" sequence used
/// by the `agent_ensure!`, `agent_bail!`, `agent_expect!`, and `agent_ok!`
/// macros. Marked `#[track_caller]` so `Location::caller()` resolves to the
/// macro call site (user code), preserving accurate panic attribution.
#[doc(hidden)]
#[track_caller]
pub fn emit_and_panic(code: &str, message: String, hint: &str, local_commands: Vec<String>) -> ! {
    let caller = ::std::panic::Location::caller();
    let packet = RepairPacket::for_assert(
        code.to_string(),
        message.clone(),
        caller.file().to_string(),
        caller.line(),
        caller.column(),
        hint.to_string(),
        local_commands,
        crate::current_timestamp(),
    );
    crate::emit_repair_packet_direct(&packet);
    panic!("[{}] {}", code, message);
}

impl HookConfig {
    /// Create a default hook config rooted at `workspace_root`.
    ///
    /// Output defaults to `<workspace_root>/target/agent/last-failure.json`.
    pub fn new(workspace_root: &str) -> Self {
        Self {
            output_path: format!("{workspace_root}/target/agent/last-failure.json"),
            application: None,
        }
    }

    /// Set the application name for richer repair packets.
    pub fn with_application(mut self, name: &str) -> Self {
        self.application = Some(name.to_string());
        self
    }
}
