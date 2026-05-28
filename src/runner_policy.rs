use anyhow::Result;
use std::collections::BTreeSet;
use tracing::info;

use crate::gitlab_client::GitlabClient;
use crate::state::Pool;

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

pub async fn enforce_pool_runner_policy(client: &GitlabClient, pools: &[Pool]) -> Result<usize> {
    let runners = client.list_all_runners().await?;
    let mut updated = 0;

    for pool in pools {
        let Some(runner) = runners
            .iter()
            .find(|runner| runner.id == pool.gitlab_runner_id)
        else {
            continue;
        };

        let desired_tags = parse_pool_tags(&pool.tags);
        let desired_run_untagged = desired_tags.is_empty();
        if tags_match(&runner.tag_list, &desired_tags)
            && runner.run_untagged == desired_run_untagged
        {
            continue;
        }

        let tag_refs = desired_tags.iter().map(String::as_str).collect::<Vec<_>>();
        client
            .update_runner(runner.id, &tag_refs, desired_run_untagged)
            .await?;
        updated += 1;
        info!(
            runner_id = runner.id,
            pool = %pool.name,
            run_untagged = desired_run_untagged,
            tag_count = desired_tags.len(),
            "normalized runner to pool policy"
        );
    }

    Ok(updated)
}

fn parse_pool_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

fn tags_match(current: &[String], desired: &[String]) -> bool {
    current.iter().collect::<BTreeSet<_>>() == desired.iter().collect::<BTreeSet<_>>()
}
