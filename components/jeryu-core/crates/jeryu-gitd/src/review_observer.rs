//! Read-only Git observation for Core's authenticated review lifecycle.
//! Administrative installation still owns exclusive physical storage custody;
//! this adapter does not authorize standalone Git writers or perform merges.

use std::path::{Component, Path, PathBuf};

use jeryu_core::{
    ForgeError, MergeGitChange, MergeGitObservation, ReviewGitObservation, ReviewGitObserver,
    ReviewGitTarget,
};

use crate::{GitdError, RepoManager};

#[cfg(unix)]
mod native {
    use std::io::Read;
    use std::os::fd::OwnedFd;
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::net::UnixStream;
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    use jeryu_core::{ManagedGitIdentity, ObservedReviewRef, ReviewGitRepository};
    use rustix::process::{
        Pid, Signal, WaitId, WaitIdOptions, kill_process_group, test_kill_process_group, waitid,
    };
    use sha2::{Digest, Sha256};

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(super) struct FileIdentity {
        device: u64,
        inode: u64,
    }

    pub(super) fn checked(path: &Path, directory: bool) -> jeryu_core::Result<FileIdentity> {
        if !path.is_absolute() {
            return Err(unavailable("managed Git paths must be absolute"));
        }
        let mut prefix = PathBuf::new();
        for component in path.components() {
            if !matches!(component, Component::RootDir | Component::Normal(_)) {
                return Err(unavailable("managed Git paths must be normalized"));
            }
            prefix.push(component);
            let metadata = std::fs::symlink_metadata(&prefix).map_err(io_error)?;
            if metadata.file_type().is_symlink() {
                return Err(unavailable("managed Git paths cannot contain symlinks"));
            }
        }
        let metadata = std::fs::symlink_metadata(path).map_err(io_error)?;
        if (directory && !metadata.is_dir())
            || (!directory && !metadata.is_file())
            || metadata.mode() & 0o022 != 0
        {
            return Err(unavailable(
                "managed Git resources must have the expected type and deny group/other writes",
            ));
        }
        Ok(FileIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    pub(super) fn digest(path: &Path) -> jeryu_core::Result<String> {
        let mut file = std::fs::File::open(path).map_err(io_error)?;
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 16384];
        loop {
            let length = file.read(&mut buffer).map_err(io_error)?;
            if length == 0 {
                break;
            }
            digest.update(&buffer[..length]);
        }
        let digest = digest.finalize();
        Ok(digest
            .iter()
            .fold(String::with_capacity(64), |mut out, byte| {
                out.push_str(&format!("{byte:02x}"));
                out
            }))
    }

    fn io_error(error: std::io::Error) -> ForgeError {
        unavailable(&format!("Git observation I/O failed: {error}"))
    }

    fn unavailable(reason: &str) -> ForgeError {
        ForgeError::WriterUnavailable(reason.into())
    }

    struct Output {
        code: Option<i32>,
        stdout: String,
        stderr: String,
    }

    #[derive(Debug)]
    pub(super) struct UnresolvedProcess {
        pub(super) child: Child,
        pub(super) reason: String,
    }

    fn read_available(
        stream: &mut UnixStream,
        bytes: &mut Vec<u8>,
        eof: &mut bool,
    ) -> jeryu_core::Result<()> {
        let mut buffer = [0_u8; 4096];
        while !*eof {
            match stream.read(&mut buffer) {
                Ok(0) => *eof = true,
                Ok(count) => {
                    bytes.extend_from_slice(&buffer[..count]);
                    if bytes.len() > 65536 {
                        return Err(unavailable("Git observation output exceeded its limit"));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(io_error(error)),
            }
        }
        Ok(())
    }

    impl ManagedReviewGitObserver {
        fn run(&self, repository: &Path, args: &[&str]) -> jeryu_core::Result<Output> {
            if let Some(process) = self
                .unresolved
                .lock()
                .map_err(|_| unavailable("Git process custody lock failed"))?
                .first()
            {
                return Err(unavailable(&format!(
                    "Git process {} requires administrative reconciliation: {}",
                    process.child.id(),
                    process.reason
                )));
            }
            let (mut output_reader, output_writer) = UnixStream::pair().map_err(io_error)?;
            let (mut error_reader, error_writer) = UnixStream::pair().map_err(io_error)?;
            output_reader.set_nonblocking(true).map_err(io_error)?;
            error_reader.set_nonblocking(true).map_err(io_error)?;
            // No inherited HOME, SSH agent, Git routing/config, replacement
            // objects, promisor fetching, optional locks or interactive helpers.
            let mut child = Command::new(&self.git)
                .env_clear()
                .env("LC_ALL", "C")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_OPTIONAL_LOCKS", "0")
                .env("GIT_NO_REPLACE_OBJECTS", "1")
                .env("GIT_TERMINAL_PROMPT", "0")
                .arg("--no-replace-objects")
                .arg("--git-dir")
                .arg(repository)
                .args([
                    "-c",
                    "core.fsmonitor=false",
                    "-c",
                    "core.hooksPath=/dev/null",
                    "-c",
                    "protocol.allow=never",
                    "-c",
                    "core.commitGraph=false",
                ])
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::from(OwnedFd::from(output_writer)))
                .stderr(Stdio::from(OwnedFd::from(error_writer)))
                .process_group(0)
                .spawn()
                .map_err(io_error)?;
            let group = Pid::from_child(&child);
            let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
            let (mut stdout_eof, mut stderr_eof) = (false, false);
            let deadline = Instant::now() + Duration::from_secs(10);
            // WNOWAIT retains the waitable child and therefore its PID/PGID
            // identity. Never reap it while a later group signal is possible.
            let wait_options =
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT;
            let mut lost_wait_custody = None;
            let failure = loop {
                if let Err(error) = read_available(&mut output_reader, &mut stdout, &mut stdout_eof)
                    .and_then(|()| read_available(&mut error_reader, &mut stderr, &mut stderr_eof))
                {
                    break Some(error);
                }
                match waitid(WaitId::Pid(group), wait_options) {
                    Ok(Some(_)) if stdout_eof && stderr_eof => break None,
                    Ok(_) => {}
                    Err(rustix::io::Errno::INTR) => continue,
                    Err(error) => {
                        lost_wait_custody = Some(error.to_string());
                        break Some(unavailable(&format!("Git wait custody failed: {error}")));
                    }
                }
                if Instant::now() >= deadline {
                    break Some(unavailable("Git observation timed out"));
                }
                std::thread::sleep(Duration::from_millis(5));
            };
            // Reconfirm exclusive wait custody immediately before the sole
            // signal. A lost child (including ECHILD) never permits a numeric
            // process-group signal. The owning service must not install an
            // unrelated wait-any reaper over children owned by this adapter.
            let custody_error = lost_wait_custody.or_else(|| {
                waitid(WaitId::Pid(group), wait_options)
                    .err()
                    .map(|error| error.to_string())
            });
            if let Some(error) = custody_error {
                let reason = format!(
                    "Git cleanup lost wait custody: {error}; process_group={}; signal=not_attempted",
                    group.as_raw_pid()
                );
                self.unresolved
                    .lock()
                    .map_err(|_| unavailable("Git process custody lock failed"))?
                    .push(UnresolvedProcess {
                        child,
                        reason: reason.clone(),
                    });
                return Err(unavailable(&reason));
            }
            // Even an exited successful leader retains its PID until reap.
            // Drain any remaining owned group members before releasing it.
            let signal = kill_process_group(group, Signal::KILL);
            let cleanup_deadline = Instant::now() + Duration::from_secs(2);
            let mut status = None;
            let mut group_gone = false;
            let mut reap_error = None;
            while Instant::now() < cleanup_deadline {
                if status.is_none() {
                    match child.try_wait() {
                        Ok(Some(exit)) => status = Some(exit),
                        Ok(None) => {}
                        Err(error) => {
                            reap_error = Some(error.to_string());
                            break;
                        }
                    }
                }
                // This is read-only. After reap there are no signal calls,
                // including on failure or administrative fencing paths.
                group_gone = test_kill_process_group(group) == Err(rustix::io::Errno::SRCH);
                if status.is_some() && group_gone {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let reason = format!(
                "{}; process_group={}; signal_before_reap={signal:?}; reaped={}; group_gone={group_gone}; reap_error={reap_error:?}",
                failure
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "Git completed".into()),
                group.as_raw_pid(),
                status.is_some(),
            );
            if status.is_none()
                || !group_gone
                || !(signal.is_ok() || signal == Err(rustix::io::Errno::SRCH))
            {
                self.unresolved
                    .lock()
                    .map_err(|_| unavailable("Git process custody lock failed"))?
                    .push(UnresolvedProcess {
                        child,
                        reason: reason.clone(),
                    });
                return Err(unavailable(&reason));
            }
            if failure.is_some() {
                return Err(unavailable(&reason));
            }
            let status = status.expect("successful cleanup reaped the owned child");
            Ok(Output {
                code: status.code(),
                stdout: String::from_utf8(stdout)
                    .map_err(|_| unavailable("non-UTF8 Git output"))?,
                stderr: String::from_utf8(stderr).map_err(|_| unavailable("non-UTF8 Git error"))?,
            })
        }

        fn success(&self, repository: &Path, args: &[&str]) -> jeryu_core::Result<String> {
            let output = self.run(repository, args)?;
            if output.code != Some(0) {
                return Err(unavailable(&format!(
                    "Git observation failed ({:?}): {}",
                    output.code, output.stderr
                )));
            }
            Ok(output.stdout.trim_end_matches('\n').into())
        }

        fn observe_merge_source(
            &self,
            target: &ReviewGitTarget,
        ) -> jeryu_core::Result<MergeGitObservation> {
            if target.source.id.is_nil()
                || target.destination.id.is_nil()
                || (target.source.id == target.destination.id
                    && target.source != target.destination)
            {
                return Err(ForgeError::Validation(
                    "inconsistent merge repository UUID mapping".into(),
                ));
            }
            let before = self.observe(target)?;
            let (source, source_identity) = self.inspect_repository(&target.source)?;
            let (destination, destination_identity) =
                self.inspect_repository(&target.destination)?;
            if source_identity != before.source.identity
                || destination_identity != before.destination.identity
                || (source == destination && target.source.id != target.destination.id)
            {
                return Err(ForgeError::Conflict(
                    "merge repository custody differs from observed UUID scope".into(),
                ));
            }
            let head = &before.source.commit_sha;
            let base = &before.destination.commit_sha;
            self.require_complete_object_store(&source)?;
            self.require_complete_object_store(&destination)?;
            // Full fsck, with no alternate/promisor/config/network fallback,
            // verifies the actual source graph rather than cached PR metadata.
            self.success(
                &source,
                &[
                    "fsck",
                    "--strict",
                    "--no-reflogs",
                    "--no-dangling",
                    head,
                    base,
                ],
            )?;
            if self.success(&source, &["cat-file", "-t", base])? != "commit"
                || self.success(
                    &source,
                    &["rev-parse", "--verify", &format!("{base}^{{tree}}")],
                )? != before.destination.tree_sha
            {
                return Err(unavailable("source lacks the observed base commit/tree"));
            }
            let ancestry = self.run(&source, &["merge-base", "--is-ancestor", base, head])?;
            let base_is_ancestor = match ancestry.code {
                Some(0) if ancestry.stdout.is_empty() && ancestry.stderr.is_empty() => true,
                Some(1) if ancestry.stdout.is_empty() && ancestry.stderr.is_empty() => false,
                _ => return Err(unavailable("Git could not determine exact merge ancestry")),
            };
            let range = format!("{base}..{head}");
            let merge_commit = self.success(
                &source,
                &["rev-list", "--min-parents=2", "--max-count=1", &range],
            )?;
            if !merge_commit.is_empty() {
                oid(&merge_commit)?;
            }
            let raw = self.success(
                &source,
                &[
                    "diff-tree",
                    "--no-commit-id",
                    "--raw",
                    "-z",
                    "--no-abbrev",
                    "--no-renames",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--ignore-submodules=none",
                    "-r",
                    base,
                    head,
                ],
            )?;
            let changes = parse_merge_delta(&raw)?;
            // A fork's source objects must be admitted to the destination by a
            // separate guarded import before an executor can advance its ref.
            // Observation never fetches/copies objects or writes a quarantine.
            let receive = self.run(
                &destination,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{head}^{{commit}}"),
                ],
            )?;
            let destination_has_source_graph = match receive.code {
                Some(0)
                    if receive.stdout.trim_end_matches('\n') == head
                        && receive.stderr.is_empty() =>
                {
                    self.success(
                        &destination,
                        &["fsck", "--strict", "--no-reflogs", "--no-dangling", head],
                    )?;
                    true
                }
                Some(1) if receive.stdout.is_empty() && receive.stderr.is_empty() => false,
                _ => return Err(unavailable("Git receiving object inspection failed")),
            };
            let after = self.observe(target)?;
            if before != after {
                return Err(ForgeError::Conflict(
                    "source/base refs or custody changed during merge observation".into(),
                ));
            }
            Ok(MergeGitObservation {
                refs: before,
                object_format: "sha1".into(),
                base_is_ancestor,
                contains_merge_commits: !merge_commit.is_empty(),
                source_graph_verified: true,
                destination_has_source_graph,
                changes,
            })
        }

        fn require_complete_object_store(&self, repository: &Path) -> jeryu_core::Result<()> {
            let config = self.success(
                repository,
                &["config", "--local", "--no-includes", "--null", "--list"],
            )?;
            for entry in config.split('\0').filter(|entry| !entry.is_empty()) {
                let (key, _) = entry.split_once('\n').unwrap_or((entry, ""));
                if key.to_ascii_lowercase().starts_with("fsck.") {
                    return Err(unavailable(
                        "repository-local fsck policy or waiver input is not admitted",
                    ));
                }
            }
            match std::fs::symlink_metadata(repository.join("shallow")) {
                Ok(_) => {
                    return Err(unavailable(
                        "shallow merge source is not a complete admitted graph",
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error(error)),
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut directories = vec![repository.join("objects")];
            let mut count = 0_u64;
            while let Some(directory) = directories.pop() {
                checked(&directory, true)?;
                for entry in std::fs::read_dir(&directory).map_err(io_error)? {
                    count += 1;
                    if count > 250_000 || Instant::now() >= deadline {
                        return Err(unavailable(
                            "complete object-store custody inventory exceeded its admitted limit",
                        ));
                    }
                    let path = entry.map_err(io_error)?.path();
                    let metadata = std::fs::symlink_metadata(&path).map_err(io_error)?;
                    checked(&path, metadata.is_dir())?;
                    if path
                        .extension()
                        .is_some_and(|extension| extension == "promisor")
                    {
                        return Err(unavailable("promisor object custody is not admitted"));
                    }
                    if metadata.is_dir() {
                        directories.push(path);
                    } else if metadata.nlink() != 1 {
                        return Err(unavailable(
                            "external hardlink object custody is not admitted",
                        ));
                    }
                }
            }
            Ok(())
        }

        fn inspect_repository(
            &self,
            repository: &ReviewGitRepository,
        ) -> jeryu_core::Result<(PathBuf, ManagedGitIdentity)> {
            let id = crate::RepoId::new(&repository.owner, &repository.name)
                .map_err(|error| unavailable(&error.to_string()))?;
            if id.name != repository.name {
                return Err(unavailable("catalog repository name is not canonical"));
            }
            let path = self.root.join(&id.owner).join(id.bare_name());
            let root_id = checked(&self.root, true)?;
            if root_id != self.root_identity {
                return Err(unavailable("managed storage root identity changed"));
            }
            checked(&self.root.join(&id.owner), true)?;
            let repo_id = checked(&path, true)?;
            for directory in ["objects", "objects/info", "objects/pack", "refs"] {
                checked(&path.join(directory), true)?;
            }
            for optional in [
                "commondir",
                "objects/info/alternates",
                "objects/info/http-alternates",
                "info/grafts",
            ] {
                match std::fs::symlink_metadata(path.join(optional)) {
                    Ok(_) => {
                        return Err(unavailable(
                            "external object/common-directory/graft routing is not admitted",
                        ));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(io_error(error)),
                }
            }
            for required in ["HEAD", "config"] {
                checked(&path.join(required), false)?;
            }
            if std::fs::metadata(path.join("config"))
                .map_err(io_error)?
                .len()
                > 65536
            {
                return Err(unavailable(
                    "repository configuration exceeds observation limit",
                ));
            }
            for item in std::fs::read_dir(path.join("objects/pack")).map_err(io_error)? {
                checked(&item.map_err(io_error)?.path(), false)?;
            }
            match std::fs::symlink_metadata(path.join("packed-refs")) {
                Ok(_) => {
                    checked(&path.join("packed-refs"), false)?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error(error)),
            }
            let config = self.success(
                &path,
                &["config", "--local", "--no-includes", "--null", "--list"],
            )?;
            for entry in config.split('\0').filter(|entry| !entry.is_empty()) {
                let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
                let key = key.to_ascii_lowercase();
                if key.starts_with("include.")
                    || key.starts_with("includeif.")
                    || matches!(
                        key.as_str(),
                        "core.worktree"
                            | "core.alternaterefscommand"
                            | "extensions.partialclone"
                            | "extensions.worktreeconfig"
                            | "extensions.refstorage"
                    )
                    || key.ends_with(".promisor")
                    || (key == "extensions.objectformat" && value != "sha1")
                {
                    return Err(unavailable(
                        "repository configuration enables unsupported routing",
                    ));
                }
            }
            if self.success(&path, &["rev-parse", "--is-bare-repository"])? != "true"
                || self.success(&path, &["rev-parse", "--show-object-format"])? != "sha1"
            {
                return Err(unavailable(
                    "review observation requires a bare SHA-1 repository",
                ));
            }
            Ok((
                path.clone(),
                ManagedGitIdentity {
                    storage_root: self.root.to_string_lossy().into_owned(),
                    root_device: root_id.device,
                    root_inode: root_id.inode,
                    repository_path: path.to_string_lossy().into_owned(),
                    repository_device: repo_id.device,
                    repository_inode: repo_id.inode,
                    git_executable: self.git.to_string_lossy().into_owned(),
                    git_executable_sha256: self.git_digest.clone(),
                },
            ))
        }

        fn observe_ref(
            &self,
            path: &Path,
            identity: ManagedGitIdentity,
            reference: &str,
        ) -> jeryu_core::Result<ObservedReviewRef> {
            // Public trait targets may be constructed by trusted Rust callers;
            // still validate before using either a ref name or filesystem path.
            if !reference.starts_with("refs/heads/")
                || reference.contains("..")
                || reference.contains("@{")
                || reference
                    .split('/')
                    .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"))
                || reference.ends_with('.')
                || reference
                    .bytes()
                    .any(|b| b <= b' ' || b == 127 || b"~^:?*[\\".contains(&b))
            {
                return Err(ForgeError::Validation(
                    "unsupported review branch ref".into(),
                ));
            }
            // Every existing component of a loose ref must remain under the
            // admitted physical repository; packed refs are checked separately.
            let mut prefix = path.to_path_buf();
            for component in Path::new(reference).components() {
                prefix.push(component);
                match std::fs::symlink_metadata(&prefix) {
                    Ok(meta) => {
                        checked(&prefix, meta.is_dir())?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                    Err(error) => return Err(io_error(error)),
                }
            }
            let symbolic = self.run(path, &["symbolic-ref", "--quiet", reference])?;
            if symbolic.code == Some(0) {
                return Err(unavailable("symbolic review refs are not admitted"));
            }
            if symbolic.code != Some(1) || !symbolic.stderr.is_empty() {
                return Err(unavailable("could not classify direct review ref"));
            }
            let exists = self.run(path, &["show-ref", "--verify", "--quiet", reference])?;
            if exists.code == Some(1) && exists.stderr.is_empty() {
                return Err(ForgeError::NotFound(format!("Git ref {reference}")));
            }
            if exists.code != Some(0) {
                return Err(unavailable("Git failed while inspecting ref existence"));
            }
            let commit_sha = self.success(path, &["show-ref", "--verify", "--hash", reference])?;
            oid(&commit_sha)?;
            if self.success(path, &["cat-file", "-t", &commit_sha])? != "commit" {
                return Err(unavailable("review branch does not name a commit"));
            }
            let tree_sha = self.success(
                path,
                &["rev-parse", "--verify", &format!("{commit_sha}^{{tree}}")],
            )?;
            oid(&tree_sha)?;
            if self.success(path, &["cat-file", "-t", &tree_sha])? != "tree" {
                return Err(unavailable("commit tree is unavailable"));
            }
            for object in [&commit_sha, &tree_sha] {
                let loose = path.join("objects").join(&object[..2]).join(&object[2..]);
                match std::fs::symlink_metadata(&loose) {
                    Ok(_) => {
                        checked(&loose, false)?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(io_error(error)),
                }
            }
            Ok(ObservedReviewRef {
                reference: reference.into(),
                commit_sha,
                tree_sha,
                identity,
            })
        }
    }

    fn parse_merge_delta(raw: &str) -> jeryu_core::Result<Vec<MergeGitChange>> {
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        if !raw.ends_with('\0') {
            return Err(unavailable("unterminated Git tree delta"));
        }
        let fields = raw[..raw.len() - 1].split('\0').collect::<Vec<_>>();
        if fields.len() % 2 != 0 {
            return Err(unavailable("incomplete Git tree delta record"));
        }
        let mut result = Vec::new();
        let mut paths = std::collections::BTreeSet::new();
        for pair in fields.chunks_exact(2) {
            let path = pair[1];
            if path.is_empty()
                || path.starts_with('/')
                || path.split('/').any(|part| matches!(part, "" | "." | ".."))
                || !paths.insert(path.to_owned())
            {
                return Err(unavailable("invalid or duplicate actual Git delta path"));
            }
            let header = pair[0]
                .strip_prefix(':')
                .ok_or_else(|| unavailable("Git delta lacks raw header"))?;
            let columns = header.split(' ').collect::<Vec<_>>();
            if columns.len() != 5 || !matches!(columns[4], "A" | "D" | "M" | "T") {
                return Err(unavailable("unsupported Git delta shape or status"));
            }
            for mode in &columns[..2] {
                if !matches!(*mode, "000000" | "100644" | "100755" | "120000" | "160000") {
                    return Err(unavailable("unsupported Git delta file mode"));
                }
            }
            for (mode, object) in [(columns[0], columns[2]), (columns[1], columns[3])] {
                if mode == "000000" {
                    if object != "0000000000000000000000000000000000000000" {
                        return Err(unavailable("absent Git delta side has an object"));
                    }
                } else {
                    oid(object)?;
                }
            }
            if (columns[0] == "000000") != (columns[4] == "A")
                || (columns[1] == "000000") != (columns[4] == "D")
                || (columns[0] == columns[1] && columns[2] == columns[3])
            {
                return Err(unavailable(
                    "Git delta status contradicts its object/mode sides",
                ));
            }
            result.push(MergeGitChange {
                path: path.into(),
                status: columns[4].into(),
                old_mode: columns[0].into(),
                new_mode: columns[1].into(),
                old_oid: columns[2].into(),
                new_oid: columns[3].into(),
            });
        }
        result.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(result)
    }

    #[cfg(test)]
    mod merge_delta_tests {
        use super::*;
        const ZERO: &str = "0000000000000000000000000000000000000000";
        const BLOB: &str = "1111111111111111111111111111111111111111";

        #[test]
        fn raw_delta_preserves_nul_framed_newline_path_and_full_object_mode() {
            let raw = format!(":000000 100644 {ZERO} {BLOB} A\0line\nname\0");
            let delta = parse_merge_delta(&raw).unwrap();
            assert_eq!(delta.len(), 1);
            assert_eq!(delta[0].path, "line\nname");
            assert_eq!(delta[0].new_oid, BLOB);
            assert_eq!(delta[0].old_mode, "000000");
            assert!(parse_merge_delta("").unwrap().is_empty());
        }

        #[test]
        fn malformed_duplicate_and_contradictory_delta_records_refuse() {
            let valid = format!(":000000 100644 {ZERO} {BLOB} A\0file\0");
            for raw in [
                valid.trim_end_matches('\0').to_owned(),
                format!("{valid}{valid}"),
                valid.replace(" A\0", " R100\0"),
                valid.replace(" A\0", " M\0"),
                valid.replace("100644", "100666"),
                valid.replace("file", "../outside"),
                valid.replace(BLOB, "1111"),
                valid.replace(BLOB, ZERO),
                format!(":100644 100644 {BLOB} {BLOB} M\0file\0"),
                format!(":000000 100644 {BLOB} {BLOB} A\0file\0"),
            ] {
                assert!(parse_merge_delta(&raw).is_err(), "{raw:?}");
            }
        }
    }

    fn oid(value: &str) -> jeryu_core::Result<()> {
        if value.len() != 40
            || value
                .bytes()
                .any(|b| !matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            || value.bytes().all(|b| b == b'0')
        {
            return Err(unavailable("Git returned an invalid full SHA-1"));
        }
        Ok(())
    }

    impl ReviewGitObserver for ManagedReviewGitObserver {
        fn observe_merge(
            &self,
            target: &ReviewGitTarget,
        ) -> jeryu_core::Result<MergeGitObservation> {
            self.observe_merge_source(target)
        }

        fn storage_root(&self) -> &Path {
            &self.root
        }

        fn observe(&self, target: &ReviewGitTarget) -> jeryu_core::Result<ReviewGitObservation> {
            if checked(&self.git, false)? != self.git_identity
                || digest(&self.git)? != self.git_digest
            {
                return Err(unavailable("review Git executable identity changed"));
            }
            let observe = || -> jeryu_core::Result<ReviewGitObservation> {
                let (source, source_identity) = self.inspect_repository(&target.source)?;
                let (destination, destination_identity) =
                    self.inspect_repository(&target.destination)?;
                Ok(ReviewGitObservation {
                    source: self.observe_ref(&source, source_identity, &target.source_ref)?,
                    destination: self.observe_ref(
                        &destination,
                        destination_identity,
                        &target.destination_ref,
                    )?,
                })
            };
            let first = observe()?;
            // Detect concurrent unmanaged writes; complete transport serialization
            // remains a separate installation/execute-merge requirement.
            let second = observe()?;
            if first != second {
                return Err(ForgeError::Conflict(
                    "Git changed during review observation".into(),
                ));
            }
            if checked(&self.git, false)? != self.git_identity
                || digest(&self.git)? != self.git_digest
            {
                return Err(unavailable(
                    "review Git executable changed during observation",
                ));
            }
            if !self
                .unresolved
                .lock()
                .map_err(|_| unavailable("Git process custody lock failed"))?
                .is_empty()
            {
                return Err(unavailable(
                    "concurrent Git observation left unresolved process custody",
                ));
            }
            Ok(first)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        #[test]
        fn held_output_descriptor_is_nonblocking_and_large_output_is_rejected() {
            let (mut reader, mut writer) = UnixStream::pair().unwrap();
            reader.set_nonblocking(true).unwrap();
            let mut bytes = Vec::new();
            let mut eof = false;
            // An open but idle inherited writer cannot block observation.
            read_available(&mut reader, &mut bytes, &mut eof).unwrap();
            assert!(!eof);
            assert!(bytes.is_empty());
            writer.write_all(b"actual output").unwrap();
            read_available(&mut reader, &mut bytes, &mut eof).unwrap();
            assert_eq!(bytes, b"actual output");
            bytes.resize(65536, 0);
            writer.write_all(b"overflow").unwrap();
            assert!(matches!(
                read_available(&mut reader, &mut bytes, &mut eof),
                Err(ForgeError::WriterUnavailable(_))
            ));
        }

        #[test]
        fn hung_owned_git_process_is_bounded_killed_and_reaped() {
            let directory = tempfile::Builder::new()
                .permissions(std::fs::Permissions::from_mode(0o700))
                .tempdir()
                .unwrap();
            let executable = directory.path().join("hung-git");
            std::fs::write(&executable, "#!/bin/sh\nexec /bin/sleep 30\n").unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
            let mut config = crate::GitdConfig::new(directory.path());
            config.git_bin = executable.to_string_lossy().into();
            let observer = ManagedReviewGitObserver::new(RepoManager::new(config)).unwrap();
            let before = Instant::now();
            let error = match observer.run(directory.path(), &[]) {
                Ok(_) => panic!("hung process cannot succeed"),
                Err(error) => error,
            };
            assert!(
                before.elapsed() < Duration::from_secs(13),
                "bounded cleanup must not call blocking wait/join"
            );
            let reason = error.to_string();
            assert!(reason.contains("timed out"), "{reason}");
            assert!(reason.contains("reaped=true; group_gone=true"), "{reason}");
            assert!(observer.unresolved.lock().unwrap().is_empty());
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn exited_leader_with_retained_pipe_is_not_reaped_before_group_signal() {
            const CHILD: &str = "JERYU_REVIEW_RETAINED_PIPE_CHILD";
            if std::env::var_os(CHILD).is_none() {
                // Subreaper state is process-wide, so use one isolated test
                // process with a hard parent-owned deadline, never the shared
                // parallel harness. The child explicitly reaps its adoptee.
                let output = Command::new("/usr/bin/timeout")
                    .args(["--signal=TERM", "--kill-after=2s", "20s"])
                    .arg(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "review_observer::native::tests::exited_leader_with_retained_pipe_is_not_reaped_before_group_signal",
                        "--test-threads=1",
                        "--nocapture",
                    ])
                    .env(CHILD, "1")
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
                return;
            }
            use rustix::process::{WaitOptions, getpid, set_child_subreaper, wait};
            set_child_subreaper(Some(getpid())).unwrap();
            let directory = tempfile::Builder::new()
                .permissions(std::fs::Permissions::from_mode(0o700))
                .tempdir()
                .unwrap();
            let executable = directory.path().join("exited-git");
            std::fs::write(&executable, "#!/bin/sh\n/bin/sleep 30 &\nexit 0\n").unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
            let mut config = crate::GitdConfig::new(directory.path());
            config.git_bin = executable.to_string_lossy().into();
            let observer = ManagedReviewGitObserver::new(RepoManager::new(config)).unwrap();
            let before = Instant::now();
            let result = observer.run(directory.path(), &[]);
            // A killed orphan remains a waitable zombie under this dedicated
            // subreaper. Reap any of this isolated process's children: the
            // observer's separate group is outside waitpid(None)'s selection.
            // Do not signal after the observer releases the leader identity.
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut adopted = Vec::new();
            loop {
                match wait(WaitOptions::NOHANG) {
                    Ok(Some((pid, status))) => adopted.push((pid, status)),
                    Err(rustix::io::Errno::CHILD) => break,
                    Ok(None) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    other => panic!("owned test children did not drain: {other:?}"),
                }
            }
            assert!(before.elapsed() < Duration::from_secs(15));
            let error = match result {
                Err(error) => error.to_string(),
                Ok(_) => panic!("an incomplete observation cannot become success after cleanup"),
            };
            assert!(error.contains("timed out"), "{error}");
            assert!(error.contains("signal_before_reap=Ok(())"), "{error}");
            assert!(error.contains("reaped=true; group_gone=false"), "{error}");
            assert_eq!(adopted.len(), 1);
            assert_eq!(adopted[0].1.terminating_signal(), Some(9));
            assert_eq!(observer.unresolved.lock().unwrap().len(), 1);
            assert!(matches!(
                observer.run(directory.path(), &[]),
                Err(ForgeError::WriterUnavailable(_))
            ));
        }
    }
}

/// Shared-runtime observer configured by the owning service, never by jobs or
/// HTTP request fields. Requires an absolute approved Git executable.
#[derive(Debug)]
pub struct ManagedReviewGitObserver {
    root: PathBuf,
    #[cfg(unix)]
    git: PathBuf,
    #[cfg(unix)]
    root_identity: native::FileIdentity,
    #[cfg(unix)]
    git_identity: native::FileIdentity,
    #[cfg(unix)]
    git_digest: String,
    #[cfg(unix)]
    unresolved: std::sync::Mutex<Vec<native::UnresolvedProcess>>,
}

impl ManagedReviewGitObserver {
    pub fn new(manager: RepoManager) -> crate::Result<Self> {
        #[cfg(unix)]
        {
            let root = manager.config().storage_root.clone();
            let git = PathBuf::from(&manager.config().git_bin);
            let map = |error: ForgeError| GitdError::InvalidInput(error.to_string());
            let root_identity = native::checked(&root, true).map_err(map)?;
            let git_identity = native::checked(&git, false).map_err(map)?;
            let git_digest = native::digest(&git).map_err(map)?;
            Ok(Self {
                root,
                git,
                root_identity,
                git_identity,
                git_digest,
                unresolved: std::sync::Mutex::new(Vec::new()),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = manager;
            Err(GitdError::InvalidInput(
                "managed review observation requires Unix physical custody".into(),
            ))
        }
    }
}

#[cfg(not(unix))]
impl ReviewGitObserver for ManagedReviewGitObserver {
    fn storage_root(&self) -> &Path {
        &self.root
    }
    fn observe(&self, _: &ReviewGitTarget) -> jeryu_core::Result<ReviewGitObservation> {
        Err(ForgeError::WriterUnavailable(
            "managed review observation requires Unix physical custody".into(),
        ))
    }
}
