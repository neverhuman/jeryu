use std::collections::BTreeSet;
use std::process::Command;
use std::sync::{Arc, Barrier};
use std::thread;

use jeryu_agentbridge::AgentDriver;
use jeryu_runner_core::{NetworkPolicy, SecretPolicy};
use tempfile::tempdir;

#[test]
fn deterministic_editbot_profile_is_network_denied() {
    let scratch = tempdir().expect("scratch tempdir");
    let driver = AgentDriver::deterministic_editbot(scratch.path());

    assert_eq!(driver.network_policy(), NetworkPolicy::Deny);
    assert_eq!(driver.secret_policy(), SecretPolicy::None);
}

#[test]
fn parallel_editbot_staging_uses_unique_ready_dirs() {
    const WORKERS: usize = 80;

    let scratch = tempdir().expect("scratch tempdir");
    let barrier = Arc::new(Barrier::new(WORKERS));
    let mut handles = Vec::new();

    for idx in 0..WORKERS {
        let scratch_root = scratch.path().to_path_buf();
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            let driver = AgentDriver::deterministic_editbot(scratch_root);
            let script = format!("#!/usr/bin/env sh\nprintf '%s\\n' editbot-{idx}\n");

            barrier.wait();
            let staged = driver
                .stage_edit_bot(script.as_bytes())
                .expect("stage edit-bot");
            let root = staged.root().to_path_buf();

            assert!(staged.ready_dir().is_dir());
            assert!(staged.executable().is_file());
            assert!(
                !root.join("pending").exists(),
                "pending directory must not remain executable"
            );

            let output = Command::new(staged.executable())
                .output()
                .expect("run staged edit-bot");
            assert!(output.status.success());
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                format!("editbot-{idx}\n")
            );

            root
        }));
    }

    let mut roots = BTreeSet::new();
    for handle in handles {
        roots.insert(handle.join().expect("worker thread"));
    }

    assert_eq!(
        roots.len(),
        WORKERS,
        "each parallel stage must receive a unique tempdir"
    );
}
