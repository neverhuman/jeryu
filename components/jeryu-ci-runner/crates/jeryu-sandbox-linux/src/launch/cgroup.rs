//! Fail-closed cgroup-v2 construction for sandboxed children.

use super::{SandboxError, SandboxResult};
use jeryu_runner_core::sandbox::CgroupLimits;
use std::path::{Path, PathBuf};

/// Create a fresh child cgroup under the delegated `parent`, enable controllers,
/// and write the limits. Returns the path to its `cgroup.procs` (where the child
/// writes its own pid in `pre_exec`).
pub(super) fn create_cgroup(
    parent: &Path,
    limits: &CgroupLimits,
    require_cgroup: bool,
) -> SandboxResult<PathBuf> {
    create_cgroup_with_writer(parent, limits, require_cgroup, |path, data| {
        std::fs::write(path, data)
    })
}

fn create_cgroup_with_writer(
    parent: &Path,
    limits: &CgroupLimits,
    require_cgroup: bool,
    write_file: impl Fn(&Path, &[u8]) -> std::io::Result<()>,
) -> SandboxResult<PathBuf> {
    // Ensure the parent delegates the load-bearing controllers we need to
    // children. Strict agent plans fail closed if memory or pids delegation
    // cannot be enabled; ordinary CI jobs keep the older best-effort posture.
    enable_cgroup_controller(
        parent,
        "memory",
        require_cgroup,
        "cgroup_memory_controller_enable_failed",
        &write_file,
    )?;
    enable_cgroup_controller(
        parent,
        "pids",
        require_cgroup,
        "cgroup_pids_controller_enable_failed",
        &write_file,
    )?;
    // CPU weight remains a tuning hint; do not refuse a strict memory/pids jail
    // merely because CPU delegation is absent.
    let _ = write_file(&parent.join("cgroup.subtree_control"), b"+cpu");

    let name = format!("jeryu-job-{}.scope", jeryu_runner_core::receipt::now_ms());
    let dir = parent.join(name);
    std::fs::create_dir(&dir)
        .map_err(|err| SandboxError::new("cgroup_create_failed", err.to_string()))?;

    // Best-effort for ordinary CI; mandatory for strict agent workcells.
    if let Err(err) = write_cgroup_limit(
        &dir,
        "memory.max",
        limits.memory_max_bytes.to_string().as_bytes(),
        require_cgroup,
        "cgroup_memory_max_write_failed",
        &write_file,
    ) {
        let _ = std::fs::remove_dir(&dir);
        return Err(err);
    }
    if let Err(err) = write_cgroup_limit(
        &dir,
        "pids.max",
        limits.pids_max.to_string().as_bytes(),
        require_cgroup,
        "cgroup_pids_max_write_failed",
        &write_file,
    ) {
        let _ = std::fs::remove_dir(&dir);
        return Err(err);
    }
    // cpu.weight in cgroup-v2 is 1..=10000; the plan uses the same scale band.
    let _ = write_file(
        &dir.join("cpu.weight"),
        limits.cpu_weight.clamp(1, 10_000).to_string().as_bytes(),
    );

    Ok(dir.join("cgroup.procs"))
}

fn enable_cgroup_controller(
    parent: &Path,
    controller: &'static str,
    require_cgroup: bool,
    strict_error_code: &'static str,
    write_file: &impl Fn(&Path, &[u8]) -> std::io::Result<()>,
) -> SandboxResult<()> {
    let path = parent.join("cgroup.subtree_control");
    let token = format!("+{controller}");
    match write_file(&path, token.as_bytes()) {
        Ok(()) => Ok(()),
        Err(err) if require_cgroup => Err(SandboxError::new(
            strict_error_code,
            format!(
                "failed to enable cgroup controller {controller} at {}: {err}",
                path.display()
            ),
        )),
        Err(_) => Ok(()),
    }
}

fn write_cgroup_limit(
    dir: &Path,
    filename: &'static str,
    data: &[u8],
    require_cgroup: bool,
    strict_error_code: &'static str,
    write_file: &impl Fn(&Path, &[u8]) -> std::io::Result<()>,
) -> SandboxResult<()> {
    let path = dir.join(filename);
    match write_file(&path, data) {
        Ok(()) => Ok(()),
        Err(err) if require_cgroup => Err(SandboxError::new(
            strict_error_code,
            format!(
                "failed to write cgroup limit {filename} at {}: {err}",
                path.display()
            ),
        )),
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_limits() -> CgroupLimits {
        CgroupLimits {
            memory_max_bytes: 64 * 1024 * 1024,
            cpu_weight: 100,
            pids_max: 32,
            io_weight: 100,
        }
    }

    fn forced_write_failure() -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "forced cgroup write failure",
        )
    }

    #[test]
    fn strict_cgroup_requires_memory_controller_enable() {
        let parent = tempfile::tempdir().expect("temp cgroup parent");
        let err = create_cgroup_with_writer(parent.path(), &test_limits(), true, |path, data| {
            if path.file_name().and_then(|name| name.to_str()) == Some("cgroup.subtree_control")
                && data == b"+memory"
            {
                Err(forced_write_failure())
            } else {
                Ok(())
            }
        })
        .expect_err("strict cgroup must fail closed when memory controller enable fails");

        assert_eq!(err.code(), "cgroup_memory_controller_enable_failed");
        assert!(err.message().contains("memory"));
    }

    #[test]
    fn strict_cgroup_requires_pids_controller_enable() {
        let parent = tempfile::tempdir().expect("temp cgroup parent");
        let err = create_cgroup_with_writer(parent.path(), &test_limits(), true, |path, data| {
            if path.file_name().and_then(|name| name.to_str()) == Some("cgroup.subtree_control")
                && data == b"+pids"
            {
                Err(forced_write_failure())
            } else {
                Ok(())
            }
        })
        .expect_err("strict cgroup must fail closed when pids controller enable fails");

        assert_eq!(err.code(), "cgroup_pids_controller_enable_failed");
        assert!(err.message().contains("pids"));
    }

    #[test]
    fn strict_cgroup_requires_memory_and_pids_limit_writes() {
        for (filename, expected_code) in [
            ("memory.max", "cgroup_memory_max_write_failed"),
            ("pids.max", "cgroup_pids_max_write_failed"),
        ] {
            let parent = tempfile::tempdir().expect("temp cgroup parent");
            let err =
                create_cgroup_with_writer(parent.path(), &test_limits(), true, |path, _data| {
                    if path.file_name().and_then(|name| name.to_str()) == Some(filename) {
                        Err(forced_write_failure())
                    } else {
                        Ok(())
                    }
                })
                .expect_err("strict cgroup must fail closed when load-bearing limit writes fail");

            assert_eq!(err.code(), expected_code);
            assert!(err.message().contains(filename));
            assert!(
                std::fs::read_dir(parent.path())
                    .expect("read temp parent")
                    .next()
                    .is_none(),
                "failed strict cgroup setup should clean up its empty child directory"
            );
        }
    }

    #[test]
    fn non_strict_cgroup_limit_writes_remain_best_effort() {
        let parent = tempfile::tempdir().expect("temp cgroup parent");
        let procs =
            create_cgroup_with_writer(parent.path(), &test_limits(), false, |_path, _data| {
                Err(forced_write_failure())
            })
            .expect("non-strict cgroup setup keeps best-effort write behavior");

        assert_eq!(
            procs.file_name().and_then(|name| name.to_str()),
            Some("cgroup.procs")
        );
    }
}
