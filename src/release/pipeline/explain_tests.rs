use super::*;

#[test]
fn empty_pipeline_reports_its_own_blocker() {
    let blocker = explain_release_candidate_blocker(&HashMap::new(), false, &[], &[]);
    assert_eq!(blocker.as_deref(), Some("materialized pipeline is empty"));
}

#[test]
fn omitted_release_candidate_jobs_keep_the_vti_blocker() {
    let aggregated = HashMap::from([(
        "compile-workspace".to_string(),
        AggregatedPipelineJob::default(),
    )]);
    let blocker = explain_release_candidate_blocker(&aggregated, false, &[], &[]);
    assert_eq!(
        blocker.as_deref(),
        Some("release candidate jobs omitted by VTI")
    );
}
