//! Phase 1 Git server core for Jeryu.
//!
//! The crate intentionally uses the stock `git` binary for pack-protocol execution.
//! Jeryu owns policy, routing, quarantine, object validation, LFS storage, and
//! safe command dispatch while delegating Git's byte-for-byte pack semantics to Git
//! itself during Phase 1.

pub mod audit;
pub mod cli;
pub mod command;
pub mod config;
pub mod error;
pub mod hash;
pub mod hooks;
pub mod lfs;
pub mod mirror;
pub mod object_fsck;
pub mod pack;
pub mod path;
pub mod pktline;
pub mod protection;
pub mod protocol_v2;
pub mod quarantine;
pub mod refs;
pub mod repo;
pub mod smart_http;
pub mod ssh;

pub use config::GitdConfig;
pub use error::{GitdError, Result};
pub use repo::{RepoId, RepoManager};
