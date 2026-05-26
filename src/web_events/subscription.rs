//! Per-WebSocket-connection subscription registry.
//!
//! Each live WS connection (W-B-04) owns one `SubscriptionRegistry`. The set
//! of scopes is mutated only on the connection's task (no internal locking
//! needed). The reserved scope `"global"` matches every other scope, per
//! WEB_WORK_CLAUDE.md §35.1.15.

use std::collections::HashSet;
use uuid::Uuid;

/// Opaque identifier for a live WebSocket connection. The handler in
/// `crate::web::ws` (W-B-04) generates one of these per `accept`.
pub type ConnectionId = Uuid;

/// Scope subscription set for a single WS connection.
///
/// Operations are deliberately cheap and infallible (`HashSet` on `String`).
/// Per `WEB_WORK_CLAUDE.md` §35.1.6 the BFF re-checks viewer perms against
/// every scope on every `Subscribe` frame; this registry is purely state.
#[derive(Debug, Default, Clone)]
pub struct SubscriptionRegistry {
    pub scopes: HashSet<String>,
}

impl SubscriptionRegistry {
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self {
            scopes: HashSet::new(),
        }
    }

    /// Add a scope. Returns `true` if it was newly added.
    pub fn subscribe(&mut self, scope: impl Into<String>) -> bool {
        self.scopes.insert(scope.into())
    }

    /// Remove a scope. Returns `true` if it was present.
    pub fn unsubscribe(&mut self, scope: &str) -> bool {
        self.scopes.remove(scope)
    }

    /// Test whether this registry should deliver an event with the given
    /// scope. The reserved scope `"global"` always matches.
    pub fn is_subscribed(&self, scope: &str) -> bool {
        self.scopes.contains("global") || self.scopes.contains(scope)
    }

    /// Number of explicit scope subscriptions (including `"global"` if set).
    pub fn len(&self) -> usize {
        self.scopes.len()
    }

    /// Whether no scopes are subscribed.
    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_subscription_matches_any_scope() {
        let mut r = SubscriptionRegistry::new();
        r.subscribe("global");
        assert!(r.is_subscribed("repo.foo"));
        assert!(r.is_subscribed("mr.42"));
    }

    #[test]
    fn specific_subscription_matches_only_that_scope() {
        let mut r = SubscriptionRegistry::new();
        r.subscribe("repo.foo");
        assert!(r.is_subscribed("repo.foo"));
        assert!(!r.is_subscribed("repo.bar"));
    }

    #[test]
    fn unsubscribe_removes_scope() {
        let mut r = SubscriptionRegistry::new();
        r.subscribe("repo.foo");
        assert!(r.unsubscribe("repo.foo"));
        assert!(!r.is_subscribed("repo.foo"));
    }

    #[test]
    fn len_and_is_empty_track_membership() {
        let mut r = SubscriptionRegistry::new();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
        r.subscribe("repo.foo");
        r.subscribe("mr.1");
        assert_eq!(r.len(), 2);
        assert!(!r.is_empty());
        // Re-subscribing the same scope is a no-op.
        assert!(!r.subscribe("repo.foo"));
        assert_eq!(r.len(), 2);
    }
}
