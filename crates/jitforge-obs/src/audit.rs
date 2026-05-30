//! Append-only audit log with deterministic hash chaining.

fn digest(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xfe;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("audit:{hash:016x}")
}

/// One audit event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditEvent {
    pub sequence: u64,
    pub actor: String,
    pub action: String,
    pub tenant: String,
    pub resource: String,
    pub decision: String,
    pub previous_hash: String,
    pub event_hash: String,
}

impl AuditEvent {
    fn new(
        sequence: u64,
        actor: &str,
        action: &str,
        tenant: &str,
        resource: &str,
        decision: &str,
        previous_hash: &str,
    ) -> Self {
        let sequence_text = sequence.to_string();
        let event_hash = digest(&[
            sequence_text.as_str(),
            actor,
            action,
            tenant,
            resource,
            decision,
            previous_hash,
        ]);
        Self {
            sequence,
            actor: actor.to_owned(),
            action: action.to_owned(),
            tenant: tenant.to_owned(),
            resource: resource.to_owned(),
            decision: decision.to_owned(),
            previous_hash: previous_hash.to_owned(),
            event_hash,
        }
    }
}

/// Append-only audit log.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuditLog {
    events: Vec<AuditEvent>,
}

impl AuditLog {
    /// Create an empty audit log.
    pub const fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Append one event and return it.
    pub fn append(
        &mut self,
        actor: &str,
        action: &str,
        tenant: &str,
        resource: &str,
        decision: &str,
    ) -> AuditEvent {
        let sequence = self.events.len() as u64 + 1;
        let previous_hash = self
            .events
            .last()
            .map(|event| event.event_hash.as_str())
            .unwrap_or("genesis");
        let event = AuditEvent::new(
            sequence,
            actor,
            action,
            tenant,
            resource,
            decision,
            previous_hash,
        );
        self.events.push(event.clone());
        event
    }

    /// Events in append order.
    pub fn events(&self) -> &[AuditEvent] {
        &self.events
    }

    /// Verify hash chain integrity.
    pub fn verify(&self) -> bool {
        let mut previous = String::from("genesis");
        for (idx, event) in self.events.iter().enumerate() {
            if event.sequence != idx as u64 + 1 {
                return false;
            }
            if event.previous_hash != previous {
                return false;
            }
            let expected = AuditEvent::new(
                event.sequence,
                &event.actor,
                &event.action,
                &event.tenant,
                &event.resource,
                &event.decision,
                &event.previous_hash,
            );
            if expected.event_hash != event.event_hash {
                return false;
            }
            previous = event.event_hash.clone();
        }
        true
    }
}
