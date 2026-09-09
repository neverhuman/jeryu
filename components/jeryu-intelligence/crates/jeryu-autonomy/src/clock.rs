//! Clock implementations for the [`crate::seam::Clock`] seam.

use crate::seam::Clock;
use chrono::{DateTime, Utc};
use std::sync::Mutex;

/// Wall-clock time. Production default.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A clock pinned to a fixed instant, advanceable in tests. Lets the KillBell
/// TTL auto-arm, verdict expiry, and freeze-window edges be exercised
/// deterministically (R-5).
#[derive(Debug)]
pub struct FixedClock {
    at: Mutex<DateTime<Utc>>,
}

impl FixedClock {
    pub fn new(at: DateTime<Utc>) -> Self {
        Self { at: Mutex::new(at) }
    }

    /// Advance the clock by `delta` and return the new instant.
    pub fn advance(&self, delta: chrono::Duration) -> DateTime<Utc> {
        let mut g = self.at.lock().unwrap();
        *g += delta;
        *g
    }

    /// Set the clock to an absolute instant.
    pub fn set(&self, at: DateTime<Utc>) {
        *self.at.lock().unwrap() = at;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.at.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn fixed_clock_holds_and_advances() {
        let t0 = Utc::now();
        let c = FixedClock::new(t0);
        assert_eq!(c.now(), t0);
        let t1 = c.advance(Duration::seconds(30));
        assert_eq!(t1, t0 + Duration::seconds(30));
        assert_eq!(c.now(), t1);
    }

    #[test]
    fn system_clock_moves_forward() {
        let c = SystemClock;
        let a = c.now();
        let b = c.now();
        assert!(b >= a);
    }
}
