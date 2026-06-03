//! Agent execution driver contracts.
//!
//! Deterministic edit-bot tests run through a network-denied profile and stage
//! their executable in a unique tempdir. The executable is written under a
//! private pending directory and made visible only after an atomic directory
//! rename to `ready`, so callers never execute a partially written bot.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use jeryu_egress::LiveAgentRuntimeContract;
use jeryu_runner_core::{NetworkPolicy, SecretPolicy};
use tempfile::{Builder, TempDir};

/// Driver mode for agent execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentDriverMode {
    /// Self-authored deterministic edit-bot fixture. No network, no secrets.
    DeterministicEditBot,
    /// Opt-in live agent runtime. The egress contract owns network, secret, and
    /// budget validation before any live process is launched.
    LiveAgent(LiveAgentRuntimeContract),
}

impl AgentDriverMode {
    /// Effective network policy for this mode.
    pub fn network_policy(&self) -> NetworkPolicy {
        match self {
            Self::DeterministicEditBot => NetworkPolicy::Deny,
            Self::LiveAgent(contract) => contract.network_policy,
        }
    }

    /// Effective secret policy for this mode.
    pub fn secret_policy(&self) -> SecretPolicy {
        match self {
            Self::DeterministicEditBot => SecretPolicy::None,
            Self::LiveAgent(contract) => contract.secret_policy,
        }
    }
}

/// Agent driver rooted at a caller-provided scratch directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentDriver {
    scratch_root: PathBuf,
    mode: AgentDriverMode,
}

impl AgentDriver {
    /// Network-denied deterministic edit-bot driver.
    pub fn deterministic_editbot(scratch_root: impl Into<PathBuf>) -> Self {
        Self {
            scratch_root: scratch_root.into(),
            mode: AgentDriverMode::DeterministicEditBot,
        }
    }

    /// Live agent driver using an explicit egress contract.
    pub fn live_agent(
        scratch_root: impl Into<PathBuf>,
        contract: LiveAgentRuntimeContract,
    ) -> Self {
        Self {
            scratch_root: scratch_root.into(),
            mode: AgentDriverMode::LiveAgent(contract),
        }
    }

    /// Driver mode.
    pub fn mode(&self) -> &AgentDriverMode {
        &self.mode
    }

    /// Effective network policy.
    pub fn network_policy(&self) -> NetworkPolicy {
        self.mode.network_policy()
    }

    /// Effective secret policy.
    pub fn secret_policy(&self) -> SecretPolicy {
        self.mode.secret_policy()
    }

    /// Stage a deterministic edit-bot executable through a unique tempdir and
    /// atomic pending-to-ready directory rename.
    pub fn stage_edit_bot(&self, executable: &[u8]) -> io::Result<StagedEditBot> {
        if !matches!(self.mode, AgentDriverMode::DeterministicEditBot) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "live agent mode does not stage deterministic edit-bots",
            ));
        }

        fs::create_dir_all(&self.scratch_root)?;
        let root = Builder::new()
            .prefix("jeryu-agentbridge-editbot-")
            .tempdir_in(&self.scratch_root)?;
        let pending_dir = root.path().join("pending");
        let ready_dir = root.path().join("ready");
        fs::create_dir(&pending_dir)?;

        let pending_executable = pending_dir.join("edit-bot");
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&pending_executable)?;
        file.write_all(executable)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);

        make_executable(&pending_executable)?;
        sync_dir(&pending_dir)?;
        fs::rename(&pending_dir, &ready_dir)?;
        sync_dir(root.path())?;

        Ok(StagedEditBot {
            root,
            executable: ready_dir.join("edit-bot"),
            ready_dir,
        })
    }

    /// Stage and run a deterministic edit-bot.
    pub fn run_edit_bot<I, S>(&self, executable: &[u8], args: I) -> io::Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let staged = self.stage_edit_bot(executable)?;
        Command::new(staged.executable())
            .args(args)
            .current_dir(staged.ready_dir())
            .output()
    }
}

/// Staged edit-bot. Dropping this value removes the unique tempdir.
pub struct StagedEditBot {
    root: TempDir,
    executable: PathBuf,
    ready_dir: PathBuf,
}

impl StagedEditBot {
    /// Unique root directory for this staged bot.
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// Ready directory made visible by atomic rename after all writes complete.
    pub fn ready_dir(&self) -> &Path {
        &self.ready_dir
    }

    /// Executable path under the ready directory.
    pub fn executable(&self) -> &Path {
        &self.executable
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn sync_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
    }
    Ok(())
}
