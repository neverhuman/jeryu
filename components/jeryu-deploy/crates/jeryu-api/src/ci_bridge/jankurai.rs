//! Installed Jankurai identity verification and authoritative score ingestion.

use super::*;

/// Verify a physical auditor file before every authoritative score. An explicit
/// path supports the offline sandbox image, but it never relaxes identity and
/// there is no ambient PATH or network-install fallback.
pub(super) fn verify_jankurai_identity(
    path: &Path,
    expected_version: &str,
    expected_sha256: &str,
) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("governed jankurai path is not absolute".to_string());
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("governed jankurai metadata failed: {error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("governed jankurai is not a physical regular file".to_string());
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|error| format!("governed jankurai canonicalization failed: {error}"))?;
    if canonical != path {
        return Err("governed jankurai path traverses a symlink".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.nlink() != 1 {
            return Err("governed jankurai has multiple physical links".to_string());
        }
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("governed jankurai is not executable".to_string());
        }
    }
    let bytes =
        std::fs::read(path).map_err(|error| format!("governed jankurai read failed: {error}"))?;
    let digest = hex::encode(Sha256::digest(bytes));
    if digest != expected_sha256 {
        return Err(format!("governed jankurai digest mismatch: {digest}"));
    }
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|error| format!("governed jankurai version failed: {error}"))?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || version != expected_version {
        return Err(format!("governed jankurai version mismatch: {version}"));
    }
    Ok(path.to_path_buf())
}

pub(super) fn require_exact_object_keys(
    value: &serde_json::Value,
    label: &str,
    expected: &[&str],
) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("governed jankurai receipt authority mismatch: {label}"))?;
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        return Err(format!(
            "governed jankurai receipt authority mismatch: {label} keys"
        ));
    }
    Ok(())
}

pub(super) fn verify_jankurai_receipt(
    receipt_path: &Path,
    binary_path: &Path,
    expected_version: &str,
    expected_sha256: &str,
) -> Result<(), String> {
    if !receipt_path.is_absolute() {
        return Err("governed jankurai receipt path is not absolute".to_string());
    }
    let metadata = std::fs::symlink_metadata(receipt_path)
        .map_err(|error| format!("governed jankurai receipt metadata failed: {error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("governed jankurai receipt is not a physical regular file".to_string());
    }
    let canonical = std::fs::canonicalize(receipt_path)
        .map_err(|error| format!("governed jankurai receipt canonicalization failed: {error}"))?;
    if canonical != receipt_path {
        return Err("governed jankurai receipt path traverses a symlink".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err("governed jankurai receipt has multiple physical links".to_string());
        }
    }
    let bytes = std::fs::read(receipt_path)
        .map_err(|error| format!("governed jankurai receipt read failed: {error}"))?;
    let digest = hex::encode(Sha256::digest(&bytes));
    let named_digest = receipt_path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if named_digest.len() != 64
        || !named_digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        || digest != named_digest
    {
        return Err("governed jankurai receipt content address mismatch".to_string());
    }
    let document: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("governed jankurai receipt JSON failed: {error}"))?;
    let top_level_keys: &[&str] = if document.get("timestamp").is_some() {
        &[
            "binary",
            "build",
            "conclusion",
            "governance",
            "installation",
            "operator",
            "run_id",
            "schema",
            "source",
            "test_mode",
            "timestamp",
        ]
    } else {
        &[
            "binary",
            "build",
            "conclusion",
            "governance",
            "installation",
            "operator",
            "run_id",
            "schema",
            "source",
            "test_mode",
        ]
    };
    require_exact_object_keys(&document, "/", top_level_keys)?;
    require_exact_object_keys(
        document
            .pointer("/source")
            .unwrap_or(&serde_json::Value::Null),
        "/source",
        &[
            "archive_sha256",
            "cargo_lock_sha256",
            "commit",
            "remote",
            "tag",
            "tree",
            "verification",
        ],
    )?;
    require_exact_object_keys(
        document
            .pointer("/governance")
            .unwrap_or(&serde_json::Value::Null),
        "/governance",
        &[
            "manifest_commit",
            "manifest_repo",
            "manifest_sha256",
            "manifest_tree",
            "protected_main",
            "protection_policy",
            "status",
        ],
    )?;
    require_exact_object_keys(
        document
            .pointer("/binary")
            .unwrap_or(&serde_json::Value::Null),
        "/binary",
        &["sha256", "version_output"],
    )?;
    for pointer in ["/operator", "/run_id"] {
        if document
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err(format!(
                "governed jankurai receipt authority mismatch: {pointer}"
            ));
        }
    }
    if document.get("timestamp").is_some()
        && document
            .pointer("/timestamp")
            .and_then(serde_json::Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err("governed jankurai receipt authority mismatch: /timestamp".to_string());
    }
    let governed_receipt: serde_json::Value =
        serde_json::from_str(GOVERNED_JANKURAI_INSTALLATION_RECEIPT_JSON)
            .map_err(|error| format!("governed jankurai build authority JSON failed: {error}"))?;
    let governed_build = governed_receipt
        .pointer("/build")
        .ok_or_else(|| "governed jankurai build authority is missing /build".to_string())?;
    if document.pointer("/build") != Some(governed_build) {
        return Err("governed jankurai receipt authority mismatch: /build".to_string());
    }
    let expected_strings = [
        ("/schema", "jeryu.jankurai-installation/v2"),
        ("/source/remote", GOVERNED_JANKURAI_SOURCE_REPO),
        ("/source/tag", GOVERNED_JANKURAI_SOURCE_TAG),
        ("/source/commit", GOVERNED_JANKURAI_SOURCE_REV),
        ("/source/tree", GOVERNED_JANKURAI_SOURCE_TREE),
        (
            "/source/archive_sha256",
            GOVERNED_JANKURAI_SOURCE_ARCHIVE_SHA256,
        ),
        (
            "/source/cargo_lock_sha256",
            GOVERNED_JANKURAI_CARGO_LOCK_SHA256,
        ),
        ("/source/verification", "release-authoritative"),
        ("/build/rustc", GOVERNED_JANKURAI_RUSTC_VERSION),
        ("/build/cargo", GOVERNED_JANKURAI_CARGO_VERSION),
        ("/build/target_triple", GOVERNED_JANKURAI_TARGET_TRIPLE),
        ("/build/mode", GOVERNED_JANKURAI_BUILD_MODE),
        ("/build/no_proxy", "127.0.0.1,localhost,::1"),
        ("/governance/status", "governed"),
        ("/governance/manifest_repo", GOVERNED_JANKURAI_MANIFEST_REPO),
        (
            "/governance/manifest_commit",
            GOVERNED_JANKURAI_MANIFEST_COMMIT,
        ),
        ("/governance/manifest_tree", GOVERNED_JANKURAI_MANIFEST_TREE),
        (
            "/governance/manifest_sha256",
            GOVERNED_JANKURAI_MANIFEST_SHA256,
        ),
        ("/governance/protection_policy", "immutable-main-v1"),
        ("/binary/sha256", expected_sha256),
        ("/binary/version_output", expected_version),
        (
            "/installation/path",
            binary_path.to_str().unwrap_or_default(),
        ),
        ("/conclusion", "success"),
    ];
    for (pointer, expected) in expected_strings {
        if document
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            != Some(expected)
        {
            return Err(format!(
                "governed jankurai receipt authority mismatch: {pointer}"
            ));
        }
    }
    let expected_true = [
        "/build/cargo_net_offline",
        "/build/git_global_config_disabled",
        "/build/git_system_config_disabled",
        "/governance/protected_main",
        "/installation/atomic",
    ];
    for pointer in expected_true {
        if document
            .pointer(pointer)
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        {
            return Err(format!(
                "governed jankurai receipt authority mismatch: {pointer}"
            ));
        }
    }
    let expected_false = [
        "/build/git_http_follow_redirects",
        "/build/git_terminal_prompt",
        "/build/jankurai_update_check",
        "/test_mode",
    ];
    for pointer in expected_false {
        if document
            .pointer(pointer)
            .and_then(serde_json::Value::as_bool)
            != Some(false)
        {
            return Err(format!(
                "governed jankurai receipt authority mismatch: {pointer}"
            ));
        }
    }
    Ok(())
}

pub(super) fn verify_jankurai_authority(
    path: &Path,
    receipt_paths: &[PathBuf],
    expected_version: &str,
    expected_sha256: &str,
) -> Result<PathBuf, String> {
    let binary = verify_jankurai_identity(path, expected_version, expected_sha256)?;
    let mut failures = Vec::new();
    for receipt in receipt_paths {
        match verify_jankurai_receipt(receipt, &binary, expected_version, expected_sha256) {
            Ok(()) => return Ok(binary),
            Err(error) => failures.push(error),
        }
    }
    Err(format!(
        "governed jankurai has no matching installation receipt: {}",
        failures.join("; ")
    ))
}

pub(super) fn jankurai_bin() -> Result<PathBuf, String> {
    let path = std::env::var_os("JERYU_JANKURAI_BIN")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(GOVERNED_JANKURAI_PATH));
    let mut receipt_paths = if let Some(receipt) =
        std::env::var_os("JERYU_JANKURAI_RECEIPT").filter(|value| !value.is_empty())
    {
        vec![PathBuf::from(receipt)]
    } else if path == Path::new(GOVERNED_JANKURAI_PATH) {
        std::fs::read_dir(GOVERNED_JANKURAI_RECEIPT_DIR)
            .map_err(|error| format!("governed jankurai receipt directory failed: {error}"))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|candidate| candidate.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect()
    } else {
        return Err("non-default governed jankurai requires an explicit receipt".to_string());
    };
    receipt_paths.sort();
    verify_jankurai_authority(
        &path,
        &receipt_paths,
        GOVERNED_JANKURAI_VERSION,
        GOVERNED_JANKURAI_SHA256,
    )
}

/// THE GUARANTEE (Layer 2). Compute the authoritative jankurai diff-score for a
/// pushed head on the HOST — which has the real trunk and the forge DB, neither of
/// which the `--network none` agent cell can reach — record it through the
/// automatic host path, and publish the `jankurai/proof` check-run derived from
/// it. The separate external ingest route is admin-only maintenance authority.
///
/// Diff-only against the host-computed merge-base (fast, `changed_fast`). Strict:
/// the proof passes only when score ≥ floor AND no hard findings AND no NEW caps.
/// Best-effort throughout — a clone/audit failure records a `tool-failed` score or
/// is skipped, but never blocks the push (it runs on the receive-pack pool).
pub(super) fn record_authoritative_jankurai_score(
    core: &ForgeCore,
    git_bin: &str,
    bare: &Path,
    owner: &str,
    repo: &str,
    update: &RefUpdate,
) {
    record_authoritative_jankurai_score_with(
        core,
        git_bin,
        bare,
        owner,
        repo,
        update,
        jankurai_bin,
    );
}

pub(super) fn record_authoritative_jankurai_score_with<F>(
    core: &ForgeCore,
    git_bin: &str,
    bare: &Path,
    owner: &str,
    repo: &str,
    update: &RefUpdate,
    resolve_jankurai: F,
) where
    F: FnOnce() -> Result<PathBuf, String>,
{
    if update.new_oid == ZERO_OID {
        return; // ref delete: nothing to score
    }
    let Some(branch) = update.ref_name.strip_prefix("refs/heads/") else {
        return; // only branch heads are scored
    };
    // Never use a stored score as proof that this host performed the audit. An
    // administrative backfill, an interrupted earlier attempt, or legacy state
    // may already have written this SHA. Recompute first; Core's
    // (branch, commit_sha) upsert keeps the durable score bounded, and the
    // newly completed check becomes the exact-head authority.

    // Automatically removed standalone clone of the head; it is never registered
    // as a Git worktree and never touches the live bare.
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let sandbox = std::env::temp_dir().join(format!(
        "jeryu-jankurai-{owner}-{repo}-{}-{suffix}-{}",
        std::process::id(),
        update.new_oid
    ));
    let _ = std::fs::remove_dir_all(&sandbox);
    let bare_str = bare.to_string_lossy().to_string();
    let sandbox_str = sandbox.to_string_lossy().to_string();
    if !run_git_status(
        git_bin,
        None,
        &[
            "clone",
            "--quiet",
            "--no-local",
            "--no-checkout",
            &bare_str,
            &sandbox_str,
        ],
    ) {
        let _ = std::fs::remove_dir_all(&sandbox);
        return;
    }
    if !run_git_status(
        git_bin,
        Some(&sandbox),
        &["checkout", "--quiet", "--detach", &update.new_oid],
    ) {
        let _ = std::fs::remove_dir_all(&sandbox);
        return;
    }

    // Merge-base against the REAL trunk (the bare has refs/heads/main; the cell
    // never could). For the first main ref or an orphaned branch, materialize the
    // empty tree in this disposable clone so every file is audited. Using the new
    // head itself as the first-main base would produce an empty false-green diff.
    let base = if branch == "main" {
        (update.old_oid != ZERO_OID)
            .then(|| update.old_oid.clone())
            .or_else(|| write_empty_tree(git_bin, &sandbox))
    } else {
        run_git_stdout(
            git_bin,
            Some(bare),
            &["merge-base", "refs/heads/main", &update.new_oid],
        )
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| write_empty_tree(git_bin, &sandbox))
    };

    // Forced scoring for unconfigured repos (Part D): if the head carries no policy
    // of its own, drop the jeryu-managed default into the throwaway sandbox (never
    // tracked, never committed — untracked files do not enter the diff set) so the
    // repo still gets a real floor and a real verdict.
    if !sandbox.join("agent/audit-policy.toml").exists() {
        let _ = std::fs::create_dir_all(sandbox.join("agent"));
        let _ = std::fs::write(
            sandbox.join("agent/audit-policy.toml"),
            DEFAULT_AUDIT_POLICY_TOML,
        );
    }
    let skip_proof = !sandbox.join("agent/owner-map.json").exists();

    // Run the pinned auditor. --advisory-only: always write the JSON and exit 0; we
    // derive the strict verdict from the JSON ourselves.
    let out_json = sandbox.join("target/jankurai/diff/diff-score.json");
    if let Some(parent) = out_json.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let out_json_str = out_json.to_string_lossy().to_string();
    let exit_code = match (base.as_deref(), resolve_jankurai()) {
        (Some(base), Ok(jankurai)) => {
            let mut command = Command::new(&jankurai);
            command
                .arg("diff-audit")
                .arg(&sandbox_str)
                .arg("--base-ref")
                .arg(base);
            command
                .arg("--json")
                .arg(&out_json_str)
                .arg("--advisory-only");
            if skip_proof {
                command.arg("--skip-proof");
            }
            command
                .status()
                .ok()
                .and_then(|status| status.code())
                .map(i64::from)
                .unwrap_or(-1)
        }
        (Some(_), Err(error)) => {
            eprintln!("authoritative Jankurai identity rejected: {error}");
            -1
        }
        (None, _) => {
            eprintln!("authoritative Jankurai base resolution failed");
            -1
        }
    };

    let report: Option<serde_json::Value> = std::fs::read(&out_json)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    let (request, pass) = jankurai_score_request(branch, &update.new_oid, report, exit_code);

    if let Err(error) = core.record_jankurai_score(owner, repo, request) {
        eprintln!("authoritative Jankurai score persistence failed: {error}");
        let _ = std::fs::remove_dir_all(&sandbox);
        return;
    }
    let conclusion = if pass {
        CheckConclusion::Success
    } else {
        CheckConclusion::Failure
    };
    let _ = core.create_check_run(
        owner,
        repo,
        CreateCheckRunRequest {
            name: "jankurai/proof".to_string(),
            head_sha: update.new_oid.clone(),
            status: Some(CheckRunStatus::Completed),
            conclusion: Some(conclusion),
            ..Default::default()
        },
    );

    let _ = std::fs::remove_dir_all(&sandbox);
}

pub(super) fn jankurai_score_request(
    branch: &str,
    commit_sha: &str,
    report: Option<serde_json::Value>,
    exit_code: i64,
) -> (RecordJankuraiScoreRequest, bool) {
    let parsed = report.as_ref().and_then(|report| {
        if exit_code != 0 {
            return None;
        }
        let score = u32::try_from(report.get("score")?.as_u64()?).ok()?;
        let decision = report.get("decision")?;
        let hard_findings = u32::try_from(decision.get("hard_findings")?.as_u64()?).ok()?;
        let minimum_score = u32::try_from(decision.get("minimum_score")?.as_u64()?).ok()?;
        if score > 100 || minimum_score > 100 {
            return None;
        }
        let caps_applied = report
            .get("caps_applied")?
            .as_array()?
            .iter()
            .map(|cap| cap.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()?;
        Some((score, hard_findings, minimum_score, caps_applied))
    });

    match parsed {
        Some((score, hard_findings, minimum_score, caps_applied)) => {
            let effective_floor = minimum_score.max(HOST_JANKURAI_MINIMUM_SCORE);
            let pass = score >= effective_floor && hard_findings == 0 && caps_applied.is_empty();
            (
                RecordJankuraiScoreRequest {
                    branch: branch.to_string(),
                    commit_sha: commit_sha.to_string(),
                    score: Some(score),
                    hard_findings: Some(hard_findings),
                    decision: "scored".to_string(),
                    caps_applied,
                    report,
                    tool_exit: None,
                },
                pass,
            )
        }
        None => (
            RecordJankuraiScoreRequest {
                branch: branch.to_string(),
                commit_sha: commit_sha.to_string(),
                score: None,
                hard_findings: None,
                decision: "tool-failed".to_string(),
                caps_applied: Vec::new(),
                report,
                tool_exit: Some(exit_code),
            },
            false,
        ),
    }
}
