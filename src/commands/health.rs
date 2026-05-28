use anyhow::Result;

pub(crate) async fn execute_health_command(json: bool, ci: bool) -> Result<i32> {
    let report = jeryu::health::build_health_report(jeryu::health::HealthOptions { ci }).await;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_health_report(&report);
    }
    Ok(if report.ok { 0 } else { 1 })
}

fn print_health_report(report: &jeryu::health::HealthReport) {
    println!("--- jeryu health ---");
    println!("  Mode:   {:?}", report.mode);
    println!("  Status: {}", report.status);
    println!(
        "  Checks: {} ok / {} degraded / {} failed / {} total",
        report.summary.checks_ok,
        report.summary.checks_degraded,
        report.summary.checks_failed,
        report.summary.checks_total
    );
    if let Some(topology) = &report.pool_topology {
        println!(
            "  Runners: active={} desired={}",
            topology.active_total, topology.desired_total
        );
    }
    if !report.reserved_runner_nodes.is_empty() {
        println!("  Reserved:");
        for node in &report.reserved_runner_nodes {
            println!(
                "    {} active={} desired={} {}",
                node.node_alias, node.active, node.desired, node.reason
            );
        }
    }
    println!();
    for check in &report.checks {
        println!("  [{:?}] {:<24} {}", check.status, check.id, check.detail);
    }
}
