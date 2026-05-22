use super::*;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};
use vt100::Screen;

pub(crate) fn run_and_capture(args: &Args, cell_w: u32, cell_h: u32) -> Result<Screen> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: args.rows,
        cols: args.cols,
        pixel_width: clamp_u16(u32::from(args.cols) * cell_w),
        pixel_height: clamp_u16(u32::from(args.rows) * cell_h),
    })?;

    let mut cmd = CommandBuilder::new(&args.cmd[0]);
    for arg in args.cmd.iter().skip(1) {
        cmd.arg(arg);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("LANG", "C.UTF-8");
    cmd.env("LC_ALL", "C.UTF-8");
    cmd.env("COLUMNS", args.cols.to_string());
    cmd.env("LINES", args.rows.to_string());
    cmd.env("CLICOLOR_FORCE", "1");
    cmd.env_remove("NO_COLOR");
    if let Some(path) = &args.ready_file {
        cmd.env("TUI_READY_FILE", path);
    }

    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    thread::spawn(move || {
        let mut buf = [0u8; 16384];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut parser = vt100::Parser::new(args.rows, args.cols, 2000);
    let start = Instant::now();
    let mut last_output = Instant::now();
    let mut sent = args.send.is_empty();
    let min_wait = Duration::from_millis(args.min_wait_ms);
    let max_wait = Duration::from_millis(args.max_wait_ms);
    let quiet = Duration::from_millis(args.quiet_ms);
    let send_after = Duration::from_millis(args.send_after_ms);

    loop {
        if !sent && start.elapsed() >= send_after {
            for s in &args.send {
                writer.write_all(&decode_escapes(s))?;
                writer.flush()?;
                thread::sleep(Duration::from_millis(30));
            }
            sent = true;
        }

        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(bytes) => {
                parser.process(&bytes);
                last_output = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if start.elapsed() >= max_wait {
            break;
        }
        let ready = match &args.ready_file {
            Some(path) => path.metadata().is_ok_and(|meta| meta.len() > 0),
            None => true,
        };
        if ready && start.elapsed() >= min_wait && last_output.elapsed() >= quiet {
            break;
        }
    }

    let screen = parser.screen().clone();
    let _ = child.kill();
    let _ = child.wait();
    Ok(screen)
}
