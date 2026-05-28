use anyhow::Result;
use tracing::info;

use crate::gitlab_client::GitlabClient;

pub async fn enforce_untagged_runners(client: &GitlabClient) -> Result<usize> {
    let runners = client.list_all_runners().await?;
    let mut updated = 0;

    for runner in runners {
        if runner.tag_list.is_empty() && runner.run_untagged {
            continue;
        }

        client.update_runner(runner.id, &[], true).await?;
        updated += 1;
        info!(runner_id = runner.id, description = ?runner.description, "normalized runner to untagged policy");
    }

    Ok(updated)
}
