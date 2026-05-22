use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CiSchema {
    pub(crate) jobs: Vec<CiSchemaJob>,
    #[serde(default)]
    pub(crate) milestones: Vec<CiSchemaMilestone>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub(crate) struct CiSchemaJob {
    pub(crate) id: String,
    pub(crate) lane: String,
    pub(crate) release_blocking: bool,
    #[serde(default)]
    pub(crate) section: String,
    #[serde(default)]
    pub(crate) summary: String,
    #[serde(default)]
    pub(crate) runner_tags: String,
    #[serde(default)]
    pub(crate) runner_pool: String,
    #[serde(default)]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) component: String,
    #[serde(default)]
    pub(crate) pipeline_product: String,
    #[serde(default)]
    pub(crate) evidence_driven: bool,
    #[serde(default)]
    pub(crate) depends_on: Vec<String>,
    #[serde(default)]
    pub(crate) evidence_outputs: Vec<String>,
    #[serde(default)]
    pub(crate) estimated_cost: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub(crate) struct CiSchemaMilestone {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) lane: String,
    pub(crate) release_blocking: bool,
    #[serde(default)]
    pub(crate) pipeline_product: String,
    pub(crate) jobs: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub(crate) enum ReleaseHealth {
    Blocked,
    Ready,
    Running,
    RemotePassed,
    E2ePassed,
    Failed,
    Outdated,
}
