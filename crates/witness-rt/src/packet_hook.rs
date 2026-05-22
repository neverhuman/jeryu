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
    let packet = crate::packet::RepairPacket::for_assert(
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
