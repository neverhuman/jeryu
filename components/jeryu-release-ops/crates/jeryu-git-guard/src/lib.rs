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

mod decision;
mod error;
mod policy;
mod subcommands;

pub use decision::{git_command_decision, leading_subcommand};
pub use error::{GitDecision, GitGuardError};

/// Process exit code the wrapper returns when a command is refused.
pub const DENY_EXIT_CODE: i32 = 13;

#[cfg(test)]
mod tests;
