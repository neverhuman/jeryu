use super::*;
use crate::launch::terminate_prepared_child;
use std::ffi::CString;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt;
use std::time::{Duration, Instant};

fn payload(descriptor: OwnedFd) -> SandboxPayload {
    SandboxPayload {
        cgroup_procs: None,
        apply_user_ns: false,
        apply_mount_ns: false,
        apply_pid_ns: false,
        landlock: Some(descriptor),
        seccomp_bpf: None,
        pty_slave_fd: None,
        rlimits: RlimitFallback {
            memory_max_bytes: 0,
        },
    }
}

fn path_cstring(path: &Path) -> CString {
    CString::new(path.as_os_str().as_bytes()).unwrap()
}

// The caller prepares every value before fork. Child closures below use only
// raw syscalls and the production child setup; they never allocate or unwind.
fn check_child(check: impl FnOnce() -> i32) {
    // SAFETY: the child runs the syscall-only check then _exit, without Drop.
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork: {}", IoError::last_os_error());
    if pid == 0 {
        terminate_prepared_child(check());
    }
    check_reaped_child(pid);
}

fn wait_until(pid: libc::pid_t, deadline: Instant) -> Option<i32> {
    let mut status = 0;
    loop {
        // SAFETY: pid is our unreaped child; status is a live writable integer.
        let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if result == pid {
            return Some(status);
        }
        if result == -1 && IoError::last_os_error().kind() != ErrorKind::Interrupted {
            panic!(
                "waitpid lost child custody: pid={pid}, error={}; no further signal attempted",
                IoError::last_os_error()
            );
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn check_reaped_child(pid: libc::pid_t) {
    let Some(status) = wait_until(pid, Instant::now() + Duration::from_secs(5)) else {
        // SAFETY: this exact unreaped child still owns pid; do not kill a group.
        let killed = unsafe { libc::kill(pid, libc::SIGKILL) };
        let kill_error = (killed == -1).then(IoError::last_os_error);
        let reaped = wait_until(pid, Instant::now() + Duration::from_secs(2));
        assert!(
            reaped.is_some(),
            "child cleanup unresolved: pid={pid}, kill_error={kill_error:?}"
        );
        assert!(
            killed == 0
                || kill_error
                    .as_ref()
                    .is_some_and(|e| e.raw_os_error() == Some(libc::ESRCH)),
            "child termination failed: pid={pid}, kill_error={kill_error:?}"
        );
        panic!("Landlock child exceeded five seconds; exact child reaped");
    };
    assert!(libc::WIFEXITED(status), "child status: {status}");
    assert_eq!(libc::WEXITSTATUS(status), 0, "child check failed");
}

fn can_open(path: &CString, flags: libc::c_int) -> bool {
    // SAFETY: path is NUL-terminated and no creation flag requires a mode.
    let descriptor = unsafe { libc::open(path.as_ptr(), flags | libc::O_CLOEXEC) };
    if descriptor < 0 {
        return false;
    }
    // SAFETY: this call just created and owns descriptor.
    unsafe { libc::close(descriptor) };
    true
}

#[test]
fn prepared_landlock_enforces_child_access_without_confining_parent() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let inside = workspace.join("input");
    let outside = root.path().join("outside");
    std::fs::write(&inside, b"input").unwrap();
    std::fs::write(&outside, b"sentinel").unwrap();
    let rules = [LandlockRule {
        path: workspace,
        read: true,
        write: false,
        execute: false,
    }];
    let descriptor =
        prepare_landlock(&rules, 1, root.path()).expect("actual kernel Landlock ruleset");
    // SAFETY: descriptor is owned and live, F_GETFD only inspects it.
    let flags = unsafe { libc::fcntl(descriptor.as_raw_fd(), libc::F_GETFD) };
    assert!(flags >= 0, "F_GETFD: {}", IoError::last_os_error());
    assert_ne!(flags & libc::FD_CLOEXEC, 0);
    let sandbox = payload(descriptor);
    let inside_c = path_cstring(&inside);
    let outside_c = path_cstring(&outside);
    check_child(|| {
        if apply_in_child(&sandbox).is_err() {
            return 1;
        }
        if !can_open(&inside_c, libc::O_RDONLY)
            || can_open(&inside_c, libc::O_WRONLY)
            || can_open(&outside_c, libc::O_RDONLY)
        {
            return 2;
        }
        0
    });
    std::fs::write(&inside, b"parent still writable").unwrap();
    assert_eq!(std::fs::read(&outside).unwrap(), b"sentinel");
}

#[test]
fn prepared_landlock_keeps_original_directory_when_path_is_replaced() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let retained = root.path().join("retained");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("input"), b"original").unwrap();
    let rules = [LandlockRule {
        path: workspace.clone(),
        read: true,
        write: false,
        execute: false,
    }];
    let sandbox =
        payload(prepare_landlock(&rules, 1, root.path()).expect("actual kernel Landlock ruleset"));
    std::fs::rename(&workspace, &retained).unwrap();
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("input"), b"replacement").unwrap();
    let original = path_cstring(&retained.join("input"));
    let replacement = path_cstring(&workspace.join("input"));
    check_child(|| {
        if apply_in_child(&sandbox).is_err() {
            return 1;
        }
        if !can_open(&original, libc::O_RDONLY) || can_open(&replacement, libc::O_RDONLY) {
            return 2;
        }
        0
    });
}

#[test]
fn invalid_landlock_descriptor_retains_kernel_errno_in_child() {
    let descriptor: OwnedFd = std::fs::File::open("/dev/null").unwrap().into();
    let sandbox = payload(descriptor);
    check_child(|| match apply_in_child(&sandbox) {
        Err(error) if error.raw_os_error() == Some(libc::EBADFD) => 0,
        _ => 1,
    });
}

#[test]
fn relative_landlock_rule_uses_job_workspace_instead_of_parent_cwd() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("input"), b"job input").unwrap();
    let outside = std::env::current_dir().unwrap().join("Cargo.toml");
    assert!(outside.is_file(), "owning package Cargo.toml must exist");
    let rules = [LandlockRule {
        path: ".".into(),
        read: true,
        write: false,
        execute: false,
    }];
    let sandbox = payload(prepare_landlock(&rules, 1, &workspace).unwrap());
    let inside = path_cstring(&workspace.join("input"));
    let outside = path_cstring(&outside);
    check_child(|| {
        if apply_in_child(&sandbox).is_err() {
            return 1;
        }
        if !can_open(&inside, libc::O_RDONLY) || can_open(&outside, libc::O_RDONLY) {
            return 2;
        }
        0
    });
}

#[test]
fn prepared_ruleset_avoids_closed_standard_stream_slots() {
    const CHILD: &str = "JERYU_LANDLOCK_LOW_DESCRIPTOR_TEST";
    const WORKSPACE: &str = "JERYU_LANDLOCK_LOW_DESCRIPTOR_WORKSPACE";
    const TEST: &str =
        "launch::internals::tests::prepared_ruleset_avoids_closed_standard_stream_slots";
    const PROOF: &[u8] = b"JERYU_LANDLOCK_LOW_DESCRIPTOR_PASSED\n";
    if std::env::var(CHILD).as_deref() == Ok("1")
        && std::env::args().any(|argument| argument == TEST)
    {
        let root = std::path::PathBuf::from(std::env::var_os(WORKSPACE).unwrap());
        let rules = [LandlockRule {
            path: root.clone(),
            read: true,
            write: false,
            execute: false,
        }];
        // This is a separately exec'd one-test process, never a shared test
        // parent or a post-fork allocating closure. No caller owns its stdin.
        // SAFETY: close only this isolated process's standard input descriptor.
        assert_eq!(unsafe { libc::close(libc::STDIN_FILENO) }, 0);
        let descriptor = prepare_landlock(&rules, 1, &root).unwrap();
        assert!(descriptor.as_raw_fd() >= 3);
        let sandbox = payload(descriptor);
        let path = path_cstring(&root);
        let result = (|| {
            // Simulate Command wiring standard input before child setup.
            // SAFETY: open obtains a descriptor; dup2 targets only this child's stdin.
            let input = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY) };
            if input == -1 {
                return 1;
            }
            if input != libc::STDIN_FILENO {
                let result = unsafe { libc::dup2(input, libc::STDIN_FILENO) };
                unsafe { libc::close(input) };
                if result == -1 {
                    return 2;
                }
            }
            if apply_in_child(&sandbox).is_err() || !can_open(&path, libc::O_RDONLY) {
                return 3;
            }
            0
        })();
        // Do not return to allocating test-harness code after confinement. The
        // parent owns the workspace; this process has no child to orphan and
        // no scratch directory whose Drop would be skipped by process exit.
        if result == 0 {
            // SAFETY: stdout is the parent's open transcript; PROOF is live.
            let written = unsafe { libc::write(1, PROOF.as_ptr().cast(), PROOF.len()) };
            if written != PROOF.len() as isize {
                terminate_prepared_child(4);
            }
        }
        terminate_prepared_child(result);
    }
    let workspace = tempfile::tempdir().unwrap();
    let transcript = tempfile::NamedTempFile::new().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .env_clear()
        .env(CHILD, "1")
        .env(WORKSPACE, workspace.path())
        .args(["--exact", TEST, "--test-threads=1"])
        .stdin(Stdio::null())
        .stdout(transcript.as_file().try_clone().unwrap())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap_or_else(|error| {
            panic!(
                "isolated child custody lost: pid={}, error={error}; no further signal attempted",
                child.id()
            )
        }) {
            break status;
        }
        if Instant::now() >= deadline {
            let killed = child.kill();
            let cleanup_deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if child
                    .try_wait()
                    .unwrap_or_else(|error| {
                        panic!(
                            "isolated child cleanup custody lost: pid={}, error={error}, kill_result={killed:?}; no further signal attempted",
                            child.id()
                        )
                    })
                    .is_some()
                {
                    panic!("isolated Landlock child exceeded five seconds; reaped: {killed:?}");
                }
                assert!(
                    Instant::now() < cleanup_deadline,
                    "isolated child cleanup unresolved: pid={}, kill_result={killed:?}",
                    child.id()
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success(), "isolated child check failed: {status}");
    let output = std::fs::read_to_string(transcript.path()).unwrap();
    assert!(
        output.contains(std::str::from_utf8(PROOF).unwrap()),
        "isolated low-descriptor case did not execute: {output}"
    );
}
