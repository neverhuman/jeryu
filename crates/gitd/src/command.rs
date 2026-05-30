//! External command execution with explicit error handling.

use crate::error::{GitdError, Result};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Output captured from a command.
#[derive(Debug)]
pub struct CommandOutput {
    /// Standard output bytes.
    pub stdout: Vec<u8>,
    /// Standard error text.
    pub stderr: String,
}

/// Run a command and capture output.
pub fn run_capture(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<CommandOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(GitdError::GitCommandFailed {
            program: format!("{} {}", program, args.join(" ")),
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        });
    }
    Ok(CommandOutput {
        stdout: out.stdout,
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    })
}

/// Run a command with bytes on stdin and capture output.
pub fn run_with_stdin(
    program: &str,
    args: &[&str],
    stdin: &[u8],
    cwd: Option<&Path>,
) -> Result<CommandOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let mut child = cmd.spawn()?;
    if let Some(mut pipe) = child.stdin.take() {
        pipe.write_all(stdin)?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(GitdError::GitCommandFailed {
            program: format!("{} {}", program, args.join(" ")),
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        });
    }
    Ok(CommandOutput {
        stdout: out.stdout,
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    })
}

/// Replace the current process with a git service command on Unix, or run it on
/// non-Unix platforms.
pub fn exec_or_run(program: &str, args: &[&str]) -> Result<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = Command::new(program).args(args).exec();
        Err(GitdError::Io(err))
    }
    #[cfg(not(unix))]
    {
        let status = Command::new(program).args(args).status()?;
        Ok(status.code().unwrap_or(1))
    }
}
