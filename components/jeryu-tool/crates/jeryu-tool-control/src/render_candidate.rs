use super::*;
use crate::render_rules::render_candidate_consumer;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom};

const MONOREPO_REPOSITORY: &str = "https://github.com/neverhuman/jeryu.git";
const TOOL_DIRECTORY: &str = "components/jeryu-tool";
const MANIFEST_PATH: &str = "components/jeryu-tool/tool-manifest.toml";
const PIN_PATH: &str = "components/jeryu-tool/generated/jankurai-pin.env";
const TEMPLATE_PATH: &str = "components/jeryu-tool/ops/render-assets/require-jankurai.sh";

fn candidate_git_bytes(root: &Path, arguments: &[&str]) -> Result<Vec<u8>, String> {
    let output = local_git_command(root)
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env_remove("GIT_REPLACE_REF_BASE")
        .args(arguments)
        .output()
        .map_err(|error| format!("candidate Git read failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "candidate Git read failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn candidate_git(root: &Path, arguments: &[&str]) -> Result<String, String> {
    String::from_utf8(candidate_git_bytes(root, arguments)?)
        .map(|text| text.trim().to_owned())
        .map_err(|_| "candidate Git returned non-UTF-8 output".to_owned())
}

#[derive(Debug, Default)]
struct CandidateArgs {
    root: PathBuf,
    write: bool,
    expected_head: Option<String>,
    repos: BTreeSet<String>,
}

fn candidate_args(raw: &[String]) -> Result<CandidateArgs, String> {
    let mut args = CandidateArgs::default();
    let mut mode = None;
    let mut index = 0;
    while index < raw.len() {
        let flag = raw[index].as_str();
        match flag {
            "--check" | "--write" => {
                if mode.replace(flag == "--write").is_some() {
                    return Err(
                        "candidate rendering requires one --check or --write mode".to_owned()
                    );
                }
            }
            "--monorepo-root" | "--expected-head" | "--repo" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| format!("{flag} requires a value"))?;
                match flag {
                    "--monorepo-root" => {
                        if !args.root.as_os_str().is_empty() {
                            return Err("duplicate --monorepo-root".to_owned());
                        }
                        args.root = PathBuf::from(value);
                    }
                    "--expected-head" => {
                        if !regex("^[0-9a-f]{40}$").is_match(value)
                            || args.expected_head.replace(value.clone()).is_some()
                        {
                            return Err("--expected-head requires one 40-hex commit".to_owned());
                        }
                    }
                    "--repo" => {
                        if !CANONICAL_REPOS.contains(&value.as_str())
                            || !args.repos.insert(value.clone())
                        {
                            return Err(format!("invalid or duplicate candidate --repo {value:?}"));
                        }
                    }
                    _ => unreachable!(),
                }
            }
            _ => return Err(format!("unrecognized candidate renderer argument: {flag}")),
        }
        index += 1;
    }
    if !args.root.is_absolute() {
        return Err("candidate rendering requires --monorepo-root ABSOLUTE_PATH".to_owned());
    }
    args.write = mode.unwrap_or(false);
    if args.write && args.expected_head.is_none() {
        return Err("candidate writes require --expected-head".to_owned());
    }
    if args.repos.is_empty() {
        args.repos
            .extend(CANONICAL_REPOS.iter().map(|name| (*name).to_owned()));
    }
    Ok(args)
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    head: String,
    tree: String,
}

fn snapshot(root: &Path, expected: Option<&str>, clean: bool) -> Result<Snapshot, String> {
    if !root.is_absolute() || fs::canonicalize(root).map_err(|error| error.to_string())? != root {
        return Err("candidate source must be one physical absolute monorepo path".to_owned());
    }
    #[cfg(unix)]
    if fs::metadata(root).map_err(|error| error.to_string())?.uid()
        != fs::metadata("/proc/self")
            .map_err(|error| error.to_string())?
            .uid()
    {
        return Err("candidate source must be owned by the invoking user".to_owned());
    }
    validate_repository_storage(root)?;
    validate_index_state("jeryu-monorepo", root)?;
    let head = candidate_git(root, &["rev-parse", "HEAD"])?;
    let tree = candidate_git(root, &["rev-parse", "HEAD^{tree}"])?;
    if !regex("^[0-9a-f]{40}$").is_match(&head) || !regex("^[0-9a-f]{40}$").is_match(&tree) {
        return Err("candidate source has malformed commit/tree identity".to_owned());
    }
    if expected.is_some_and(|expected| expected != head) {
        return Err(format!(
            "candidate HEAD mismatch: expected={} actual={head}",
            expected.unwrap_or("")
        ));
    }
    if clean
        && !candidate_git(root, &["status", "--porcelain", "--untracked-files=all"])?.is_empty()
    {
        return Err(
            "candidate source must be clean; commit generated changes before qualification"
                .to_owned(),
        );
    }
    Ok(Snapshot { head, tree })
}

fn committed_text(root: &Path, head: &str, relative: &str) -> Result<String, String> {
    let path = root.join(relative);
    validate_write_target(root, &path)?;
    let object = format!("{head}:{relative}");
    let committed = candidate_git_bytes(root, &["show", &object])?;
    let fail = || format!("candidate input differs from its exact committed blob: {relative}");
    let before = fs::symlink_metadata(&path).map_err(|_| fail())?;
    let mut file = fs::File::open(&path).map_err(|_| fail())?;
    if !same_file_identity(&before, &file.metadata().map_err(|_| fail())?) {
        return Err(fail());
    }
    let mut working = Vec::new();
    file.read_to_end(&mut working).map_err(|_| fail())?;
    if working != committed
        || !same_file_identity(&before, &file.metadata().map_err(|_| fail())?)
        || !same_file_identity(&before, &fs::symlink_metadata(&path).map_err(|_| fail())?)
        || fs::canonicalize(&path).map_err(|_| fail())? != path
    {
        return Err(fail());
    }
    String::from_utf8(committed).map_err(|_| format!("candidate input is not UTF-8: {relative}"))
}

fn candidate_path(relative: &Path) -> bool {
    let fixed = [
        "ops/ci/ensure-jankurai.sh",
        "ops/ci/lib.sh",
        "ops/ci/common.sh",
        "ops/ci/coverage.sh",
        "ops/ci/pr-ci.sh",
        "ci-fast-push.sh",
        "agent/ci-lanes.toml",
        "agent/audit-policy.toml",
        "scripts/ci-doctor.sh",
        "policy/default-audit-policy.toml",
    ];
    fixed.iter().any(|name| relative == Path::new(name))
        || (relative.parent() == Some(Path::new(".github/workflows"))
            && relative
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| regex(r"^[A-Za-z0-9][A-Za-z0-9_.-]*\.yml$").is_match(name)))
}

fn targets(tool_root: &Path, args: &CandidateArgs) -> Result<Vec<PathBuf>, String> {
    if tool_root != args.root.join(TOOL_DIRECTORY)
        || fs::canonicalize(tool_root).map_err(|error| error.to_string())? != tool_root
    {
        return Err(
            "candidate --tool-root must be the physical components/jeryu-tool directory".to_owned(),
        );
    }
    let mut targets = BTreeSet::new();
    for name in &args.repos {
        let root = if name == "jeryu" {
            args.root.clone()
        } else {
            args.root.join("components").join(name)
        };
        if fs::canonicalize(&root).map_err(|error| error.to_string())? != root {
            return Err(format!("candidate component has a path alias: {name}"));
        }
        if name != "jeryu" && fs::symlink_metadata(root.join(".git")).is_ok() {
            return Err(format!(
                "candidate component contains a nested checkout: {name}"
            ));
        }
        if !root.join("ops/ci/lib.sh").is_file() {
            return Err(format!(
                "candidate component is missing its CI identity consumer: {name}"
            ));
        }
        for path in consumer_paths(&root)? {
            if candidate_path(
                path.strip_prefix(&root)
                    .map_err(|error| error.to_string())?,
            ) {
                targets.insert(path);
            }
        }
    }
    if args.repos.contains("jeryu-tool") {
        targets.insert(args.root.join(PIN_PATH));
    }
    Ok(targets.into_iter().collect())
}

struct Change {
    path: PathBuf,
    original: String,
    expected: String,
}

fn write_changes(root: &Path, initial: &Snapshot, changes: &[Change]) -> Result<(), String> {
    let mut held = Vec::new();
    for change in changes
        .iter()
        .filter(|change| change.original != change.expected)
    {
        validate_write_target(root, &change.path)?;
        let before = fs::symlink_metadata(&change.path).map_err(|error| error.to_string())?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&change.path)
            .map_err(|error| {
                format!(
                    "cannot hold candidate output {}: {error}",
                    change.path.display()
                )
            })?;
        if !same_file_identity(
            &before,
            &file.metadata().map_err(|error| error.to_string())?,
        ) {
            return Err("candidate output changed while opening".to_owned());
        }
        let mut original = String::new();
        file.read_to_string(&mut original)
            .map_err(|error| error.to_string())?;
        if original != change.original {
            return Err("candidate output changed after planning".to_owned());
        }
        held.push((file, before, change));
    }
    if snapshot(root, Some(&initial.head), true)? != *initial {
        return Err("candidate source changed before rendering".to_owned());
    }
    for (file, before, change) in &held {
        if !same_file_identity(before, &file.metadata().map_err(|error| error.to_string())?)
            || !same_file_identity(
                before,
                &fs::symlink_metadata(&change.path).map_err(|error| error.to_string())?,
            )
            || fs::canonicalize(&change.path).map_err(|error| error.to_string())? != change.path
        {
            return Err("candidate output custody changed before rendering".to_owned());
        }
    }
    for (file, _, change) in &mut held {
        file.seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        file.write_all(change.expected.as_bytes())
            .map_err(|error| error.to_string())?;
        file.set_len(change.expected.len() as u64)
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        if !same_file_identity(
            &file.metadata().map_err(|error| error.to_string())?,
            &fs::symlink_metadata(&change.path).map_err(|error| error.to_string())?,
        ) {
            return Err(
                "candidate output path changed during rendering; inspect the worktree".to_owned(),
            );
        }
    }
    if snapshot(root, Some(&initial.head), false)? != *initial {
        return Err(
            "candidate source identity changed during rendering; inspect the worktree".to_owned(),
        );
    }
    Ok(())
}

fn execute(tool_root: &Path, raw_args: &[String]) -> Result<(i32, String), String> {
    let args = candidate_args(raw_args)?;
    let initial = snapshot(&args.root, args.expected_head.as_deref(), true)?;
    let targets = targets(tool_root, &args)?;
    let manifest = committed_text(&args.root, &initial.head, MANIFEST_PATH)?;
    let pin = Pin::parse(&manifest)?;
    let parsed: toml::Value = toml::from_str(&manifest).map_err(|error| error.to_string())?;
    let distribution = parsed
        .get("distribution")
        .and_then(|value| value.get("source_repository"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| "candidate rendering requires schema 2 public distribution".to_owned())?;
    let template = committed_text(&args.root, &initial.head, TEMPLATE_PATH)?;
    if !template.starts_with("require_jankurai() {\n") || !template.ends_with("}\n") {
        return Err("candidate verifier template has an invalid function boundary".to_owned());
    }
    let mut changes = Vec::new();
    for path in targets {
        let relative = path
            .strip_prefix(&args.root)
            .map_err(|error| error.to_string())?
            .to_str()
            .ok_or_else(|| "candidate path is not UTF-8".to_owned())?;
        let original = committed_text(&args.root, &initial.head, relative)?;
        let expected = if relative == PIN_PATH {
            pin.env_text()
        } else {
            render_candidate_consumer(&path, &original, &pin, template.trim_end())?
        };
        changes.push(Change {
            path,
            original,
            expected,
        });
    }
    let consumers = changes
        .iter()
        .map(|change| {
            Ok(serde_json::json!({
                "path": change.path.strip_prefix(&args.root).map_err(|error| error.to_string())?,
                "before_sha256": sha256_bytes(change.original.as_bytes())?,
                "expected_sha256": sha256_bytes(change.expected.as_bytes())?,
                "changed": change.original != change.expected,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let drift = changes
        .iter()
        .any(|change| change.original != change.expected);
    let manifest_blob = candidate_git(
        &args.root,
        &["rev-parse", &format!("{}:{MANIFEST_PATH}", initial.head)],
    )?;
    let mut provenance = serde_json::json!({
        "schema": "jeryu.jankurai-candidate-render/v1",
        "mode": if args.write { "write" } else { "check" },
        "scope": {
            "repositories": args.repos,
            "complete": args.repos.len() == CANONICAL_REPOS.len(),
        },
        "source": {
            "repository": MONOREPO_REPOSITORY,
            "commit": initial.head,
            "tree": initial.tree,
            "clean_at_start": true,
        },
        "manifest": {
            "path": MANIFEST_PATH,
            "blob": manifest_blob,
            "sha256": sha256_bytes(manifest.as_bytes())?,
        },
        "distribution": {
            "repository": distribution,
            "tag": pin.get("tag"),
            "commit": pin.get("rev"),
            "tree": pin.get("source_tree"),
        },
        "producer_repository": pin.get("repo"),
        "governance": {
            "protected_main": false,
            "handover": "pending",
            "predecessor_authentication": "not-performed",
        },
        "verification": {
            "build": "not-performed",
            "installation": "not-performed",
            "public_readback": "not-performed",
        },
        "generated_pin_sha256": sha256_bytes(pin.env_text().as_bytes())?,
        "consumers": consumers,
        "drift": drift,
    });
    if snapshot(&args.root, Some(&initial.head), true)? != initial {
        return Err("candidate source changed while planning render".to_owned());
    }
    // The receipt binds these bytes, including when workspace dependencies
    // enable serde_json's insertion-order representation.
    provenance.sort_all_objects();
    let output = serde_json::to_string_pretty(&provenance).map_err(|error| error.to_string())?;
    if args.write {
        write_changes(&args.root, &initial, &changes)?;
    }
    Ok((i32::from(!args.write && drift), format!("{output}\n")))
}

pub(super) fn run(tool_root: &Path, raw_args: &[String]) -> Result<i32, String> {
    let (code, output) = execute(tool_root, raw_args)?;
    print!("{output}");
    Ok(code)
}

#[cfg(test)]
#[path = "render_candidate_tests.rs"]
mod tests;

#[cfg(unix)]
#[path = "proof_inventory.rs"]
mod inventory;

pub(super) fn run_inventory(tool_root: &Path, raw: &[String]) -> Result<i32, String> {
    #[cfg(unix)]
    {
        inventory::run(tool_root, raw)
    }
    #[cfg(not(unix))]
    {
        let _ = (tool_root, raw);
        Err("source inventory requires Linux filesystem custody".into())
    }
}
