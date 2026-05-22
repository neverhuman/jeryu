use serde::Deserialize;

use super::SharedState;

#[derive(Debug, Deserialize)]
pub(crate) struct PipelineHookPayload {
    pub(crate) project: Option<ProjectInfo>,
    pub(crate) object_attributes: Option<PipelineAttributes>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectInfo {
    pub(crate) id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PipelineAttributes {
    pub(crate) id: Option<i64>,
    pub(crate) status: Option<String>,
    pub(crate) sha: Option<String>,
    #[serde(rename = "ref")]
    pub(crate) ref_name: Option<String>,
}

pub(crate) async fn handle_pipeline_event_from_body(
    state: SharedState,
    body: &str,
) -> Result<(), serde_json::Error> {
    let payload = serde_json::from_str::<PipelineHookPayload>(body)?;
    super::handle_pipeline_event(state, payload).await;
    Ok(())
}
