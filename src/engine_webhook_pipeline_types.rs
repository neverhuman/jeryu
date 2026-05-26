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
    pub(crate) path_with_namespace: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_hook_payload_carries_project_slug_for_sidecars() {
        let payload: PipelineHookPayload = serde_json::from_str(
            r#"{
                "project": {"id": 42, "path_with_namespace": "root/jekko"},
                "object_attributes": {
                    "id": 7,
                    "status": "success",
                    "sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "ref": "main"
                }
            }"#,
        )
        .unwrap();

        let project = payload.project.unwrap();
        assert_eq!(project.id, Some(42));
        assert_eq!(project.path_with_namespace.as_deref(), Some("root/jekko"));
        let attrs = payload.object_attributes.unwrap();
        assert_eq!(attrs.status.as_deref(), Some("success"));
        assert_eq!(attrs.ref_name.as_deref(), Some("main"));
    }
}
