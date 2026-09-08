//! Private PTY child-side wiring shared by the public launch paths.

use std::io::Error as IoError;
use std::os::fd::RawFd;

/// Make the PTY `slave` fd the calling (session-leader) process's controlling
/// terminal and wire stdin/stdout/stderr to it. Caller must already be a session
/// leader (`setsid`). Async-signal-safe: only direct syscalls, no allocation.
pub(super) fn wire_pty_slave(slave: RawFd) -> std::io::Result<()> {
    // SAFETY: as the session leader ioctl(TIOCSCTTY) claims the slave as our
    // controlling tty; dup2 wires the three standard fds to it. Both are direct
    // syscalls with no shared state.
    unsafe {
        if libc::ioctl(slave, libc::TIOCSCTTY, 0) != 0 {
            return Err(IoError::last_os_error());
        }
        for target in 0..3 {
            if libc::dup2(slave, target) == -1 {
                return Err(IoError::last_os_error());
            }
        }
        if slave > 2 {
            libc::close(slave);
        }
    }
    Ok(())
}
