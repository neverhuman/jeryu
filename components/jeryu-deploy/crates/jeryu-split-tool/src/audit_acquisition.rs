//! Anonymous, disposable public source acquisition; never release admission.
use super::{Source, oid, snapshot, write_json};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[path = "audit_source_custody.rs"]
mod custody;
#[path = "audit_source_graph.rs"]
mod graph;

const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 2 * MAX_FILE_BYTES;
const MAX_SOURCE_ENTRIES: usize = 200_000;

#[derive(Debug)]
pub(super) struct TimedOut;
impl std::fmt::Display for TimedOut {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("public acquisition timed out")
    }
}
impl std::error::Error for TimedOut {}

pub(super) struct Acquired {
    scratch: PathBuf,
    source: PathBuf,
    identity: (u64, u64, u32),
    input_identity: (u64, u64, u32),
    source_identity: (u64, u64, u32),
    commit: String,
    tree: String,
    graph: String,
    session: Session,
    timeout_seconds: u64,
    pub(super) receipt: Value,
}

impl Acquired {
    pub(super) fn path(&self) -> &Path {
        &self.source
    }

    pub(super) fn commit(&self) -> &str {
        &self.commit
    }

    pub(super) fn tree(&self) -> &str {
        &self.tree
    }

    /// Only a completed successful scope may remove its disposable clone.
    /// All other paths deliberately retain it; there is no destructive Drop.
    pub(super) fn finish(mut self, succeeded: bool) -> Result<()> {
        custody::unchanged_directory(&self.scratch, self.identity)?;
        custody::unchanged_directory(&self.scratch.join("input.git"), self.input_identity)?;
        custody::unchanged_directory(&self.source, self.source_identity)?;
        self.session.deadline = Instant::now() + Duration::from_secs(self.timeout_seconds);
        ensure!(
            graph::verify(
                &mut self.session,
                &self.scratch.join("input.git"),
                &self.commit,
                true
            )? == self.graph
                && graph::verify(&mut self.session, &self.source, &self.commit, false)?
                    == self.graph,
            "public source object graph changed after audit"
        );
        if !succeeded {
            eprintln!(
                "retaining unsuccessful public audit source: {}",
                self.source.display()
            );
            return Ok(());
        }
        ensure!(
            snapshot(&self.source)? == (self.commit, self.tree),
            "public source changed after audit"
        );
        custody::remove(&self.scratch, self.identity)
    }
}

fn url(repository: &str) -> Result<String> {
    let name = repository
        .strip_prefix("neverhuman/")
        .context("public source owner is unresolved")?;
    ensure!(
        !name.is_empty()
            && name.len() <= 100
            && name.as_bytes()[0].is_ascii_alphanumeric()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            && !name.ends_with(".git"),
        "invalid public repository slug"
    );
    Ok(format!("https://github.com/{repository}.git"))
}

fn maintained_ref(root: &Path, source: &Source) -> Result<Option<String>> {
    if let Some(commit) = &source.commit {
        oid(commit)?;
        return Ok(None);
    }
    ensure!(
        source.scope == "standalone",
        "supporting source needs an immutable commit; current main is not a pin"
    );
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("repos.manifest.toml"))?)?;
    let matching: Vec<_> = manifest
        .get("repo")
        .and_then(toml::Value::as_array)
        .context("owning repository inventory")?
        .iter()
        .filter(|repo| {
            repo.get("github_slug").and_then(toml::Value::as_str)
                == Some(source.repository.as_str())
        })
        .collect();
    ensure!(
        matching.len() == 1,
        "mirror is not uniquely enrolled by owning manifest"
    );
    let branch = matching[0]
        .get("default_branch")
        .and_then(toml::Value::as_str)
        .context("maintained mirror branch missing")?;
    ensure!(
        !branch.is_empty()
            && branch.len() <= 200
            && branch
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
            && !branch.split('/').any(|part| part.is_empty()
                || part.starts_with('.')
                || part.ends_with('.')
                || part.ends_with(".lock"))
            && !branch.contains(".."),
        "invalid maintained mirror branch"
    );
    Ok(Some(format!("refs/heads/{branch}")))
}

fn observed_commit(output: &str, reference: &str) -> Result<String> {
    let lines: Vec<_> = output.lines().collect();
    ensure!(
        lines.len() == 1,
        "public reference must resolve to exactly one object"
    );
    let (commit, actual_ref) = lines[0]
        .split_once('\t')
        .context("malformed public reference response")?;
    ensure!(
        actual_ref == reference,
        "public reference response names a different ref"
    );
    oid(commit)?;
    Ok(commit.to_owned())
}

/// Every process gets a closed environment and fixed system tools. The source
/// config is generated by clone; no candidate hooks, filters or submodules run.
fn command(cwd: &Path, seconds: u64) -> Command {
    let mut command = Command::new("/usr/bin/timeout");
    command
        .current_dir(cwd)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        // Git ignores a ceiling equal to cwd; its parent is the boundary
        // setup stops at before inspecting any ancestor repository config.
        .env("GIT_CEILING_DIRECTORIES", cwd.parent().unwrap_or(cwd))
        .args(["--signal=TERM", "--kill-after=10s"])
        .arg(seconds.to_string())
        .arg("/usr/bin/prlimit")
        .arg(format!("--fsize={MAX_FILE_BYTES}:{MAX_FILE_BYTES}"))
        .args([
            "--",
            "/usr/bin/git",
            "-c",
            "protocol.allow=never",
            "-c",
            "protocol.https.allow=always",
            "-c",
            "credential.helper=",
            "-c",
            "core.askPass=/bin/false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.logAllRefUpdates=false",
            "-c",
            "http.followRedirects=false",
            "-c",
            "submodule.recurse=false",
            "-c",
            "gc.auto=0",
            "-c",
            "fetch.fsckObjects=true",
            "-c",
            "transfer.fsckObjects=true",
        ])
        .stdin(Stdio::null());
    command
}

struct Session {
    directory: PathBuf,
    deadline: Instant,
    step: usize,
}

impl Session {
    fn run(&mut self, cwd: &Path, args: &[&str]) -> Result<String> {
        ensure!(
            !cwd.as_os_str().as_encoded_bytes().contains(&b':'),
            "public cache path cannot contain Git ceiling separator"
        );
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(TimedOut.into());
        }
        let prefix = self.directory.join(format!("git-{:02}", self.step));
        self.step += 1;
        let stdout_path = prefix.with_extension("stdout");
        let stdout = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&stdout_path)?;
        let stderr = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(prefix.with_extension("stderr"))?;
        let status = command(cwd, remaining.as_secs().max(1))
            .args(args)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .status()?;
        if matches!(status.code(), Some(124 | 137)) {
            return Err(TimedOut.into());
        }
        ensure!(
            status.success(),
            "public source Git {} failed ({:?}); inspect retained private acquisition logs",
            args[0],
            status.code()
        );
        // This bounds the accepted cache; per-file and elapsed bounds also apply
        // during acquisition. It is not a filesystem quota while Git is running.
        custody::inspect(&self.directory, MAX_SOURCE_BYTES, MAX_SOURCE_ENTRIES)?;
        Ok(String::from_utf8(super::read_report(&stdout_path)?)?
            .trim_end()
            .to_owned())
    }
}

pub(super) fn acquire(
    root: &Path,
    out: &Path,
    source: &Source,
    index: usize,
    timeout_seconds: u64,
) -> Result<Acquired> {
    ensure!(
        source.path.is_none()
            && matches!(
                source.scope.as_str(),
                "standalone" | "dependency" | "optional"
            ),
        "public acquisition scope required"
    );
    ensure!(
        (1..=3600).contains(&timeout_seconds),
        "public acquisition timeout must be 1..3600 seconds"
    );
    let public_url = url(&source.repository)?;
    let observed_ref = maintained_ref(root, source)?;
    custody::owned_directory(out)?;
    let directory = out.join(format!("{index:04}.public-input"));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .context("use fresh public acquisition storage")?;
    let mut session = Session {
        directory: directory.clone(),
        deadline: Instant::now() + Duration::from_secs(timeout_seconds),
        step: 0,
    };
    let observed_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let commit = if let Some(reference) = &observed_ref {
        observed_commit(
            &session.run(
                &directory,
                &["ls-remote", "--exit-code", &public_url, reference],
            )?,
            reference,
        )?
    } else {
        source.commit.clone().context("immutable source commit")?
    };
    let scratch = directory.join("scratch");
    fs::DirBuilder::new().mode(0o700).create(&scratch)?;
    let identity = custody::owned_directory(&scratch)?;
    session.run(&scratch, &["init", "--bare", "--template=", "input.git"])?;
    let input = scratch.join("input.git");
    fs::set_permissions(&input, fs::Permissions::from_mode(0o700))?;
    let input_identity = custody::owned_directory(&input)?;
    session.run(
        &input,
        &[
            "fetch",
            "--no-tags",
            "--no-recurse-submodules",
            &public_url,
            &commit,
        ],
    )?;
    // This input repository was just created. No original or remote ref moves.
    session.run(
        &input,
        &["update-ref", "--no-deref", "HEAD", &commit, &"0".repeat(40)],
    )?;
    let (path, graph, source_identity) =
        clone_input(&mut session, &scratch, input_identity, &commit, &public_url)?;
    let tree = prove(&mut session, &path, &commit)?;
    ensure!(
        session.run(&path, &["config", "--get", "remote.origin.url"])? == public_url,
        "public source origin changed"
    );
    let receipt = json!({"schema":"jeryu.public-audit-source/v1", "repository":source.repository,
        "scope":source.scope, "url":public_url, "requested_commit":source.commit,
        "observed_ref":observed_ref, "observed_at":observed_at, "commit":commit, "tree":tree,
        "source_clean":true, "history_complete":true, "history_scope":"selected-commit-ancestors",
        "git_object_set_sha256":graph, "anonymous":true,
        "export_provenance_verified":false, "release_qualified":false});
    write_json(&directory.join("source.json"), &receipt)?;
    let receipt = json!({"path":format!("{index:04}.public-input/source.json"),
        "sha256":super::hash(&super::read_report(&directory.join("source.json"))?), "identity":receipt});
    Ok(Acquired {
        scratch,
        source: path,
        identity,
        input_identity,
        source_identity,
        commit,
        tree,
        graph,
        session,
        timeout_seconds,
        receipt,
    })
}

fn clone_input(
    session: &mut Session,
    scratch: &Path,
    input_identity: (u64, u64, u32),
    commit: &str,
    public_url: &str,
) -> Result<(PathBuf, String, (u64, u64, u32))> {
    let input = scratch.join("input.git");
    custody::unchanged_directory(&input, input_identity)?;
    let before = graph::verify(session, &input, commit, true)?;
    // File transport is allowed for this one internally constructed disposable
    // input only; every network acquisition command remains HTTPS-only.
    session.run(
        scratch,
        &[
            "-c",
            "protocol.file.allow=always",
            "clone",
            "--no-local",
            "--no-checkout",
            "--no-tags",
            "--template=",
            input.to_str().context("public cache path must be UTF-8")?,
            "source",
        ],
    )?;
    let path = scratch.join("source");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    let source_identity = custody::owned_directory(&path)?;
    session.run(&path, &["remote", "set-url", "origin", public_url])?;
    session.run(&path, &["checkout", "--detach", commit])?;
    custody::unchanged_directory(&input, input_identity)?;
    custody::unchanged_directory(&path, source_identity)?;
    ensure!(
        graph::verify(session, &input, commit, true)? == before
            && graph::verify(session, &path, commit, false)? == before,
        "public clone differs from its selected input graph"
    );
    Ok((path, before, source_identity))
}

fn prove(session: &mut Session, path: &Path, commit: &str) -> Result<String> {
    oid(commit)?;
    // The audit tree is narrower than its acquisition directory: links into
    // sibling acquisition logs are outside this source and must be refused.
    custody::inspect(path, MAX_SOURCE_BYTES, MAX_SOURCE_ENTRIES)?;
    ensure!(
        session.run(path, &["rev-parse", "--is-shallow-repository"])? == "false",
        "shallow public source is incomplete"
    );
    for forbidden in [
        ".git/shallow",
        ".git/info/grafts",
        ".git/objects/info/alternates",
    ] {
        ensure!(
            !path.join(forbidden).try_exists()?,
            "incomplete or redirected public object graph"
        );
    }
    ensure!(
        session
            .run(path, &["for-each-ref", "refs/replace/"])?
            .is_empty(),
        "replacement objects are not public source"
    );
    session.run(path, &["fsck", "--full", "--no-reflogs"])?;
    ensure!(
        !session
            .run(path, &["ls-files", "--stage"])?
            .lines()
            .any(|line| line.starts_with("160000 ")),
        "public source has unacquired submodules; declare their immutable audit scopes before execution"
    );
    ensure!(
        session
            .run(path, &["status", "--porcelain=v1", "--untracked-files=all"])?
            .is_empty(),
        "public source is not clean"
    );
    ensure!(
        session.run(path, &["rev-parse", "HEAD"])? == commit,
        "public source does not match the exact requested commit"
    );
    let tree = session.run(path, &["rev-parse", "HEAD^{tree}"])?;
    oid(&tree)?;
    Ok(tree)
}

#[cfg(test)]
#[path = "audit_acquisition_tests.rs"]
mod tests;
