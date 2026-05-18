// Brought in from upstream/main during the v3.1.0 merge of unrelated
// histories. Upstream uses this lock from cargo_cache_tests.rs, git/system.rs,
// and remote_ops.rs to serialize tests that mutate process-wide env vars.
// Local equivalents of those tests use file-scoped Mutexes; keep the lock
// available so future ports of upstream tests work without rewiring.
#![allow(dead_code)]

use std::sync::{LazyLock, Mutex};

pub(crate) static PATH_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
