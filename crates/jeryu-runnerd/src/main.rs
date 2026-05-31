#![doc = "jeryu_runnerd CLI entrypoint."]

use jeryu_runner_core::receipt::ReceiptStatus;
use jeryu_runnerd::{DispatchEngine, DispatchMode, load_job_file};
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let args = env::args().collect::<Vec<_>>();
    let Some(command) = args.get(1).map(String::as_str) else {
        print_usage();
        return Ok(ExitCode::from(2));
    };

    match command {
        "explain" => {
            let path = args.get(2).ok_or_else(|| "missing job file".to_string())?;
            let job = load_job_file(path).map_err(|err| err.to_string())?;
            let receipt = DispatchEngine::new().dispatch(&job, DispatchMode::Explain);
            println!("{}", receipt.to_json());
            Ok(status_to_exit(receipt.status))
        }
        "run" => {
            let path = args.get(2).ok_or_else(|| "missing job file".to_string())?;
            let job = load_job_file(path).map_err(|err| err.to_string())?;
            let receipt = DispatchEngine::new().dispatch(&job, DispatchMode::Run);
            println!("{}", receipt.to_json());
            Ok(status_to_exit(receipt.status))
        }
        "self-test" => {
            let probe = jeryu_runnerd::startup_probe::native_startup_probe();
            println!(
                "{{\"startup_probe_ms\":{},\"started_ms\":{},\"finished_ms\":{}}}",
                probe.elapsed.as_millis(),
                probe.started_ms,
                probe.finished_ms
            );
            Ok(ExitCode::SUCCESS)
        }
        _ => {
            print_usage();
            Ok(ExitCode::from(2))
        }
    }
}

fn status_to_exit(status: ReceiptStatus) -> ExitCode {
    match status {
        ReceiptStatus::Planned | ReceiptStatus::Passed => ExitCode::SUCCESS,
        ReceiptStatus::Failed | ReceiptStatus::TimedOut => ExitCode::from(1),
        ReceiptStatus::Denied => ExitCode::from(3),
    }
}

fn print_usage() {
    eprintln!("usage: jeryu_runnerd <explain|run|self-test> [job-file]");
}
