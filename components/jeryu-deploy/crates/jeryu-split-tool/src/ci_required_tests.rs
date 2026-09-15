use super::*;
use serde_json::{Value, json};

fn fixture() -> (Arguments, Value, Value) {
    let arguments = Arguments {
        run: PathBuf::new(),
        jobs: PathBuf::new(),
        repository: "neverhuman/jeryu".into(),
        source: "a".repeat(40),
        run_id: 42,
        attempt: 3,
    };
    let run = json!({"id":42,"run_attempt":3,"head_sha":arguments.source,
        "path":".github/workflows/ci.yml","name":"Jeryu",
        "repository":{"full_name":"neverhuman/jeryu"}});
    let mut jobs: Vec<Value> = REQUIRED
        .iter()
        .enumerate()
        .map(|(index, lane)| {
            json!({
                "id":index+1,"name":format!("verify / {lane}"),"run_id":42,"run_attempt":3,
                "head_sha":arguments.source,"workflow_name":"Jeryu","status":"completed","conclusion":"success"
            })
        })
        .collect();
    jobs.push(json!({"id":99,"name":"jeryu/required","run_id":42,"run_attempt":3,
        "head_sha":arguments.source,"workflow_name":"Jeryu","status":"in_progress","conclusion":null}));
    (
        arguments,
        run,
        json!({"total_count":jobs.len(),"jobs":jobs}),
    )
}

fn check(arguments: &Arguments, run: &Value, jobs: &Value) -> Result<()> {
    verify(
        arguments,
        &serde_json::to_vec(run)?,
        &serde_json::to_vec(jobs)?,
    )
}

#[test]
fn accepts_complete_exact_attempt_and_complete_pagination() {
    let (arguments, run, jobs) = fixture();
    check(&arguments, &run, &jobs).unwrap();
    let all = jobs["jobs"].as_array().unwrap();
    let total = all.len();
    let pages = format!(
        "{}\n{}",
        json!({"total_count":total,"jobs":&all[..4]}),
        json!({"total_count":total,"jobs":&all[4..]})
    );
    verify(
        &arguments,
        &serde_json::to_vec(&run).unwrap(),
        pages.as_bytes(),
    )
    .unwrap();
}

#[test]
fn rejects_advisory_jobs_in_the_required_workflow() {
    let (arguments, run, mut jobs) = fixture();
    jobs["jobs"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({
            "id":50,"name":"host-or-nightly / auditor","run_id":42,"run_attempt":3,
            "head_sha":arguments.source,"workflow_name":"Jeryu","status":"completed","conclusion":"failure"
        }));
    jobs["total_count"] = json!(jobs["jobs"].as_array().unwrap().len());
    assert!(check(&arguments, &run, &jobs).is_err());
}

#[test]
fn rejects_every_unsuccessful_or_unfinished_required_job() {
    for conclusion in [
        "failure",
        "cancelled",
        "timed_out",
        "skipped",
        "neutral",
        "action_required",
        "stale",
    ] {
        let (arguments, run, mut jobs) = fixture();
        jobs["jobs"][0]["conclusion"] = json!(conclusion);
        assert!(check(&arguments, &run, &jobs).is_err(), "{conclusion}");
    }
    let (arguments, run, mut jobs) = fixture();
    jobs["jobs"][0]["status"] = json!("in_progress");
    assert!(check(&arguments, &run, &jobs).is_err());
    jobs["jobs"][0]["conclusion"] = Value::Null;
    assert!(check(&arguments, &run, &jobs).is_err());
}

#[test]
fn rejects_incomplete_duplicate_and_substituted_jobs() {
    for change in [
        "missing",
        "total",
        "duplicate-id",
        "duplicate-name",
        "unexpected",
        "aggregate",
    ] {
        let (arguments, run, mut jobs) = fixture();
        match change {
            "missing" => {
                jobs["jobs"].as_array_mut().unwrap().remove(0);
            }
            "total" => jobs["total_count"] = json!(7),
            "duplicate-id" => jobs["jobs"][1]["id"] = jobs["jobs"][0]["id"].clone(),
            "duplicate-name" => jobs["jobs"][1]["name"] = jobs["jobs"][0]["name"].clone(),
            "unexpected" => jobs["jobs"][0]["name"] = json!("verify / easy"),
            "aggregate" => {
                let last = jobs["jobs"].as_array().unwrap().len() - 1;
                jobs["jobs"][last]["name"] = json!("another aggregate");
            }
            _ => unreachable!(),
        }
        assert!(check(&arguments, &run, &jobs).is_err(), "{change}");
    }
}

#[test]
fn rejects_another_source_repository_workflow_or_attempt() {
    for (field, value) in [
        ("id", json!(43)),
        ("run_attempt", json!(2)),
        ("head_sha", json!("b".repeat(40))),
        ("path", json!(".github/workflows/easy.yml")),
        ("name", json!("Other")),
        ("repository", json!({"full_name":"attacker/jeryu"})),
    ] {
        let (arguments, mut run, jobs) = fixture();
        run[field] = value;
        assert!(check(&arguments, &run, &jobs).is_err(), "{field}");
    }
    for (field, value) in [
        ("run_id", json!(43)),
        ("run_attempt", json!(2)),
        ("head_sha", json!("b".repeat(40))),
        ("workflow_name", json!("Other")),
    ] {
        let (arguments, run, mut jobs) = fixture();
        jobs["jobs"][0][field] = value;
        assert!(check(&arguments, &run, &jobs).is_err(), "{field}");
    }
}

#[test]
fn rejects_malformed_empty_and_duplicate_field_pages() {
    let (arguments, run, _) = fixture();
    let run = serde_json::to_vec(&run).unwrap();
    for pages in [
        "",
        "[]",
        "null",
        "{}",
        "{\"total_count\":8,\"total_count\":8,\"jobs\":[]}",
    ] {
        assert!(
            verify(&arguments, &run, pages.as_bytes()).is_err(),
            "{pages}"
        );
    }
}
