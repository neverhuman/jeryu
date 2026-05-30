#![forbid(unsafe_code)]

use jeryu_phase11_core::TenantId;
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args = env::args().collect::<Vec<_>>();
    let command = args.get(1).map(String::as_str).unwrap_or("help");
    let tenant =
        match TenantId::new(env::var("JERYU_TENANT").unwrap_or_else(|_| "tenant-a".to_string())) {
            Ok(tenant) => tenant,
            Err(error) => {
                eprintln!("invalid tenant: {}", error);
                return ExitCode::from(2);
            }
        };
    let actor = env::var("JERYU_ACTOR").unwrap_or_else(|_| "phase11-cli".to_string());

    match command {
        "readiness" => {
            println!("{}", jeryu_kernel::readiness(tenant, &actor).to_json());
            ExitCode::SUCCESS
        }
        "evidence" => {
            let report = jeryu_kernel::readiness(tenant, &actor);
            println!("{}", report.compliance_json);
            ExitCode::SUCCESS
        }
        "upgrade" => {
            let report = jeryu_kernel::readiness(tenant, &actor);
            println!("{}", report.lifecycle_json);
            ExitCode::SUCCESS
        }
        "replay" => {
            let report = jeryu_kernel::readiness(tenant, &actor);
            println!("{}", report.replay_json);
            ExitCode::SUCCESS
        }
        "help" | "--help" | "-h" => {
            println!("{}", jeryu_kernel::help());
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command: {}", other);
            eprintln!("{}", jeryu_kernel::help());
            ExitCode::from(2)
        }
    }
}
