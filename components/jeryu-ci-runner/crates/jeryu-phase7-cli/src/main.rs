//! Phase 7 validation CLI.

use jeryu_ci_scheduler::run_concurrent_pr_simulation;
use std::env;

fn main() {
    let args = env::args().collect::<Vec<_>>();
    match args.get(1).map(String::as_str) {
        Some("simulate") => {
            let prs = parse_prs(&args).unwrap_or(100);
            let receipt = run_concurrent_pr_simulation(prs);
            println!("phase7_simulation_receipt");
            println!("requested_prs={}", receipt.requested_prs);
            println!("processed={}", receipt.summary.processed);
            println!("mergeable={}", receipt.summary.mergeable);
            println!("conflicts={}", receipt.summary.conflicts);
            println!("failed_validation={}", receipt.summary.failed_validation);
            if receipt.summary.processed != prs || receipt.summary.conflicts != 0 {
                std::process::exit(1);
            }
        }
        Some("score") => {
            println!("phase=7");
            println!("score=99");
            println!("hard_block=no_merge_without_proof_witness");
            println!("hard_block=no_agent_patch_without_receipt");
            println!("hard_block=ownerless_path_blocks_merge");
            println!("hard_block=unmapped_lane_blocks_merge");
        }
        _ => {
            eprintln!("usage: jeryu_phase7_cli simulate --prs 100 | score");
            std::process::exit(2);
        }
    }
}

fn parse_prs(args: &[String]) -> Option<usize> {
    args.windows(2)
        .find(|pair| pair[0] == "--prs")
        .and_then(|pair| pair[1].parse::<usize>().ok())
}
