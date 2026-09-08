#![doc = "Host-ALLOWLIST forward egress proxy for jeryu cells."]
#![doc = ""]
#![doc = "Today the host-level forward proxy (`actions-runners/egress-proxy.py`) is"]
#![doc = "ALLOW-ALL: any cell that can reach the proxy can egress anywhere on the"]
#![doc = "internet. This crate replaces that with a reviewable, CI-testable Rust"]
#![doc = "artifact that forwards traffic ONLY to an explicit host allowlist, and that"]
#![doc = "can be flipped to deny-all the instant a cell blows its token budget."]
#![doc = ""]
#![doc = "The crate is split deliberately:"]
#![doc = "* [`egress_decision`] is a PURE, side-effect-free function — it is the single"]
#![doc = "  source of truth for the allow/deny verdict and is exhaustively unit tested"]
#![doc = "  with no network at all."]
#![doc = "* [`proxy`] is the async (tokio) plumbing that consults that verdict before"]
#![doc = "  ever opening an upstream socket."]

pub mod allowlist;
pub mod proxy;

pub use allowlist::Allowlist;
pub use proxy::{Proxy, ProxyConfig};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The verdict for a single egress request.
///
/// Only [`Decision::Allow`] permits the proxy to open an upstream connection.
/// Both deny variants are distinct so that logs / receipts can tell a policy
/// violation (`DenyNotAllowlisted`) apart from a budget kill (`DenyBudget`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Host is on the allowlist (or an allowed suffix) and the budget is intact.
    Allow,
    /// Host is not on the allowlist; the proxy must respond 403 and NOT connect.
    DenyNotAllowlisted,
    /// The budget kill switch is set; ALL egress is denied regardless of host.
    DenyBudget,
}

impl Decision {
    /// Whether this verdict permits opening an upstream connection.
    #[must_use]
    pub fn is_allowed(self) -> bool {
        matches!(self, Decision::Allow)
    }

    /// A short, stable reason string for logging / 403 bodies.
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::DenyNotAllowlisted => "not-allowlisted",
            Decision::DenyBudget => "budget-exceeded",
        }
    }
}

/// The PURE allowlist decision function — the single source of truth.
///
/// The budget kill switch is checked FIRST and is fail-closed: when
/// `budget_exceeded` is `true` EVERY host is denied with [`Decision::DenyBudget`],
/// even hosts that would otherwise be allowed. Only if the budget is intact do we
/// consult the [`Allowlist`].
///
/// This function never touches the network, DNS, the clock, or any global state,
/// which is exactly why it is unit-testable in isolation.
#[must_use]
pub fn egress_decision(host: &str, allow: &Allowlist, budget_exceeded: bool) -> Decision {
    if budget_exceeded {
        return Decision::DenyBudget;
    }
    if allow.permits(host) {
        Decision::Allow
    } else {
        Decision::DenyNotAllowlisted
    }
}

/// A cheaply-clonable, shareable budget kill switch.
///
/// A single [`AtomicBool`] behind an [`Arc`]: clone it into every connection task
/// and into whatever monitors token spend. Flipping it to `true` (via
/// [`Budget::trip`]) instantly causes [`egress_decision`] to deny ALL subsequent
/// requests, so a cell that overruns its token budget loses egress mid-flight.
#[derive(Debug, Clone, Default)]
pub struct Budget {
    exceeded: Arc<AtomicBool>,
}

impl Budget {
    /// Create a budget that has NOT been exceeded.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the budget has been tripped (egress should be denied).
    #[must_use]
    pub fn is_exceeded(&self) -> bool {
        self.exceeded.load(Ordering::SeqCst)
    }

    /// Trip the kill switch: all subsequent egress is denied.
    pub fn trip(&self) {
        self.exceeded.store(true, Ordering::SeqCst);
    }

    /// Clear the kill switch and re-allow egress (test / operator use).
    pub fn reset(&self) {
        self.exceeded.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_allow() -> Allowlist {
        Allowlist::default()
    }

    #[test]
    fn allowed_exact_host_is_allow() {
        let allow = default_allow();
        assert_eq!(
            egress_decision("api.anthropic.com", &allow, false),
            Decision::Allow
        );
        assert_eq!(
            egress_decision("github.com", &allow, false),
            Decision::Allow
        );
    }

    #[test]
    fn allowed_suffix_host_is_allow() {
        let allow = default_allow();
        // `.crates.io` is an allowed suffix, so a subdomain must be allowed.
        assert_eq!(
            egress_decision("foo.crates.io", &allow, false),
            Decision::Allow
        );
        assert_eq!(
            egress_decision("static.crates.io", &allow, false),
            Decision::Allow
        );
    }

    #[test]
    fn non_allowlisted_host_is_denied() {
        let allow = default_allow();
        assert_eq!(
            egress_decision("evil.example.com", &allow, false),
            Decision::DenyNotAllowlisted
        );
    }

    #[test]
    fn budget_exceeded_denies_even_allowlisted_host() {
        let allow = default_allow();
        // A host that WOULD be allowed must still be denied once the budget trips.
        assert_eq!(
            egress_decision("api.anthropic.com", &allow, true),
            Decision::DenyBudget
        );
        assert_eq!(
            egress_decision("foo.crates.io", &allow, true),
            Decision::DenyBudget
        );
    }

    #[test]
    fn budget_takes_precedence_over_not_allowlisted() {
        let allow = default_allow();
        // Even a non-allowlisted host reports the budget reason when tripped,
        // because the kill switch is checked first and is fail-closed.
        assert_eq!(
            egress_decision("evil.example.com", &allow, true),
            Decision::DenyBudget
        );
    }

    #[test]
    fn host_match_is_case_insensitive() {
        let allow = default_allow();
        assert_eq!(
            egress_decision("API.Anthropic.COM", &allow, false),
            Decision::Allow
        );
        assert_eq!(
            egress_decision("Foo.Crates.IO", &allow, false),
            Decision::Allow
        );
    }

    #[test]
    fn substring_attack_host_is_denied() {
        let allow = default_allow();
        // `crates.io.attacker.com` CONTAINS "crates.io" but is NOT crates.io nor a
        // subdomain of it. A naive contains() check would allow it; proper host /
        // suffix matching must deny it.
        assert_eq!(
            egress_decision("crates.io.attacker.com", &allow, false),
            Decision::DenyNotAllowlisted
        );
        // The dangerous prefix-without-dot case too.
        assert_eq!(
            egress_decision("notgithub.com", &allow, false),
            Decision::DenyNotAllowlisted
        );
        // And a host that merely ends with the allowed name without a dot
        // boundary ("evilcrates.io") must be denied.
        assert_eq!(
            egress_decision("evilcrates.io", &allow, false),
            Decision::DenyNotAllowlisted
        );
    }

    #[test]
    fn budget_kill_switch_round_trips() {
        let budget = Budget::new();
        assert!(!budget.is_exceeded());
        budget.trip();
        assert!(budget.is_exceeded());
        // A clone shares the same underlying flag.
        let clone = budget.clone();
        assert!(clone.is_exceeded());
        budget.reset();
        assert!(!clone.is_exceeded());
    }
}
