//! Live `jeryu autonomy doctor` smoke test.
//!
//! Pings every configured provider with a 10-token PING and reports
//! OK / NOKEY / AUTH / RATE / DOWN. Gated on `JERYU_LLM_LIVE=1`.
//!
//! ```bash
//! JERYU_LLM_LIVE=1 cargo test --test llm_doctor -- --ignored --nocapture
//! ```

use jeryu::llm::{DoctorProbe, ProviderStatus, SecretResolver, render_report, sweep_providers};

#[tokio::test]
#[ignore = "live LLM call; set JERYU_LLM_LIVE=1 to run"]
async fn sweep_all_providers() {
    if std::env::var("JERYU_LLM_LIVE").as_deref() != Ok("1") {
        eprintln!("JERYU_LLM_LIVE not set; skipping");
        return;
    }
    let probes = DoctorProbe::default_set();
    let resolver = SecretResolver::from_env();
    let results = sweep_providers(&probes, &resolver).await;
    let report = render_report(&results);
    eprintln!("\n{report}");

    // We assert that at least one provider was OK — that's the
    // user's contract: "your ~/llm.env should give you a working stack."
    let ok_count = results
        .iter()
        .filter(|r| r.status == ProviderStatus::Ok)
        .count();
    assert!(
        ok_count >= 1,
        "no provider returned OK; full report:\n{report}"
    );
}
