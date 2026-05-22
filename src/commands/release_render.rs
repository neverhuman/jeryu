use anyhow::Result;
use jeryu::release;

pub(super) fn print_preflight_report(report: &release::PreflightReport, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!(
            "Preflight {}: {} blocker(s)",
            if report.ok { "PASS" } else { "FAIL" },
            report.blockers.len()
        );
        for b in &report.blockers {
            println!("  [{}] {} — {}", b.code, b.detail, b.recommended_action);
        }
    }
    Ok(())
}

pub(super) fn print_doctor_report(report: &release::DoctorReport, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        let status = if report.blockers.is_empty() {
            "OK"
        } else {
            "BLOCKED"
        };
        println!("Doctor [{status}]: {}", report.version);
        println!("  next_action: {}", report.next_action);
        println!("  canary_complete: {}", report.canary_complete);
        println!("  prod_complete: {}", report.prod_complete);
        println!("  safe_to_reconcile: {}", report.safe_to_reconcile);
        if !report.preflight.is_empty() {
            println!("\nPreflight:");
            for (k, v) in &report.preflight {
                println!("  {k}: {v}");
            }
        }
        println!("\nGates:");
        for (k, v) in &report.gates {
            println!("  {k}: {}", if *v { "present" } else { "MISSING" });
        }
        if !report.blockers.is_empty() {
            println!("\nBlockers:");
            for b in &report.blockers {
                println!("  - {:?}", b);
            }
        }
    }
    Ok(())
}
