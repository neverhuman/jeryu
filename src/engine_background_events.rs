use tracing::{debug, error, warn};

use super::SharedState;
use super::reconcile::reconcile_once;

pub(crate) async fn docker_event_loop(state: SharedState) {
    use futures_util::StreamExt;

    debug!("starting docker event listener");
    let mut stream = state.docker.events();

    while let Some(msg) = stream.next().await {
        if let Ok(event) = msg
            && let Some(typ) = event.typ
            && typ == bollard::models::EventMessageTypeEnum::CONTAINER
            && let Some(action) = event.action
            && (action == "die" || action == "oom")
            && let Some(actor) = event.actor
            && let Some(attrs) = actor.attributes
            && attrs.get("jeryu.managed").map(|s| s.as_str()) == Some("true")
        {
            let name = match attrs.get("name").cloned() {
                Some(n) => n,
                None => String::new(),
            };
            warn!(%name, action, "jeryu manager container terminated unexpectedly");
            if let Some(manager_id) = attrs.get("jeryu.manager_id")
                && let Err(error) = state.db.update_manager_state(manager_id, "stopped").await
            {
                error!(%manager_id, %error, "failed to mark dead runner manager stopped");
            }
            if let Err(e) = reconcile_once(&state).await {
                error!(error = %e, "reconciliation failed after container death");
            }
        }
    }
}
