use super::*;

pub(super) fn scrub_git_environment(command: &mut Command) {
    for key in SCRUBBED_GIT_ENVIRONMENT {
        command.env_remove(key);
    }
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("NO_PROXY", DIRECT_HOST_NO_PROXY)
        .env("no_proxy", DIRECT_HOST_NO_PROXY);
}

pub(super) fn local_git_command(root: &Path) -> Command {
    let mut command = Command::new(GIT_BIN);
    command.arg("-C").arg(root).args([
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "credential.helper=",
        "-c",
        "diff.external=",
        "-c",
        "submodule.recurse=false",
    ]);
    scrub_git_environment(&mut command);
    command
}

pub(super) fn git_local_output_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = local_git_command(root);
    command.args(args);
    let output = command.output().map_err(|error| {
        format!(
            "renderer custody check failed for {}: {error}",
            root.display()
        )
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        return Err(format!(
            "renderer custody check failed for {}: {}",
            root.display(),
            if stderr.is_empty() { stdout } else { stderr }
        ));
    }
    Ok(output.stdout)
}

pub(super) fn git_local_output(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = git_local_output_bytes(root, args)?;
    Ok(String::from_utf8_lossy(&output).trim().to_owned())
}

#[cfg(unix)]
pub(super) fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.mode() == right.mode()
        && left.uid() == right.uid()
        && left.gid() == right.gid()
        && left.nlink() == right.nlink()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

#[cfg(unix)]
pub(super) fn read_token_file_with_hook(
    path: &Path,
    after_open: impl FnOnce(),
) -> Result<String, String> {
    let fail = || {
        format!(
            "renderer credential file failed custody validation: {}",
            path.display()
        )
    };
    if !path.is_absolute() || fs::canonicalize(path).map_err(|_| fail())? != path {
        return Err(fail());
    }
    let before = fs::symlink_metadata(path).map_err(|_| fail())?;
    let process_uid = fs::metadata("/proc/self").map_err(|_| fail())?.uid();
    if !before.file_type().is_file()
        || before.file_type().is_symlink()
        || before.nlink() != 1
        || before.uid() != process_uid
        || before.mode() & 0o777 != 0o600
        || before.len() == 0
        || before.len() > 16_384
    {
        return Err(fail());
    }
    let mut file = File::open(path).map_err(|_| fail())?;
    let opened = file.metadata().map_err(|_| fail())?;
    if !same_file_identity(&before, &opened) {
        return Err(fail());
    }
    after_open();
    let mut value = String::new();
    (&mut file)
        .take(16_385)
        .read_to_string(&mut value)
        .map_err(|_| fail())?;
    let after = file.metadata().map_err(|_| fail())?;
    let path_after = fs::symlink_metadata(path).map_err(|_| fail())?;
    if !same_file_identity(&opened, &after)
        || !same_file_identity(&after, &path_after)
        || value.len() > 16_384
    {
        return Err(fail());
    }
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(fail());
    }
    Ok(value.to_owned())
}

#[cfg(not(unix))]
pub(super) fn read_token_file_with_hook(
    path: &Path,
    _after_open: impl FnOnce(),
) -> Result<String, String> {
    Err(format!(
        "renderer credential custody requires Unix metadata: {}",
        path.display()
    ))
}

pub(super) fn read_forge_token() -> Result<String, String> {
    let path = env::var_os("JERYU_FORGE_TOKEN_FILE")
        .map(PathBuf::from)
        .ok_or_else(|| {
            "renderer authenticated hosted read requires JERYU_FORGE_TOKEN_FILE".to_owned()
        })?;
    read_token_file_with_hook(&path, || {})
}

pub(super) fn canonical_askpass_prompt(prompt: &str) -> bool {
    let Some(path) = prompt
        .strip_prefix("Password for 'https://git@git.neverhuman.org/git/jeryu/")
        .and_then(|value| value.strip_suffix("': "))
    else {
        return false;
    };
    let Some(name) = path.strip_suffix(".git") else {
        return false;
    };
    CANONICAL_REPOS.contains(&name)
}

pub(crate) fn git_askpass(raw_args: &[String]) -> Result<String, String> {
    if raw_args.len() != 1 || !canonical_askpass_prompt(&raw_args[0]) {
        return Err("renderer hosted credential prompt failed custody validation".to_owned());
    }
    read_forge_token()
}

#[cfg(unix)]
pub(super) fn held_askpass_executable() -> Result<PathBuf, String> {
    let path = PathBuf::from(format!("/proc/{}/exe", std::process::id()));
    let metadata = fs::metadata(&path)
        .map_err(|_| "renderer could not resolve held credential helper".to_owned())?;
    if !metadata.file_type().is_file() || metadata.mode() & 0o111 == 0 {
        return Err("renderer could not resolve held credential helper".to_owned());
    }
    Ok(path)
}

#[cfg(not(unix))]
pub(super) fn held_askpass_executable() -> Result<PathBuf, String> {
    Err("renderer hosted credential helper requires Unix process custody".to_owned())
}
