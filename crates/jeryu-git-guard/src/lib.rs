#![doc = "Deny-by-default git command allowlist for confined coding agents."]
#![doc = ""]
#![doc = "A sandboxed agent works ONLY on the single branch jeryu assigned it. It may"]
#![doc = "inspect and edit that branch locally (status/diff/log/add/commit/revert/…),"]
#![doc = "but it must NOT reach the network (push/fetch/clone), spawn worktrees, create"]
#![doc = "or switch branches, rewrite history through integration commands, or inject"]
#![doc = "config/aliases that would escape the guard. Publishing is mediated by jeryu,"]
#![doc = "never by the agent calling `git push`."]
#![doc = ""]
#![doc = "This crate is split like [`jeryu_egress`]:"]
#![doc = "* [`git_command_decision`] is a PURE, side-effect-free verdict function — the"]
#![doc = "  single source of truth, exhaustively unit tested with no real git at all."]
#![doc = "* `bin/jeryu-git.rs` is the thin wrapper: it consults the verdict, then either"]
#![doc = "  `exec`s the real git or prints typed repair guidance and exits non-zero."]
#![doc = ""]
#![doc = "The wrapper is installed as the ONLY `git` on the agent's `PATH`, so every"]
#![doc = "invocation passes through this verdict. Network denial here is belt-and-"]
#![doc = "suspenders: the cell is also `--network none` with a socket-blocking seccomp"]
#![doc = "profile, so even a bypass cannot egress."]

/// Process exit code the wrapper returns when a command is refused.
pub const DENY_EXIT_CODE: i32 = 13;

/// The verdict for a single `git …` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitDecision {
    /// The command is branch-local and safe; the wrapper may exec real git.
    Allow,
    /// The command is refused; the wrapper prints the error and exits non-zero.
    Deny(GitGuardError),
}

impl GitDecision {
    /// Whether this verdict permits running real git.
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, GitDecision::Allow)
    }
}

/// Structured repair guidance for a refused git command. Mirrors the typed-repair
/// shape used across jeryu (`WorkcellError`): a stable machine `reason`, the human
/// `purpose`, concrete `common_fixes`, a `docs_url`, and a one-line `repair_hint`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitGuardError {
    /// What the agent was apparently trying to do.
    pub purpose: &'static str,
    /// Stable machine-readable reason code (e.g. `git_network_denied`).
    pub reason: &'static str,
    /// Concrete things the agent can do instead.
    pub common_fixes: &'static [&'static str],
    /// Where to read more.
    pub docs_url: &'static str,
    /// One-line steer.
    pub repair_hint: &'static str,
    /// The exact refused command, for the log line.
    command: String,
}

impl GitGuardError {
    fn new(
        purpose: &'static str,
        reason: &'static str,
        common_fixes: &'static [&'static str],
        repair_hint: &'static str,
        command: String,
    ) -> Self {
        Self {
            purpose,
            reason,
            common_fixes,
            docs_url: "docs/errors.md#git-guard",
            repair_hint,
            command,
        }
    }

    /// The refused command string (`git push origin HEAD`).
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Render a human + agent readable refusal block.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!(
            "jeryu-git: refused `{}` — {}\n  why: {}\n  fixes:\n",
            self.command, self.reason, self.purpose
        );
        for fix in self.common_fixes {
            out.push_str("    - ");
            out.push_str(fix);
            out.push('\n');
        }
        out.push_str("  hint: ");
        out.push_str(self.repair_hint);
        out.push_str("\n  docs: ");
        out.push_str(self.docs_url);
        out
    }
}

// --- reason-specific constructors ------------------------------------------------

const NETWORK_FIXES: &[&str] = &[
    "commit locally on your branch; jeryu publishes it on export/review",
    "you cannot push/fetch/clone — the cell has no network and jeryu mediates git",
];
const WORKTREE_FIXES: &[&str] = &[
    "you get exactly one branch in one runner",
    "need a second line of work? ask jeryu to start another runner — do not add worktrees",
];
const INTEGRATION_FIXES: &[&str] = &[
    "do not merge/rebase/cherry-pick — jeryu rebases your branch on main and integrates",
    "just commit your work; jeryu handles integration and conflict resolution",
];
const BRANCH_FIXES: &[&str] = &[
    "you are pinned to your assigned branch (JERYU_BRANCH)",
    "do not create/delete/rename/switch branches — request another runner for parallel work",
];
const CONFIG_WRITE_FIXES: &[&str] = &[
    "config is read-only inside the cell (identity/options are preset by jeryu)",
    "aliases and credential/remote config cannot be written from inside the cell",
];
const GLOBAL_FLAG_FIXES: &[&str] = &[
    "drop the -c/--config-env/--git-dir/--work-tree/--exec-path option",
    "run plain git subcommands inside your workspace; jeryu controls the repo layout",
];
const SUBCOMMAND_FIXES: &[&str] = &[
    "only branch-local commands are allowed (status/diff/log/show/add/commit/revert/reset/restore/stash)",
    "if you reached here via a git alias, it is refused: aliases can run arbitrary commands",
];

impl GitGuardError {
    fn network(cmd: String) -> Self {
        Self::new(
            "reach the network or another repo via git",
            "git_network_denied",
            NETWORK_FIXES,
            "commit locally; jeryu publishes and integrates your branch",
            cmd,
        )
    }
    fn worktree(cmd: String) -> Self {
        Self::new(
            "create an extra worktree / submodule checkout",
            "git_worktree_denied",
            WORKTREE_FIXES,
            "more worktrees → fire up more runners (one branch per runner)",
            cmd,
        )
    }
    fn integration(cmd: String) -> Self {
        Self::new(
            "rewrite history or integrate other refs",
            "git_integration_denied",
            INTEGRATION_FIXES,
            "just commit; jeryu rebases on main and integrates",
            cmd,
        )
    }
    fn branch(cmd: String) -> Self {
        Self::new(
            "create, delete, rename, or switch branches",
            "git_branch_denied",
            BRANCH_FIXES,
            "you are confined to one branch; request another runner for more",
            cmd,
        )
    }
    fn config_write(cmd: String) -> Self {
        Self::new(
            "write git config (remote/credential/alias/identity)",
            "git_config_write_denied",
            CONFIG_WRITE_FIXES,
            "git config is read-only in the cell",
            cmd,
        )
    }
    fn global_flag(cmd: String) -> Self {
        Self::new(
            "use a config-injecting or repo-redirecting global flag",
            "git_global_flag_denied",
            GLOBAL_FLAG_FIXES,
            "remove the -c/--git-dir/--work-tree/--exec-path/--config-env option",
            cmd,
        )
    }
    fn subcommand(cmd: String) -> Self {
        Self::new(
            "run a git subcommand outside the branch-local allowlist",
            "git_subcommand_denied",
            SUBCOMMAND_FIXES,
            "use a branch-local command, or ask jeryu for the operation you need",
            cmd,
        )
    }
}

// --- command classification sets -------------------------------------------------

/// Subcommands that reach the network or another repository.
const NETWORK_DENY: &[&str] = &[
    "push",
    "fetch",
    "pull",
    "clone",
    "remote",
    "ls-remote",
    "fetch-pack",
    "send-pack",
    "receive-pack",
    "upload-pack",
    "upload-archive",
    "daemon",
    "credential",
    "credential-cache",
    "credential-store",
    "credential-helper",
    "request-pull",
    "format-patch",
    "send-email",
    "imap-send",
    "p4",
    "svn",
    "archive",
    "http-fetch",
    "http-push",
    "remote-ext",
    "remote-fd",
    "remote-http",
    "remote-https",
    "remote-ftp",
    "remote-ftps",
];

/// Subcommands that spawn additional checkouts.
const WORKTREE_DENY: &[&str] = &["worktree", "submodule"];

/// Subcommands that rewrite history or integrate other refs — jeryu mediates these.
const INTEGRATION_DENY: &[&str] = &[
    "merge",
    "rebase",
    "cherry-pick",
    "am",
    "apply",
    "revert-sequencer",
    "gc",
    "fsck",
    "filter-branch",
    "filter-repo",
    "replace",
    "fast-import",
    "fast-export",
    "bundle",
    "prune",
    "prune-packed",
    "repack",
    "pack-objects",
    "pack-refs",
    "update-ref",
    "symbolic-ref",
    "update-index",
    "write-tree",
    "commit-tree",
    "merge-tree",
    "read-tree",
    "reset-sequencer",
];

/// Branch-local subcommands that are always safe (read or local edit only).
const ALLOWED_LOCAL: &[&str] = &[
    "status",
    "diff",
    "show",
    "log",
    "add",
    "commit",
    "revert",
    "restore",
    "rm",
    "mv",
    "reset",
    "stash",
    "blame",
    "grep",
    "shortlog",
    "describe",
    "rev-parse",
    "rev-list",
    "cat-file",
    "ls-files",
    "ls-tree",
    "show-ref",
    "for-each-ref",
    "merge-base",
    "name-rev",
    "diff-tree",
    "diff-index",
    "whatchanged",
    "count-objects",
    "verify-commit",
    "cherry",
    "range-diff",
    "check-ignore",
    "check-attr",
    "var",
    "hash-object",
    "annotate",
    "show-branch",
    "diff-files",
    "verify-tag",
];

/// The PURE verdict function — the single source of truth.
///
/// `argv` is the arguments passed to `git` (NOT including the `git` program name),
/// e.g. `["commit", "-m", "msg"]`. `assigned_branch` is the agent's pinned branch
/// (from `JERYU_BRANCH`); switching/creating any other branch is refused.
///
/// Deny-by-default: only an explicit allowlist passes. Unknown subcommands — which
/// includes user-defined aliases that can run arbitrary shell — are refused.
#[must_use]
pub fn git_command_decision(argv: &[String], assigned_branch: &str) -> GitDecision {
    let rendered = render_command(argv);

    // 1. Scan leading global options before the subcommand.
    let mut idx = 0;
    while idx < argv.len() {
        let tok = argv[idx].as_str();
        if !tok.starts_with('-') {
            break; // first non-flag token is the subcommand
        }
        if is_denied_global(tok) {
            return GitDecision::Deny(GitGuardError::global_flag(rendered));
        }
        if global_takes_value(tok) {
            idx += 2; // skip the flag and its value (e.g. `-C <path>`)
            continue;
        }
        if is_benign_global(tok) {
            idx += 1;
            continue;
        }
        // Unknown leading global flag → refuse (could be a config-injection vector).
        return GitDecision::Deny(GitGuardError::global_flag(rendered));
    }

    // 2. No subcommand (bare `git`, `git --version`, `git --help`) is harmless.
    let Some(sub) = argv.get(idx) else {
        return GitDecision::Allow;
    };
    let sub = sub.as_str();
    let rest = &argv[idx + 1..];

    // 3. Classify the subcommand (deny-by-default).
    if NETWORK_DENY.contains(&sub) {
        return GitDecision::Deny(GitGuardError::network(rendered));
    }
    if WORKTREE_DENY.contains(&sub) {
        return GitDecision::Deny(GitGuardError::worktree(rendered));
    }
    if INTEGRATION_DENY.contains(&sub) {
        return GitDecision::Deny(GitGuardError::integration(rendered));
    }
    match sub {
        "config" => config_decision(rest, rendered),
        "branch" => branch_decision(rest, rendered),
        "checkout" => checkout_decision(rest, assigned_branch, rendered),
        "switch" => switch_decision(rest, assigned_branch, rendered),
        "tag" => tag_decision(rest, rendered),
        "reflog" => reflog_decision(rest, rendered),
        _ if ALLOWED_LOCAL.contains(&sub) => GitDecision::Allow,
        _ => GitDecision::Deny(GitGuardError::subcommand(rendered)),
    }
}

fn render_command(argv: &[String]) -> String {
    let mut s = String::from("git");
    for a in argv {
        s.push(' ');
        s.push_str(a);
    }
    s
}

/// Globals that inject config or redirect the repo/worktree — always refused.
fn is_denied_global(tok: &str) -> bool {
    const EXACT: &[&str] = &[
        "-c",
        "--config-env",
        "--namespace",
        "--bare",
        "--super-prefix",
    ];
    const PREFIX: &[&str] = &[
        "--config-env=",
        "--git-dir",
        "--work-tree",
        "--exec-path=",
        "--namespace=",
        "--super-prefix=",
    ];
    if EXACT.contains(&tok) {
        return true;
    }
    // `--exec-path` with no `=` prints the path (benign); `--exec-path=X` redirects.
    if tok == "--git-dir" || tok == "--work-tree" {
        return true;
    }
    PREFIX.iter().any(|p| tok.starts_with(p))
}

/// Benign globals that consume the following token as their value.
fn global_takes_value(tok: &str) -> bool {
    tok == "-C"
}

/// Benign no-value globals.
fn is_benign_global(tok: &str) -> bool {
    const OK: &[&str] = &[
        "--no-pager",
        "--paginate",
        "-p",
        "--no-replace-objects",
        "--literal-pathspecs",
        "--no-literal-pathspecs",
        "--glob-pathspecs",
        "--noglob-pathspecs",
        "--icase-pathspecs",
        "--no-optional-locks",
        "--html-path",
        "--man-path",
        "--info-path",
        "--exec-path",
        "--version",
        "--help",
        "-P",
    ];
    OK.contains(&tok)
}

fn config_decision(args: &[String], cmd: String) -> GitDecision {
    const WRITE_FLAGS: &[&str] = &[
        "--add",
        "--unset",
        "--unset-all",
        "--replace-all",
        "--edit",
        "-e",
        "--rename-section",
        "--remove-section",
    ];
    let mut positionals = 0;
    for a in args {
        if WRITE_FLAGS.contains(&a.as_str()) {
            return GitDecision::Deny(GitGuardError::config_write(cmd));
        }
        if !a.starts_with('-') {
            positionals += 1;
        }
    }
    // `git config <name> <value>` is a write; a lone `<name>` is a read.
    if positionals >= 2 {
        return GitDecision::Deny(GitGuardError::config_write(cmd));
    }
    GitDecision::Allow
}

fn branch_decision(args: &[String], cmd: String) -> GitDecision {
    const MUTATION: &[&str] = &[
        "-d",
        "-D",
        "--delete",
        "-m",
        "-M",
        "--move",
        "-c",
        "-C",
        "--copy",
        "-f",
        "--force",
        "--set-upstream-to",
        "-u",
        "--unset-upstream",
        "--edit-description",
    ];
    // Flags that consume the next token (so its value isn't read as a new branch name).
    const VALUE_FLAGS: &[&str] = &[
        "--contains",
        "--no-contains",
        "--merged",
        "--no-merged",
        "--points-at",
        "--format",
        "--sort",
        "--color",
        "--abbrev",
    ];
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if MUTATION.contains(&a) {
            return GitDecision::Deny(GitGuardError::branch(cmd));
        }
        if VALUE_FLAGS.contains(&a) {
            i += 2;
            continue;
        }
        if a.starts_with('-') {
            // any other listing flag (-a, -r, -v, --list, --show-current, …)
            i += 1;
            continue;
        }
        // A bare positional names a branch to create/rename → refuse.
        return GitDecision::Deny(GitGuardError::branch(cmd));
    }
    GitDecision::Allow
}

fn checkout_decision(args: &[String], assigned: &str, cmd: String) -> GitDecision {
    const CREATE: &[&str] = &["-b", "-B", "--orphan", "--detach"];
    let mut positionals: Vec<&str> = Vec::new();
    let mut saw_dashdash = false;
    for a in args {
        if a == "--" {
            saw_dashdash = true;
            break;
        }
        if CREATE.contains(&a.as_str()) {
            return GitDecision::Deny(GitGuardError::branch(cmd));
        }
        // A bare `-` means "the previous branch" — a switch target, not a flag.
        if !a.starts_with('-') || a == "-" {
            positionals.push(a.as_str());
        }
    }
    // `git checkout -- <paths>` (explicit file restore) is safe.
    if saw_dashdash {
        return GitDecision::Allow;
    }
    // Allow only: bare checkout, a pathspec `.`, or re-checkout of the assigned branch.
    let safe = positionals.is_empty() || positionals.iter().all(|p| *p == "." || *p == assigned);
    if safe {
        GitDecision::Allow
    } else {
        GitDecision::Deny(GitGuardError::branch(cmd))
    }
}

fn switch_decision(args: &[String], assigned: &str, cmd: String) -> GitDecision {
    const CREATE: &[&str] = &["-c", "-C", "--create", "--orphan", "--detach"];
    let mut positionals: Vec<&str> = Vec::new();
    for a in args {
        if CREATE.contains(&a.as_str()) {
            return GitDecision::Deny(GitGuardError::branch(cmd));
        }
        // A bare `-` means "the previous branch" — a switch target, not a flag.
        if !a.starts_with('-') || a == "-" {
            positionals.push(a.as_str());
        }
    }
    if positionals.is_empty() || positionals.iter().all(|p| *p == assigned) {
        GitDecision::Allow
    } else {
        GitDecision::Deny(GitGuardError::branch(cmd))
    }
}

fn tag_decision(args: &[String], cmd: String) -> GitDecision {
    const CREATE: &[&str] = &[
        "-a",
        "--annotate",
        "-s",
        "--sign",
        "-d",
        "--delete",
        "-m",
        "--message",
        "-f",
        "--force",
        "-u",
        "--local-user",
        "-F",
        "--file",
        "--create-reflog",
    ];
    const LIST_FLAGS: &[&str] = &[
        "-l",
        "--list",
        "-n",
        "--contains",
        "--no-contains",
        "--points-at",
        "--merged",
        "--no-merged",
        "--sort",
        "--format",
        "--color",
    ];
    let mut has_list_flag = false;
    let mut has_positional = false;
    for a in args {
        if CREATE.contains(&a.as_str()) {
            return GitDecision::Deny(GitGuardError::branch(cmd));
        }
        if LIST_FLAGS.contains(&a.as_str()) {
            has_list_flag = true;
        } else if !a.starts_with('-') {
            has_positional = true;
        }
    }
    // A bare positional with no list flag is `git tag <name>` (create).
    if has_positional && !has_list_flag {
        return GitDecision::Deny(GitGuardError::branch(cmd));
    }
    GitDecision::Allow
}

fn reflog_decision(args: &[String], cmd: String) -> GitDecision {
    match args.first().map(String::as_str) {
        Some("expire") | Some("delete") => GitDecision::Deny(GitGuardError::integration(cmd)),
        _ => GitDecision::Allow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_string()).collect()
    }

    fn decide(parts: &[&str]) -> GitDecision {
        git_command_decision(&argv(parts), "agents/a/wc/feature")
    }

    fn reason(d: &GitDecision) -> &str {
        match d {
            GitDecision::Deny(e) => e.reason,
            GitDecision::Allow => "ALLOW",
        }
    }

    #[test]
    fn local_edit_and_read_allowed() {
        for cmd in [
            vec!["status"],
            vec!["status", "--short"],
            vec!["diff"],
            vec!["diff", "--cached"],
            vec!["log", "--oneline", "-10"],
            vec!["show", "HEAD"],
            vec!["add", "-A"],
            vec!["add", "src/lib.rs"],
            vec!["commit", "-m", "work"],
            vec!["commit", "--amend", "--no-edit"],
            vec!["revert", "HEAD"],
            vec!["reset", "--hard", "HEAD~1"],
            vec!["restore", "src/lib.rs"],
            vec!["restore", "--staged", "src/lib.rs"],
            vec!["rm", "old.rs"],
            vec!["mv", "a.rs", "b.rs"],
            vec!["stash"],
            vec!["stash", "pop"],
            vec!["rev-parse", "HEAD"],
            vec!["blame", "src/lib.rs"],
            vec!["grep", "needle"],
        ] {
            assert!(decide(&cmd).is_allowed(), "expected allow: git {cmd:?}");
        }
    }

    #[test]
    fn network_commands_denied() {
        for cmd in [
            vec!["push"],
            vec!["push", "origin", "HEAD"],
            vec!["push", "--force"],
            vec!["fetch"],
            vec!["fetch", "origin"],
            vec!["pull"],
            vec!["clone", "https://example.com/x.git"],
            vec!["remote", "add", "evil", "https://x"],
            vec!["remote", "-v"],
            vec!["ls-remote"],
            vec!["send-email", "x.patch"],
            vec!["format-patch", "-1"],
        ] {
            let d = decide(&cmd);
            assert_eq!(reason(&d), "git_network_denied", "git {cmd:?}");
        }
    }

    #[test]
    fn worktree_and_submodule_denied_with_runner_steer() {
        let d = decide(&["worktree", "add", "../wt"]);
        assert_eq!(reason(&d), "git_worktree_denied");
        if let GitDecision::Deny(e) = d {
            assert!(e.repair_hint.contains("more runners"));
        }
        assert_eq!(
            reason(&decide(&["submodule", "add", "x"])),
            "git_worktree_denied"
        );
    }

    #[test]
    fn integration_commands_denied() {
        for cmd in [
            vec!["merge", "other"],
            vec!["rebase", "main"],
            vec!["cherry-pick", "abc123"],
            vec!["am", "x.patch"],
            vec!["apply", "x.patch"],
            vec!["gc"],
            vec!["update-ref", "refs/heads/x", "HEAD"],
            vec!["filter-branch"],
        ] {
            assert_eq!(
                reason(&decide(&cmd)),
                "git_integration_denied",
                "git {cmd:?}"
            );
        }
    }

    #[test]
    fn branch_listing_allowed_creation_denied() {
        // listing forms
        for cmd in [
            vec!["branch"],
            vec!["branch", "-a"],
            vec!["branch", "-v"],
            vec!["branch", "--list"],
            vec!["branch", "--show-current"],
            vec!["branch", "--contains", "HEAD"],
            vec!["branch", "--merged", "main"],
        ] {
            assert!(decide(&cmd).is_allowed(), "expected allow: git {cmd:?}");
        }
        // mutation forms
        for cmd in [
            vec!["branch", "newbranch"],
            vec!["branch", "newbranch", "main"],
            vec!["branch", "-d", "old"],
            vec!["branch", "-D", "old"],
            vec!["branch", "-m", "old", "new"],
            vec!["branch", "-c", "old", "copy"],
            vec!["branch", "-u", "origin/x"],
            vec!["branch", "--set-upstream-to", "origin/x"],
        ] {
            assert_eq!(reason(&decide(&cmd)), "git_branch_denied", "git {cmd:?}");
        }
    }

    #[test]
    fn checkout_pathspec_and_own_branch_allowed_other_ref_denied() {
        assert!(decide(&["checkout", "--", "src/lib.rs"]).is_allowed());
        assert!(decide(&["checkout", "."]).is_allowed());
        assert!(decide(&["checkout", "agents/a/wc/feature"]).is_allowed());
        assert_eq!(
            reason(&decide(&["checkout", "-b", "newbranch"])),
            "git_branch_denied"
        );
        assert_eq!(reason(&decide(&["checkout", "main"])), "git_branch_denied");
        assert_eq!(
            reason(&decide(&["checkout", "--detach", "HEAD"])),
            "git_branch_denied"
        );
    }

    #[test]
    fn switch_own_branch_allowed_create_and_other_denied() {
        assert!(decide(&["switch", "agents/a/wc/feature"]).is_allowed());
        assert_eq!(
            reason(&decide(&["switch", "-c", "new"])),
            "git_branch_denied"
        );
        assert_eq!(reason(&decide(&["switch", "main"])), "git_branch_denied");
        assert_eq!(reason(&decide(&["switch", "-"])), "git_branch_denied");
    }

    #[test]
    fn config_reads_allowed_writes_denied() {
        assert!(decide(&["config", "user.name"]).is_allowed());
        assert!(decide(&["config", "--get", "user.email"]).is_allowed());
        assert!(decide(&["config", "--list"]).is_allowed());
        assert!(decide(&["config", "--get-regexp", "^user"]).is_allowed());
        assert_eq!(
            reason(&decide(&["config", "user.email", "me@x"])),
            "git_config_write_denied"
        );
        assert_eq!(
            reason(&decide(&["config", "--global", "user.name", "X"])),
            "git_config_write_denied"
        );
        assert_eq!(
            reason(&decide(&["config", "alias.x", "!sh"])),
            "git_config_write_denied"
        );
        assert_eq!(
            reason(&decide(&["config", "--unset", "core.x"])),
            "git_config_write_denied"
        );
    }

    #[test]
    fn tag_listing_allowed_creation_denied() {
        assert!(decide(&["tag"]).is_allowed());
        assert!(decide(&["tag", "-l"]).is_allowed());
        assert!(decide(&["tag", "-l", "v*"]).is_allowed());
        assert_eq!(reason(&decide(&["tag", "v1.0"])), "git_branch_denied");
        assert_eq!(
            reason(&decide(&["tag", "-a", "v1", "-m", "x"])),
            "git_branch_denied"
        );
        assert_eq!(reason(&decide(&["tag", "-d", "v1"])), "git_branch_denied");
    }

    #[test]
    fn config_injection_globals_denied() {
        // The classic alias-RCE escape: `git -c alias.x='!sh' x`.
        assert_eq!(
            reason(&decide(&["-c", "alias.x=!sh", "status"])),
            "git_global_flag_denied"
        );
        assert_eq!(
            reason(&decide(&["-c", "core.fsmonitor=evil", "status"])),
            "git_global_flag_denied"
        );
        assert_eq!(
            reason(&decide(&["--git-dir", "/other/.git", "log"])),
            "git_global_flag_denied"
        );
        assert_eq!(
            reason(&decide(&["--git-dir=/other/.git", "log"])),
            "git_global_flag_denied"
        );
        assert_eq!(
            reason(&decide(&["--work-tree", "/other", "status"])),
            "git_global_flag_denied"
        );
        assert_eq!(
            reason(&decide(&["--exec-path=/evil", "status"])),
            "git_global_flag_denied"
        );
    }

    #[test]
    fn benign_globals_pass_through_to_subcommand() {
        assert!(decide(&["--no-pager", "log"]).is_allowed());
        assert!(decide(&["-C", "subdir", "status"]).is_allowed());
        assert!(decide(&["--paginate", "diff"]).is_allowed());
        // benign global in front of a denied subcommand still denies on the subcommand
        assert_eq!(
            reason(&decide(&["--no-pager", "push"])),
            "git_network_denied"
        );
    }

    #[test]
    fn unknown_subcommand_and_aliases_denied() {
        // Unknown subcommand (incl. any configured alias) is refused.
        assert_eq!(reason(&decide(&["wtf"])), "git_subcommand_denied");
        assert_eq!(reason(&decide(&["my-alias"])), "git_subcommand_denied");
    }

    #[test]
    fn bare_git_and_version_allowed() {
        assert!(git_command_decision(&[], "b").is_allowed());
        assert!(decide(&["--version"]).is_allowed());
        assert!(decide(&["--help"]).is_allowed());
    }

    #[test]
    fn reflog_read_allowed_mutation_denied() {
        assert!(decide(&["reflog"]).is_allowed());
        assert!(decide(&["reflog", "show"]).is_allowed());
        assert_eq!(
            reason(&decide(&["reflog", "expire", "--all"])),
            "git_integration_denied"
        );
        assert_eq!(
            reason(&decide(&["reflog", "delete", "HEAD@{0}"])),
            "git_integration_denied"
        );
    }

    #[test]
    fn error_renders_typed_block() {
        if let GitDecision::Deny(e) = decide(&["push", "origin", "HEAD"]) {
            let r = e.render();
            assert!(r.contains("refused `git push origin HEAD`"));
            assert!(r.contains("git_network_denied"));
            assert!(r.contains("fixes:"));
            assert!(r.contains("docs:"));
            assert_eq!(e.command(), "git push origin HEAD");
        } else {
            panic!("expected deny");
        }
    }
}
