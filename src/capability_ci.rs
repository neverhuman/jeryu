use super::*;

#[derive(Debug, Serialize)]
struct CiJob<'a> {
    stage: &'a str,
    tags: Vec<&'a str>,
    script: Vec<&'a str>,
}

pub(crate) fn dynamic_ci_yaml(scope: &str) -> anyhow::Result<String> {
    let (job_suffix, script) = match scope {
        "unit" => ("unit", vec!["cargo test --lib --benches"]),
        "integration" => ("integration", vec!["cargo test --test '*'"]),
        "lint" => (
            "lint",
            vec![
                "cargo clippy --all-targets --all-features -- -D warnings",
                "cargo fmt -- --check",
            ],
        ),
        "full" => ("full", vec!["cargo test"]),
        other => {
            anyhow::bail!(
                "unsupported test scope '{other}'; allowed scopes: unit, integration, lint, full"
            )
        }
    };

    let mut doc = BTreeMap::new();
    doc.insert("image".to_string(), serde_yaml::to_value("rust:latest")?);
    doc.insert("stages".to_string(), serde_yaml::to_value(vec!["test"])?);
    doc.insert(
        format!("dynamic-{job_suffix}-job"),
        serde_yaml::to_value(CiJob {
            stage: "test",
            tags: vec!["jeryu"],
            script,
        })?,
    );
    Ok(serde_yaml::to_string(&doc)?)
}
