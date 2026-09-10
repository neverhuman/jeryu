//! Exact Git change inventory. This does not authenticate a protected baseline or run proofs.
use super::*;
use serde_json::{Value, json};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;

const LIMIT: u64 = 64 * 1024 * 1024;
const MAPS: [&str; 4] = [
    "agent/owner-map.json",
    "agent/test-map.json",
    "agent/proof-lanes.toml",
    "agent/audit-policy.toml",
];

#[derive(Debug)]
struct InventoryArgs {
    root: PathBuf,
    base: String,
    head: String,
    out: PathBuf,
}

fn parse_args(raw: &[String]) -> Result<InventoryArgs, String> {
    let mut values = BTreeMap::new();
    if raw.len() != 8 {
        return Err(
            "inventory requires --monorepo-root, --base, --head and --out once each".into(),
        );
    }
    for pair in raw.chunks_exact(2) {
        if !matches!(
            pair[0].as_str(),
            "--monorepo-root" | "--base" | "--head" | "--out"
        ) || values.insert(pair[0].as_str(), pair[1].as_str()).is_some()
        {
            return Err("unknown or duplicate inventory argument".into());
        }
    }
    let args = InventoryArgs {
        root: PathBuf::from(values["--monorepo-root"]),
        base: values["--base"].to_owned(),
        head: values["--head"].to_owned(),
        out: PathBuf::from(values["--out"]),
    };
    if !args.root.is_absolute()
        || !args.out.is_absolute()
        || !oid(&args.base)
        || !oid(&args.head)
        || args.base == args.head
    {
        return Err(
            "inventory requires absolute paths and two distinct full lowercase 40-hex commits"
                .into(),
        );
    }
    Ok(args)
}

fn oid(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|b| {
            [
                char::from(DIGITS[usize::from(b >> 4)]),
                char::from(DIGITS[usize::from(b & 15)]),
            ]
        })
        .collect()
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    // No ambient credentials, executable hooks, replacement objects or lazy network fetches.
    let mut child = Command::new(GIT_BIN)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "credential.helper=",
            "-c",
            "diff.external=",
            "-c",
            "diff.orderFile=/dev/null",
            "-c",
            "submodule.recurse=false",
        ])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("inventory Git could not start: {e}"))?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .ok_or("inventory Git has no stdout")?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() as u64 > LIMIT {
        let _ = child.kill();
        let _ = child.wait();
        return Err("inventory Git output failed or exceeded 64 MiB; no paths were omitted".into());
    }
    if !child.wait().map_err(|e| e.to_string())?.success() {
        return Err(format!(
            "inventory Git {} failed",
            args.first().unwrap_or(&"read")
        ));
    }
    Ok(bytes)
}

fn git_text(root: &Path, args: &[&str]) -> Result<String, String> {
    String::from_utf8(git(root, args)?)
        .map(|s| s.trim_end_matches('\n').to_owned())
        .map_err(|_| "inventory Git identity is not UTF-8".into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    mode: String,
    object: String,
}
type Tree = BTreeMap<Vec<u8>, Entry>;

fn valid_path(path: &[u8]) -> bool {
    !path.is_empty()
        && !path.contains(&0)
        && path
            .split(|b| *b == b'/')
            .all(|part| !part.is_empty() && part != b"." && part != b"..")
}

fn records(bytes: &[u8]) -> Result<Vec<&[u8]>, String> {
    if bytes.is_empty() {
        return Ok(vec![]);
    }
    Ok(bytes
        .strip_suffix(&[0])
        .ok_or("truncated NUL-delimited Git inventory")?
        .split(|b| *b == 0)
        .collect())
}

fn parse_tree(bytes: &[u8]) -> Result<Tree, String> {
    let mut tree = Tree::new();
    for record in records(bytes)? {
        let tab = record
            .iter()
            .position(|b| *b == b'\t')
            .ok_or("tree record lacks pathname")?;
        let fields: Vec<_> = std::str::from_utf8(&record[..tab])
            .map_err(|_| "invalid tree header")?
            .split(' ')
            .collect();
        let path = &record[tab + 1..];
        if fields.len() != 3
            || !oid(fields[2])
            || !valid_path(path)
            || !matches!(
                (fields[0], fields[1]),
                ("100644" | "100755" | "120000", "blob") | ("160000", "commit")
            )
        {
            return Err("invalid tree record".into());
        }
        if tree
            .insert(
                path.to_vec(),
                Entry {
                    mode: fields[0].into(),
                    object: fields[2].into(),
                },
            )
            .is_some()
        {
            return Err("duplicate Git tree path".into());
        }
    }
    Ok(tree)
}

#[derive(Debug)]
struct Change {
    status: String,
    old: Option<Vec<u8>>,
    new: Option<Vec<u8>>,
}

fn parse_changes(bytes: &[u8], old: &Tree, new: &Tree) -> Result<Vec<Change>, String> {
    let fields = records(bytes)?;
    let mut cursor = 0;
    let mut changes = Vec::new();
    let mut covered = BTreeSet::new();
    while cursor < fields.len() {
        let header = std::str::from_utf8(fields[cursor]).map_err(|_| "invalid raw diff header")?;
        let parts: Vec<_> = header
            .strip_prefix(':')
            .ok_or("missing raw diff colon")?
            .split(' ')
            .collect();
        cursor += 1;
        if parts.len() != 5 || !oid(parts[2]) || !oid(parts[3]) {
            return Err("invalid raw diff fields".into());
        }
        let status = parts[4];
        let paired = status.starts_with('R') || status.starts_with('C');
        if paired {
            let score = status[1..]
                .parse::<u8>()
                .map_err(|_| "invalid rename/copy score")?;
            if !(1..=100).contains(&score) {
                return Err("invalid rename/copy score".into());
            }
        } else if !matches!(status, "A" | "D" | "M" | "T") {
            return Err("unsupported raw diff status".into());
        }
        let first = *fields.get(cursor).ok_or("missing raw diff path")?;
        cursor += 1;
        let second = if paired {
            let path = *fields
                .get(cursor)
                .ok_or("missing rename/copy destination")?;
            cursor += 1;
            path
        } else {
            first
        };
        if !valid_path(first) || !valid_path(second) || (paired && first == second) {
            return Err("invalid raw diff path".into());
        }
        let old_path = if status == "A" {
            None
        } else {
            Some(first.to_vec())
        };
        let new_path = if status == "D" {
            None
        } else {
            Some(second.to_vec())
        };
        for (tree, path, mode, object) in [
            (old, old_path.as_ref(), parts[0], parts[2]),
            (new, new_path.as_ref(), parts[1], parts[3]),
        ] {
            match path {
                Some(path)
                    if tree
                        .get(path)
                        .is_some_and(|entry| entry.mode == mode && entry.object == object) => {}
                None if mode == "000000" && object.bytes().all(|b| b == b'0') => {}
                _ => return Err("raw diff disagrees with exact Git trees".into()),
            }
        }
        if (status == "A" || paired) && old.contains_key(second)
            || (status == "D" || status.starts_with('R')) && new.contains_key(first)
        {
            return Err("raw diff status contradicts tree path presence".into());
        }
        let affected: Vec<&Vec<u8>> = if status.starts_with('C') {
            new_path.iter().collect()
        } else {
            old_path
                .iter()
                .chain(new_path.iter())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        };
        for path in affected {
            if old.get(path) == new.get(path) || !covered.insert(path.clone()) {
                return Err("duplicate or unchanged path in raw diff".into());
            }
        }
        changes.push(Change {
            status: status.into(),
            old: old_path,
            new: new_path,
        });
    }
    let expected: BTreeSet<_> = old
        .keys()
        .chain(new.keys())
        .filter(|path| old.get(*path) != new.get(*path))
        .cloned()
        .collect();
    if covered != expected {
        return Err("raw diff omitted or invented changed paths".into());
    }
    Ok(changes)
}

fn blob(root: &Path, object: &str) -> Result<Vec<u8>, String> {
    let size = git_text(root, &["cat-file", "-s", object])?
        .parse::<u64>()
        .map_err(|_| "invalid blob size")?;
    if size > LIMIT {
        return Err("source blob exceeds inventory 64 MiB limit".into());
    }
    let data = git(root, &["cat-file", "blob", object])?;
    if data.len() as u64 != size {
        return Err("blob size changed during inventory".into());
    }
    Ok(data)
}

fn endpoint(
    root: &Path,
    tree: &Tree,
    path: Option<&Vec<u8>>,
) -> Result<(Value, Option<Vec<u8>>), String> {
    let Some(path) = path else {
        return Ok((Value::Null, None));
    };
    let entry = tree.get(path).ok_or("missing source tree entry")?;
    let (kind, data) = match entry.mode.as_str() {
        "160000" => ("gitlink", None),
        "120000" => ("symlink", Some(blob(root, &entry.object)?)),
        _ => ("regular", Some(blob(root, &entry.object)?)),
    };
    let bytes = data.as_ref().map(Vec::len);
    let sha = data.as_ref().map(|b| sha256_bytes(b)).transpose()?;
    let binary = data
        .as_ref()
        .map(|b| b.contains(&0) || std::str::from_utf8(b).is_err());
    Ok((
        json!({"path_hex":hex(path),"path_utf8":std::str::from_utf8(path).ok(),"mode":entry.mode,
        "git_object":entry.object,"kind":kind,"bytes":bytes,"sha256":sha,"binary":binary}),
        data,
    ))
}

fn line_count(bytes: &[u8]) -> usize {
    bytes.iter().filter(|b| **b == b'\n').count()
        + usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"))
}

fn range(value: &str, prefix: char) -> Result<(usize, usize), String> {
    let value = value.strip_prefix(prefix).ok_or("invalid hunk side")?;
    let (start, count) = value.split_once(',').unwrap_or((value, "1"));
    let start = start.parse::<usize>().map_err(|_| "invalid hunk start")?;
    let count = count.parse::<usize>().map_err(|_| "invalid hunk count")?;
    if count != 0 && start == 0 {
        return Err("nonempty hunk starts at zero".into());
    }
    Ok((start, count))
}

fn parse_hunks(patch: &[u8], old_lines: usize, new_lines: usize) -> Result<Vec<Value>, String> {
    let text = std::str::from_utf8(patch).map_err(|_| "text diff is not UTF-8")?;
    let mut hunks = Vec::new();
    let mut pending = (0_usize, 0_usize);
    let mut previous = (0_usize, 0_usize);
    for line in text.lines() {
        if line.starts_with("@@ ") {
            if pending != (0, 0) {
                return Err("truncated hunk body".into());
            }
            let header = line
                .strip_prefix("@@ ")
                .and_then(|s| s.split_once(" @@"))
                .ok_or("malformed hunk header")?
                .0;
            let (left, right) = header.split_once(' ').ok_or("hunk lacks both sides")?;
            let (a, b) = range(left, '-')?;
            let (c, d) = range(right, '+')?;
            let old_position = if b == 0 { a } else { a - 1 };
            let new_position = if d == 0 { c } else { c - 1 };
            let old_end = old_position.checked_add(b).ok_or("hunk overflow")?;
            let new_end = new_position.checked_add(d).ok_or("hunk overflow")?;
            if (b == 0 && d == 0)
                || old_end > old_lines
                || new_end > new_lines
                || old_position < previous.0
                || new_position < previous.1
            {
                return Err("hunk outside exact source lines or out of order".into());
            }
            previous = (old_end, new_end);
            pending = (b, d);
            hunks.push(json!({"old_start":a,"old_count":b,"new_start":c,"new_count":d}));
        } else if !hunks.is_empty() {
            if line == "\\ No newline at end of file" {
                continue;
            }
            let used = match line.as_bytes().first() {
                Some(b'-') => (1, 0),
                Some(b'+') => (0, 1),
                Some(b' ') => (1, 1),
                _ => return Err("unexpected text diff body".into()),
            };
            pending.0 = pending
                .0
                .checked_sub(used.0)
                .ok_or("extra old hunk lines")?;
            pending.1 = pending
                .1
                .checked_sub(used.1)
                .ok_or("extra new hunk lines")?;
        }
    }
    if pending != (0, 0) || (hunks.is_empty() && !patch.is_empty()) {
        return Err("incomplete text diff".into());
    }
    Ok(hunks)
}

fn hunks(
    root: &Path,
    old: &Value,
    new: &Value,
    a: Option<&[u8]>,
    b: Option<&[u8]>,
) -> Result<Value, String> {
    if [old, new]
        .iter()
        .any(|e| !e.is_null() && (e["kind"] != "regular" || e["binary"] != false))
    {
        return Ok(Value::Null);
    }
    match (a, b) {
        (Some(a), Some(b)) if a == b => Ok(json!([])),
        (Some(a), Some(b)) => {
            let left = old["git_object"].as_str().ok_or("old blob missing")?;
            let right = new["git_object"].as_str().ok_or("new blob missing")?;
            let patch = git(
                root,
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-color",
                    "--no-relative",
                    "--unified=0",
                    "--diff-algorithm=myers",
                    "--no-indent-heuristic",
                    left,
                    right,
                    "--",
                ],
            )?;
            Ok(json!(parse_hunks(&patch, line_count(a), line_count(b))?))
        }
        (None, Some(b)) if !b.is_empty() => {
            Ok(json!([{"old_start":0,"old_count":0,"new_start":1,"new_count":line_count(b)}]))
        }
        (Some(a), None) if !a.is_empty() => {
            Ok(json!([{"old_start":1,"old_count":line_count(a),"new_start":0,"new_count":0}]))
        }
        _ => Ok(json!([])),
    }
}

fn maps(root: &Path, tree: &Tree) -> Result<Value, String> {
    let mut maps = Vec::new();
    for name in MAPS {
        let path = name.as_bytes().to_vec();
        let entry = tree
            .get(&path)
            .ok_or_else(|| format!("missing root proof input {name}"))?;
        if entry.mode != "100644" && entry.mode != "100755" {
            return Err(format!("root proof input must be a regular blob: {name}"));
        }
        let data = blob(root, &entry.object)?;
        let text = std::str::from_utf8(&data).map_err(|_| "root proof input is not UTF-8")?;
        if name.ends_with(".json") {
            let _: Value =
                serde_json::from_str(text).map_err(|e| format!("invalid root map: {e}"))?;
        } else {
            let _: toml::Value =
                toml::from_str(text).map_err(|e| format!("invalid root proof TOML: {e}"))?;
        }
        maps.push(json!({"path":name,"git_blob":entry.object,"sha256":sha256_bytes(&data)?,"bytes":data.len()}));
    }
    Ok(json!(maps))
}

fn inventory(tool_root: &Path, args: &InventoryArgs) -> Result<Value, String> {
    if fs::canonicalize(tool_root).map_err(|e| e.to_string())? != args.root.join(TOOL_DIRECTORY) {
        return Err(
            "inventory Tool root must be components/jeryu-tool of the admitted monorepo".into(),
        );
    }
    for path in [
        ".git/info/grafts",
        ".git/objects/info/alternates",
        ".git/shallow",
    ] {
        if fs::symlink_metadata(args.root.join(path)).is_ok() {
            return Err(format!(
                "inventory refuses incomplete or redirected history: {path}"
            ));
        }
    }
    let initial = snapshot(&args.root, Some(&args.head), true)?;
    if git_text(
        &args.root,
        &["rev-parse", &format!("{}^{{commit}}", args.base)],
    )? != args.base
        || git_text(
            &args.root,
            &["rev-parse", &format!("{}^{{commit}}", args.head)],
        )? != args.head
    {
        return Err("inventory input is not the exact commit object".into());
    }
    git(
        &args.root,
        &["merge-base", "--is-ancestor", &args.base, &args.head],
    )?;
    let base_tree = git_text(
        &args.root,
        &["rev-parse", &format!("{}^{{tree}}", args.base)],
    )?;
    let old = parse_tree(&git(
        &args.root,
        &["ls-tree", "-rz", "--full-tree", &args.base],
    )?)?;
    let new = parse_tree(&git(
        &args.root,
        &["ls-tree", "-rz", "--full-tree", &args.head],
    )?)?;
    let raw = git(
        &args.root,
        &[
            "diff",
            "--raw",
            "-z",
            "--no-abbrev",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-relative",
            "--ignore-submodules=none",
            "--find-renames=50%",
            "--find-copies=50%",
            "--find-copies-harder",
            "-l1000",
            &args.base,
            &args.head,
            "--",
        ],
    )?;
    let changes = parse_changes(&raw, &old, &new)?;
    let mut output = Vec::new();
    for change in changes {
        let (old_endpoint, a) = endpoint(&args.root, &old, change.old.as_ref())?;
        let (new_endpoint, b) = endpoint(&args.root, &new, change.new.as_ref())?;
        let spans = hunks(
            &args.root,
            &old_endpoint,
            &new_endpoint,
            a.as_deref(),
            b.as_deref(),
        )?;
        output.push(json!({"status":change.status,"old":old_endpoint,"new":new_endpoint,"text_hunks":spans}));
    }
    let old_maps = maps(&args.root, &old)?;
    let new_maps = maps(&args.root, &new)?;
    if snapshot(&args.root, Some(&args.head), true)? != initial {
        return Err("inventory source changed during read".into());
    }
    Ok(
        json!({"schema":"jeryu.proof-change-inventory/v1","qualification":"change-inventory-only",
        "predecessor_authentication":"not-performed","routing":"not-performed","full_proof":false,"protected_main":false,
        "base":{"commit":args.base,"tree":base_tree,"root_proof_inputs":old_maps},
        "head":{"commit":args.head,"tree":initial.tree,"root_proof_inputs":new_maps},
        "policy":{"paths":"hex-encoded original Git pathname bytes; optional lossless UTF-8 display",
            "diff":"two exact ancestor commits; Git rename/copy 50%, harder copies, limit1000; undetected pairs remain complete additions/deletions",
            "text_hunks":"regular UTF-8 blobs without NUL; Myers unified0, no indent heuristic; old and new coordinates",
            "non_text":"binary, symlink and gitlink spans are null and require separate proof admission",
            "git_version":git_text(&args.root, &["--version"])?},"changes":output}),
    )
}

fn private_output(args: &InventoryArgs) -> Result<(File, PathBuf, fs::Metadata), String> {
    if args.out.starts_with(&args.root) {
        return Err("inventory output must be outside the checkout".into());
    }
    let parent = args.out.parent().ok_or("inventory output has no parent")?;
    let name = args
        .out
        .file_name()
        .ok_or("inventory output has no filename")?;
    if fs::canonicalize(parent).map_err(|e| e.to_string())? != parent {
        return Err("inventory output parent must be physical".into());
    }
    let metadata = fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
    let uid = fs::metadata("/proc/self").map_err(|e| e.to_string())?.uid();
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o7777 != 0o700 {
        return Err("inventory output parent must be owner-held mode0700".into());
    }
    let directory = File::open(parent).map_err(|e| e.to_string())?;
    if !same_file_identity(&metadata, &directory.metadata().map_err(|e| e.to_string())?) {
        return Err("inventory output parent changed while opening".into());
    }
    let anchored = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd())).join(name);
    Ok((directory, anchored, metadata))
}

fn run_with_hook(
    tool_root: &Path,
    raw: &[String],
    after_write: impl FnOnce(),
) -> Result<i32, String> {
    let args = parse_args(raw)?;
    let (directory, anchored, admitted_parent) = private_output(&args)?;
    let value = inventory(tool_root, &args)?;
    let bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&anchored)
        .map_err(|e| format!("inventory output must be new: {e}"))?;
    file.write_all(&bytes)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    let written = file.metadata().map_err(|e| e.to_string())?;
    // Match the existing Tool after-open hook style: production is a no-op;
    // tests mutate the real admitted parent or output before final readback.
    after_write();
    directory.sync_all().map_err(|e| e.to_string())?;
    let parent_path = args.out.parent().ok_or("missing output parent")?;
    let parent = fs::symlink_metadata(parent_path).map_err(|e| e.to_string())?;
    let held_parent = directory.metadata().map_err(|e| e.to_string())?;
    let held_file = file.metadata().map_err(|e| e.to_string())?;
    let path_file = fs::symlink_metadata(&args.out).map_err(|e| e.to_string())?;
    let uid = fs::metadata("/proc/self").map_err(|e| e.to_string())?.uid();
    // Directory size and timestamps legitimately change when the file is
    // created. Its admitted inode, owner, group and complete mode do not.
    let parent_identity = |metadata: &fs::Metadata| {
        (
            metadata.dev(),
            metadata.ino(),
            metadata.uid(),
            metadata.gid(),
            metadata.mode(),
        )
    };
    if fs::canonicalize(parent_path).map_err(|e| e.to_string())? != parent_path
        || !parent.is_dir()
        || parent.uid() != uid
        || parent.mode() & 0o7777 != 0o700
        || parent_identity(&parent) != parent_identity(&admitted_parent)
        || parent_identity(&held_parent) != parent_identity(&admitted_parent)
        || !held_file.is_file()
        || held_file.uid() != uid
        || held_file.mode() & 0o7777 != 0o600
        || held_file.nlink() != 1
        || !same_file_identity(&written, &held_file)
        || !same_file_identity(&held_file, &path_file)
    {
        return Err(
            "inventory output custody changed; retained diagnostic output is not admitted".into(),
        );
    }
    println!(
        "change inventory written; predecessor authentication, routing and proof qualification remain pending"
    );
    Ok(0)
}

pub(super) fn run(tool_root: &Path, raw: &[String]) -> Result<i32, String> {
    run_with_hook(tool_root, raw, || {})
}

#[cfg(test)]
#[path = "proof_inventory_tests.rs"]
mod tests;
