//! Deterministic in-cell edit-bot: the smallest code-writing process that
//! proves the agent driver loop end-to-end WITHOUT an LLM.
//!
//! This binary is the placeholder for a real `claude`/`codex` CLI. The driver
//! copies it INTO the cell checkout (so it is reachable under the workspace-only
//! Landlock rule) and execs it there. The bot is driven entirely by environment
//! variables so the host can choose its behaviour per test without any argv
//! parsing surprises:
//!
//!   * `EDITBOT_TARGET`  — path to write (relative paths resolve against the cwd,
//!     which the sandbox sets to the cell workspace).
//!   * `EDITBOT_CONTENT` — bytes to write into the target file (default `edit`).
//!   * `EDITBOT_SPEW`    — when set to a positive integer N, print N KiB of
//!     output to stdout (used to drive the output-budget kill path).
//!   * `EDITBOT_SPIN`    — when set to `1`, loop forever (used to drive the
//!     watchdog timeout path).
//!
//! Exit codes: `0` on a successful write, `1` when the write was denied (e.g.
//! Landlock blocked an out-of-cell target) or `EDITBOT_TARGET` was missing.

use std::io::Write;

fn main() {
    // Runaway path: spin forever so the watchdog has to group-kill us.
    if std::env::var("EDITBOT_SPIN").as_deref() == Ok("1") {
        loop {
            std::hint::spin_loop();
        }
    }

    // Budget path: emit a large, bounded-per-line stream so the driver's
    // output-budget cap trips and kills us mid-stream.
    if let Ok(kib) = std::env::var("EDITBOT_SPEW")
        && let Ok(kib) = kib.parse::<usize>()
    {
        let line = [b'x'; 1024];
        let mut out = std::io::stdout().lock();
        for _ in 0..kib {
            // A failed write means the reader (driver) has gone away after
            // killing us for budget; stop quietly rather than panic.
            if out.write_all(&line).is_err() || out.write_all(b"\n").is_err() {
                break;
            }
        }
        let _ = out.flush();
        return;
    }

    let Ok(target) = std::env::var("EDITBOT_TARGET") else {
        eprintln!("editbot: EDITBOT_TARGET not set");
        std::process::exit(1);
    };
    let content = std::env::var("EDITBOT_CONTENT").unwrap_or_else(|_| "edit".to_string());

    match std::fs::write(&target, content.as_bytes()) {
        Ok(()) => {
            println!("editbot: wrote {} byte(s) to {target}", content.len());
        }
        Err(err) => {
            // A Landlock-denied write outside the cell lands here; surface it so
            // the driver records a non-zero exit and the host FS stays untouched.
            eprintln!("editbot: write to {target} denied: {err}");
            std::process::exit(1);
        }
    }
}
