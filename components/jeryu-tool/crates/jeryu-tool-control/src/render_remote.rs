use super::*;

pub(super) fn git_remote_url(root: &Path, name: &str) -> Result<Option<String>, String> {
    let mut command = local_git_command(root);
    command.args(["remote", "get-url", name]);
    let output = command.output().map_err(|error| {
        format!(
            "renderer custody check failed for {}: {error}",
            root.display()
        )
    })?;
    if output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        ));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("No such remote") {
        Ok(None)
    } else {
        Err(format!(
            "renderer custody check failed for {}: {}",
            root.display(),
            stderr.trim()
        ))
    }
}

pub(super) fn protected_main_commit(
    tool_root: &Path,
    name: &str,
    authenticated: bool,
    contract_base_ref: Option<&str>,
    release_ci: bool,
) -> Result<String, String> {
    if let Some(origin) = git_remote_url(tool_root, "origin")? {
        require_canonical_hosted_origin(name, &origin)?;
        if authenticated {
            return remote_main(name);
        }
        let commit = git_local_output(tool_root, &["rev-parse", "refs/remotes/origin/main"])?;
        if !regex("^[0-9a-f]{40}$").is_match(&commit) {
            return Err("renderer could not resolve tracked protected jeryu-tool main".to_owned());
        }
        return Ok(commit);
    }

    if let Some(base) = contract_base_ref {
        if !regex("^[0-9a-f]{40}$").is_match(base) {
            return Err(format!(
                "harness JAIN_CONTRACT_BASE_REF is not a full commit sha: {base}"
            ));
        }
        git_local_output(
            tool_root,
            &["cat-file", "-e", &format!("{base}^{{commit}}")],
        )?;
        return Ok(base.to_owned());
    }

    if authenticated {
        return remote_main(name);
    }

    if release_ci {
        return Err(
            "release renderer custody requires harness-authenticated JAIN_CONTRACT_BASE_REF \
when origin is absent"
                .to_owned(),
        );
    }

    Err(format!(
        "renderer custody check failed for {}: no origin remote and no JAIN_CONTRACT_BASE_REF",
        tool_root.display()
    ))
}

pub(super) fn hosted_git_command(authenticated_origin: &str, askpass: &Path) -> Command {
    let mut command = Command::new(GIT_BIN);
    command
        .current_dir("/")
        .args([
            "-c",
            "credential.helper=",
            "-c",
            "credential.useHttpPath=true",
            "-c",
            "http.followRedirects=false",
            "-c",
            "http.sslVerify=true",
            "-c",
            "http.proxy=",
            "ls-remote",
            "--heads",
        ])
        .arg(authenticated_origin)
        .arg("refs/heads/main");
    scrub_git_environment(&mut command);
    command
        .env("GIT_ASKPASS", askpass)
        .env("JERYU_TOOL_GIT_ASKPASS", "1");
    command
}

pub(super) fn remote_main(name: &str) -> Result<String, String> {
    let expected_origin = canonical_hosted_origin(name)?;
    let authenticated_origin = expected_origin
        .strip_prefix("https://")
        .map(|suffix| format!("https://git@{suffix}"))
        .ok_or_else(|| "renderer canonical hosted origin is not HTTPS".to_owned())?;
    let askpass = held_askpass_executable()?;
    let mut command = hosted_git_command(&authenticated_origin, &askpass);
    let output = command
        .output()
        .map_err(|_| format!("renderer could not resolve protected main for {name}"))?;
    if !output.status.success() {
        return Err(format!(
            "renderer could not resolve protected main for {name}"
        ));
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() != 2
        || fields[1] != "refs/heads/main"
        || !regex("^[0-9a-f]{40}$").is_match(fields[0])
    {
        return Err(format!(
            "renderer could not resolve protected main for {name}"
        ));
    }
    Ok(fields[0].to_owned())
}

pub(super) fn sha256_bytes(bytes: &[u8]) -> Result<String, String> {
    let mut child = Command::new(SHA256_BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("renderer could not start SHA-256 verifier: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "renderer SHA-256 verifier has no input pipe".to_owned())?
        .write_all(bytes)
        .map_err(|error| format!("renderer could not stream SHA-256 input: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("renderer SHA-256 verifier failed: {error}"))?;
    if !output.status.success() {
        return Err("renderer SHA-256 verifier failed".to_owned());
    }
    let digest = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_owned();
    if !regex("^[0-9a-f]{64}$").is_match(&digest) {
        return Err("renderer SHA-256 verifier returned malformed output".to_owned());
    }
    Ok(digest)
}
