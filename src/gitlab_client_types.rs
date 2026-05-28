use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Deserialize)]
pub struct ProjectPatResp {
    pub id: i64,
    pub name: String,
    pub token: String,
    pub user_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct RunnerCreated {
    pub id: i64,
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct RunnerInfo {
    pub id: i64,
    pub description: Option<String>,
    pub paused: Option<bool>,
    #[serde(default)]
    pub tag_list: Vec<String>,
    #[serde(default)]
    pub run_untagged: bool,
}

#[derive(Debug, Deserialize)]
pub struct RunnerManager {
    pub system_id: Option<String>,
    pub status: Option<String>,
    pub contacted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Job {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub stage: String,
    #[serde(default)]
    pub allow_failure: bool,
    #[serde(skip)]
    pub pipeline_id: Option<i64>,
    #[serde(default)]
    pub pipeline: Option<PipelineRef>,
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
    pub web_url: Option<String>,
    pub queued_duration: Option<f64>,
    pub duration: Option<f64>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    #[serde(default)]
    pub tag_list: Vec<String>,
    pub runner: Option<JobRunner>,
}

impl Job {
    pub fn effective_pipeline_id(&self) -> Option<i64> {
        match self.pipeline_id {
            Some(id) => Some(id),
            None => self.pipeline.as_ref().map(|p| p.id),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct JobRunner {
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Pipeline {
    pub id: i64,
    pub sha: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub status: String,
    pub web_url: Option<String>,
    pub yaml_errors: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PipelineBridge {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub downstream_pipeline: Option<PipelineRef>,
}

#[derive(Debug, Deserialize)]
pub struct PipelineRef {
    pub id: i64,
    pub sha: Option<String>,
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
    pub status: Option<String>,
    pub web_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PipelineVariableValue {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub path_with_namespace: String,
    pub web_url: String,
}

#[derive(Debug, Deserialize)]
pub struct Issue {
    pub id: i64,
    pub iid: i64,
    pub title: String,
    pub state: String,
    pub labels: Vec<String>,
    pub web_url: String,
}

#[derive(Debug, Deserialize)]
pub struct MergeRequest {
    pub id: i64,
    pub iid: i64,
    pub title: String,
    pub state: String,
    pub web_url: String,
    pub source_branch: String,
    pub target_branch: String,
}

#[derive(Serialize)]
pub struct PipelineVariable<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_pipeline_id_prefers_nested_pipeline_metadata() {
        let job: Job = serde_json::from_str(
            r#"{
                "id": 17,
                "name": "build-enclave-server",
                "status": "running",
                "stage": "package",
                "allow_failure": false,
                "pipeline": { "id": 4242, "status": "running" }
            }"#,
        )
        .expect("deserialize job");

        assert_eq!(job.pipeline_id, None);
        assert_eq!(job.effective_pipeline_id(), Some(4242));
    }
}
