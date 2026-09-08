use super::*;

pub(super) fn manifest_authority(
    tool_root: &Path,
    authenticated: bool,
) -> Result<ManifestAuthority, String> {
    let contract_base_ref = env::var("JAIN_CONTRACT_BASE_REF").ok();
    let release_ci = env::var("JAIN_RELEASE_CI").ok().as_deref() == Some("1");
    let commit = protected_main_commit(
        tool_root,
        "jeryu-tool",
        authenticated,
        contract_base_ref.as_deref(),
        release_ci,
    )?;
    let manifest = fs::read(tool_root.join("tool-manifest.toml"))
        .map_err(|error| format!("failed to read tool-manifest.toml: {error}"))?;
    let object = format!("{commit}:tool-manifest.toml");
    let protected_manifest = git_local_output_bytes(tool_root, &["show", &object])?;
    if manifest != protected_manifest {
        return Err(
            "tool-manifest.toml must land on protected jeryu-tool main before sandbox receipt rendering"
                .to_owned(),
        );
    }
    let tree_object = format!("{commit}^{{tree}}");
    let tree = git_local_output(tool_root, &["rev-parse", &tree_object])?;
    if !regex("^[0-9a-f]{40}$").is_match(&tree) {
        return Err("renderer resolved malformed jeryu-tool manifest tree".to_owned());
    }
    Ok(ManifestAuthority {
        commit,
        tree,
        sha256: sha256_bytes(&protected_manifest)?,
    })
}

pub(super) fn validate_repository_storage(root: &Path) -> Result<(), String> {
    let dot_git = root.join(".git");
    let metadata = fs::symlink_metadata(&dot_git).map_err(|_| {
        format!(
            "renderer write root lacks repository-local .git: {}",
            root.display()
        )
    })?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(format!(
            "renderer write root must have an ordinary repository-local .git directory: {}",
            root.display()
        ));
    }
    let common = git_local_output(root, &["rev-parse", "--git-common-dir"])?;
    let common = PathBuf::from(common);
    let common = if common.is_absolute() {
        common
    } else {
        root.join(common)
    };
    let common = fs::canonicalize(common).map_err(|_| {
        format!(
            "renderer write root has unresolved Git common directory: {}",
            root.display()
        )
    })?;
    let expected = fs::canonicalize(&dot_git).map_err(|_| {
        format!(
            "renderer write root has unresolved repository-local .git: {}",
            root.display()
        )
    })?;
    if common != expected {
        return Err(format!(
            "renderer write root uses an alternate Git common directory: {}",
            root.display()
        ));
    }
    let registered = git_local_output(root, &["worktree", "list", "--porcelain"])?;
    let worktrees: Vec<&str> = registered
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .collect();
    if worktrees.len() != 1 || Path::new(worktrees[0]) != root {
        return Err(format!(
            "renderer write root repository has alternate registered worktrees: {}",
            root.display()
        ));
    }
    let top = fs::canonicalize(git_local_output(root, &["rev-parse", "--show-toplevel"])?)
        .map_err(|_| {
            format!(
                "renderer write root has unresolved top level: {}",
                root.display()
            )
        })?;
    if top != root {
        return Err(format!(
            "renderer write root top-level mismatch: expected={} actual={}",
            root.display(),
            top.display()
        ));
    }
    Ok(())
}

pub(super) fn validate_index_state(name: &str, root: &Path) -> Result<(), String> {
    let entries = git_local_output_bytes(root, &["ls-files", "-v", "-z"])?;
    if !entries.is_empty() && entries.last() != Some(&0) {
        return Err(format!(
            "renderer write root has malformed tracked index state: {name}={}",
            root.display()
        ));
    }
    if entries.is_empty() {
        return Ok(());
    }
    let entries = entries.strip_suffix(&[0]).unwrap_or(entries.as_slice());
    for entry in entries.split(|byte| *byte == 0) {
        if entry.len() < 3 || entry[0] != b'H' || entry[1] != b' ' {
            return Err(format!(
                "renderer write root has tracked index suppression or nonordinary state: \
{name}={} entry={:?}",
                root.display(),
                String::from_utf8_lossy(entry)
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn validate_write_target(root: &Path, path: &Path) -> Result<(), String> {
    let fail = || {
        format!(
            "renderer write target must be a stable regular HEAD blob: {}",
            path.display()
        )
    };
    let relative = path.strip_prefix(root).map_err(|_| fail())?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(fail());
    }
    let relative = relative.to_str().ok_or_else(fail)?;
    if fs::canonicalize(path).map_err(|_| fail())? != path {
        return Err(fail());
    }
    let before = fs::symlink_metadata(path).map_err(|_| fail())?;
    if !before.file_type().is_file() || before.file_type().is_symlink() || before.nlink() != 1 {
        return Err(fail());
    }

    let entry = git_local_output_bytes(
        root,
        &["ls-tree", "-z", "--full-tree", "HEAD", "--", relative],
    )?;
    if entry.last() != Some(&0) || entry[..entry.len() - 1].contains(&0) {
        return Err(fail());
    }
    let entry = &entry[..entry.len() - 1];
    let tab = entry
        .iter()
        .position(|byte| *byte == b'\t')
        .ok_or_else(fail)?;
    if &entry[tab + 1..] != relative.as_bytes() {
        return Err(fail());
    }
    let header = std::str::from_utf8(&entry[..tab]).map_err(|_| fail())?;
    let fields: Vec<&str> = header.split_whitespace().collect();
    if fields.len() != 3
        || !matches!(fields[0], "100644" | "100755")
        || fields[1] != "blob"
        || !regex("^[0-9a-f]{40}$").is_match(fields[2])
    {
        return Err(fail());
    }
    let working = git_local_output(root, &["hash-object", "--no-filters", "--", relative])?;
    if working != fields[2] {
        return Err(fail());
    }
    let after = fs::symlink_metadata(path).map_err(|_| fail())?;
    if !same_file_identity(&before, &after) {
        return Err(fail());
    }
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn validate_write_target(_root: &Path, path: &Path) -> Result<(), String> {
    Err(format!(
        "renderer write target custody requires Unix metadata: {}",
        path.display()
    ))
}

pub(super) fn validate_write_targets(
    repository_targets: &[(PathBuf, Vec<PathBuf>)],
) -> Result<(), String> {
    for (root, targets) in repository_targets {
        let name = root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("<unknown>");
        validate_index_state(name, root)?;
        for target in targets {
            validate_write_target(root, target)?;
        }
    }
    Ok(())
}

pub(super) fn apply_rendered_changes(
    repository_targets: &[(PathBuf, Vec<PathBuf>)],
    rendered: &[(PathBuf, String)],
) -> Result<(), String> {
    validate_write_targets(repository_targets)?;
    let allowed: BTreeSet<&Path> = repository_targets
        .iter()
        .flat_map(|(_, targets)| targets.iter().map(PathBuf::as_path))
        .collect();
    let mut unique = BTreeSet::new();
    for (path, _) in rendered {
        if !allowed.contains(path.as_path()) || !unique.insert(path.as_path()) {
            return Err(format!(
                "renderer attempted an unvalidated or duplicate write target: {}",
                path.display()
            ));
        }
    }
    for (path, value) in rendered {
        fs::write(path, value)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    }
    Ok(())
}

pub(super) fn validate_write_root(
    name: &str,
    root: &Path,
    family_root: &Path,
    expected_head: &str,
) -> Result<(), String> {
    let expected_root = family_root.join(name);
    if root != expected_root {
        return Err(format!(
            "renderer write root is not the canonical physical checkout: {name}={} expected={}",
            root.display(),
            expected_root.display()
        ));
    }
    validate_repository_storage(root)?;
    validate_index_state(name, root)?;
    if !git_local_output(root, &["status", "--porcelain", "--untracked-files=all"])?.is_empty() {
        return Err(format!(
            "renderer write root must start clean: {name}={}",
            root.display()
        ));
    }
    let origin = git_local_output(root, &["remote", "get-url", "origin"])?;
    require_canonical_hosted_origin(name, &origin)?;
    let head = git_local_output(root, &["rev-parse", "HEAD"])?;
    if head != expected_head {
        return Err(format!(
            "renderer write root HEAD mismatch: {name} expected={expected_head} actual={head}"
        ));
    }
    let protected_main = remote_main(name)?;
    let mut ancestry = local_git_command(root);
    let status = ancestry
        .args(["merge-base", "--is-ancestor", &protected_main, &head])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("renderer ancestry check failed: {error}"))?;
    if !status.success() {
        return Err(format!(
            "renderer write root is not based on current protected main: {name} main={} head={head}",
            protected_main
        ));
    }
    if !git_local_output(
        root,
        &["rev-list", "--merges", &format!("{protected_main}..{head}")],
    )?
    .is_empty()
    {
        return Err(format!(
            "renderer write root contains non-linear commits after protected main: {name}"
        ));
    }
    Ok(())
}
