#![doc = "Thin entrypoint for the load-aware CI worker authority."]
#![doc = ""]
#![doc = "All policy lives in the `jeryu_ci_governor` library. This binary probes"]
#![doc = "live system pressure, asks the library how many workers may run, and prints"]
#![doc = "that single integer to stdout so a shell can wire it straight into a job:"]
#![doc = ""]
#![doc = "```sh"]
#![doc = "WORKERS=$(jeryu-ci-governor --request \"$requested\")"]
#![doc = "```"]
#![doc = ""]
#![doc = "With `--explain`, the reason and the system snapshot are written to stderr"]
#![doc = "so stdout stays a clean integer."]

use jeryu_ci_governor::{GovernorConfig, SystemLoad, decide_jobs};

fn main() {
    let mut request: Option<u32> = None;
    let mut explain = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--request" => {
                request = args.next().and_then(|v| v.parse::<u32>().ok());
            }
            "--explain" => explain = true,
            other => {
                if let Some(value) = other.strip_prefix("--request=") {
                    request = value.parse::<u32>().ok();
                }
            }
        }
    }

    let load = SystemLoad::probe();
    let cfg = GovernorConfig::default();
    let decision = decide_jobs(&load, request, &cfg);

    if explain {
        eprintln!(
            "binding={} reason=\"{}\"",
            decision.binding.label(),
            decision.reason
        );
        eprintln!(
            "system: ncpu={} avail_mem={}MiB total_mem={}MiB swap_used={}MiB load1={:.2}",
            load.ncpu,
            load.available_mem_bytes / (1024 * 1024),
            load.total_mem_bytes / (1024 * 1024),
            load.swap_used_bytes / (1024 * 1024),
            load.load1,
        );
    }

    println!("{}", decision.jobs);
}
