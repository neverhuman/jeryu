//! Owner: System Git resolution
//! Proof: `cargo test -p jeryu -- git_system`
//! Invariants: The resolver never falls back to a hard-coded `/usr/bin/git`.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[derive(Debug, Clone)]
pub struct SystemGit {
    pub path: PathBuf,
}

impl SystemGit {
    pub fn resolve() -> Result<Self> {
        if let Some(path) = std::env::var("JERYU_SYSTEM_GIT")
            .ok()
            .filter(|path| !path_is_recursive_bridge(Path::new(path)))
        {
            return Ok(Self {
                path: PathBuf::from(path),
            });
        }

        if let Some(path) = crate::settings::get()
            .git
            .system_git
            .clone()
            .filter(|path| !path_is_recursive_bridge(Path::new(path)))
        {
            return Ok(Self {
                path: PathBuf::from(path),
            });
        }

        if let Some(path) = find_git_on_path() {
            return Ok(Self {
                path: PathBuf::from(path),
            });
        }

        Err(anyhow::anyhow!("unable to resolve system git binary"))
    }

    pub fn command(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut command = Command::new(&self.path);
        command.current_dir(cwd);
        command.args(args);
        command.env("JERYU_GIT_RECURSION_GUARD", "1");
        command.stdin(Stdio::inherit());
        command.stdout(Stdio::inherit());
        command.stderr(Stdio::inherit());
        command
    }

    pub fn output(&self, cwd: &Path, args: &[&str]) -> Result<Output> {
        self.command(cwd, args)
            .output()
            .with_context(|| format!("running git {:?}", args))
    }

    pub fn status(&self, cwd: &Path, args: &[&str]) -> Result<std::process::ExitStatus> {
        self.command(cwd, args)
            .status()
            .with_context(|| format!("running git {:?}", args))
    }
}

fn find_git_on_path() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("git");
        if candidate.is_file() && !path_is_recursive_bridge(&candidate) {
            return Some(candidate.display().to_string());
        }
    }
    None
}

fn path_is_recursive_bridge(path: &Path) -> bool {
    if path_matches_current_exe(path) {
        return true;
    }

    std::fs::read_to_string(path)
        .map(|script| script.contains("jeryu git") || script.contains("jeryu\" \"git"))
        .unwrap_or(false)
}

fn path_matches_current_exe(path: &Path) -> bool {
    let Ok(current) = std::env::current_exe() else {
        return false;
    };
    let current = match std::fs::canonicalize(current) {
        Ok(value) => value,
        Err(_) => PathBuf::new(),
    };
    let candidate = match std::fs::canonicalize(path) {
        Ok(value) => value,
        Err(_) => path.to_path_buf(),
    };
    !current.as_os_str().is_empty() && current == candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{LazyLock, Mutex};

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn set_env(key: &str, value: impl AsRef<std::ffi::OsStr>) {
        // SAFETY: tests isolate the process environment with a mutex.
        unsafe {
            std::env::set_var(key, value);
        }
    }

    fn remove_env(key: &str) {
        // SAFETY: tests isolate the process environment with a mutex.
        unsafe {
            std::env::remove_var(key);
        }
    }

    fn write_executable(path: &Path, body: &str) {
        fs::write(path, body).unwrap();
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    fn path_lookup_is_optional() {
        let _ = find_git_on_path();
    }

    #[test]
    fn env_resolution_is_not_globally_cached() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("git-one");
        let second = dir.path().join("git-two");
        write_executable(&first, "#!/usr/bin/env sh\nexit 0\n");
        write_executable(&second, "#!/usr/bin/env sh\nexit 0\n");

        set_env("JERYU_SYSTEM_GIT", &first);
        let resolved_first = SystemGit::resolve().unwrap();
        set_env("JERYU_SYSTEM_GIT", &second);
        let resolved_second = SystemGit::resolve().unwrap();
        remove_env("JERYU_SYSTEM_GIT");

        assert_eq!(resolved_first.path, first);
        assert_eq!(resolved_second.path, second);
    }

    #[test]
    fn guarded_path_lookup_skips_jeryu_git_bridge() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let bridge_dir = dir.path().join("bridge");
        let real_dir = dir.path().join("real");
        fs::create_dir_all(&bridge_dir).unwrap();
        fs::create_dir_all(&real_dir).unwrap();
        let bridge = bridge_dir.join("git");
        let real = real_dir.join("git");
        write_executable(&bridge, "#!/usr/bin/env sh\nexec jeryu git \"$@\"\n");
        write_executable(&real, "#!/usr/bin/env sh\nexit 0\n");

        let original_path = std::env::var_os("PATH");
        set_env(
            "PATH",
            std::env::join_paths([bridge_dir, real_dir]).unwrap(),
        );
        set_env("JERYU_GIT_RECURSION_GUARD", "1");
        remove_env("JERYU_SYSTEM_GIT");

        let resolved = SystemGit::resolve().unwrap();

        match original_path {
            Some(path) => set_env("PATH", path),
            None => remove_env("PATH"),
        }
        remove_env("JERYU_GIT_RECURSION_GUARD");

        assert_eq!(resolved.path, real);
    }
}
