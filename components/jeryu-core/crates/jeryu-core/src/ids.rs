//! Strong-ish string id wrappers used across Phase 7.

use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

macro_rules! id_type {
    ($name:ident, $prefix:literal) => {
        #[doc = concat!("Identifier for ", stringify!($name), ".")]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// Creates an id from a trusted existing string.
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// Creates a deterministic process-local id suitable for tests and
            /// in-memory receipts.
            pub fn fresh() -> Self {
                let n = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                Self(format!("{}_{n:016x}", $prefix))
            }

            /// Returns the borrowed id string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::new(value)
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id_type!(RepoId, "repo");
id_type!(PullRequestId, "pr");
id_type!(QueueEntryId, "queue");
id_type!(ReceiptId, "receipt");
id_type!(AgentId, "agent");
