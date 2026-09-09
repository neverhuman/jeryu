//! Memory cgroup OOM-kill: a child that balloons past its cgroup `memory.max`
//! must be OOM-killed by the kernel, not allowed to exhaust host memory.
//!
//! This complements the fork-bomb escape (which proves `pids.max`) by proving
//! the MEMORY controller: a runaway allocator is a real DoS vector the workcell
//! must contain. It runs for real only where a delegated cgroup-v2 subtree with
//! the memory controller is available; otherwise it honestly skips (the kernel
//! primitive is genuinely absent — never a fake pass). The asserted invariant is
//! that an over-limit allocator does NOT escape (survive) its memory cap.

use jeryu_sandbox_linux::capability::SandboxCapabilities;
use jeryu_sandbox_linux::{EscapeVerdict, run_in_forked_child};
use std::path::{Path, PathBuf};

/// Create a child cgroup under `parent` with a small `memory.max` and swap
/// disabled (so exceeding the cap triggers an OOM-kill rather than swapping).
/// Returns the cgroup dir, or `None` when the controller can't be delegated/set
/// here (the caller then honestly skips). Mirrors `make_limited_cgroup` in
/// `escape_suite.rs`, but constrains memory instead of pids.
fn make_limited_memory_cgroup(parent: &Path, max_bytes: u64) -> Option<PathBuf> {
    let _ = std::fs::write(parent.join("cgroup.subtree_control"), b"+memory");
    let dir = parent.join(format!("jeryu-escape-oom-{}.scope", std::process::id()));
    std::fs::create_dir(&dir).ok()?;
    // Swap MUST be 0 or the child swaps instead of OOM-ing (best-effort: some
    // hosts lack the swap controller, in which case there is nothing to disable).
    let _ = std::fs::write(dir.join("memory.swap.max"), b"0");
    if std::fs::write(dir.join("memory.max"), max_bytes.to_string()).is_err() {
        let _ = std::fs::remove_dir(&dir);
        return None;
    }
    Some(dir)
}

#[test]
fn a_runaway_allocator_is_oom_killed_by_its_cgroup_memory_max() {
    let caps = SandboxCapabilities::probe();
    eprintln!("MEMORY-OOM caps: {}", caps.summary());

    let verdict = match &caps.cgroup_v2_subtree {
        None => {
            eprintln!("SKIP: no delegated cgroup-v2 subtree; cannot enforce memory.max");
            EscapeVerdict::Skipped
        }
        Some(parent) => {
            const MAX_BYTES: u64 = 16 * 1024 * 1024; // 16 MiB cap
            match make_limited_memory_cgroup(parent, MAX_BYTES) {
                None => {
                    eprintln!("SKIP: memory controller not delegatable here");
                    EscapeVerdict::Skipped
                }
                Some(cg) => {
                    let procs = cg.join("cgroup.procs");
                    let cg_clone = cg.clone();
                    let v = run_in_forked_child(move || {
                        // Join the memory-constrained cgroup.
                        if std::fs::write(&procs, std::process::id().to_string()).is_err() {
                            return 2; // could not join -> honest skip
                        }
                        // Balloon well past the 16 MiB cap, TOUCHING every page so
                        // it is charged. With swap disabled the kernel must OOM-kill
                        // us (SIGKILL) long before reaching 128 MiB. Surviving the
                        // whole allocation means the cap did NOT contain us.
                        let chunk = 4 * 1024 * 1024usize; // 4 MiB
                        let mut held: Vec<Vec<u8>> = Vec::new();
                        for _ in 0..32 {
                            // vec![1u8; chunk] memsets every byte, faulting in pages.
                            let mut block = vec![1u8; chunk];
                            // Re-touch a byte per page to defeat any lazy/CoW tricks.
                            let mut i = 0;
                            while i < block.len() {
                                block[i] = block[i].wrapping_add(1);
                                i += 4096;
                            }
                            held.push(block);
                        }
                        std::hint::black_box(&held);
                        1 // survived 128 MiB under a 16 MiB cap == breach
                    });
                    let _ = std::fs::remove_dir(&cg_clone);
                    v
                }
            }
        }
    };

    eprintln!("MEMORY-OOM verdict: {}", verdict.as_str());
    assert_ne!(
        verdict,
        EscapeVerdict::Escaped,
        "a runaway allocator escaped its cgroup memory.max (the cap did not contain it)"
    );
}
