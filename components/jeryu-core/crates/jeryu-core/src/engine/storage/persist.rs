use std::collections::HashMap;

use rusqlite::{Connection, params};

use super::super::State;
use super::codec::*;
use super::{repo_id, storage_error};
use crate::errors::Result;

/// Serialize the desired rows into connection-local staging tables. Only the
/// snapshot reconciler may write the durable State-owned tables.
pub(super) fn stage_state(conn: &Connection, state: &State) -> Result<()> {
    for journal in state.repository_creations.values() {
        conn.execute(
            "INSERT INTO temp.repository_creation_journal (repository_id, receipt_json) VALUES (?1, ?2)",
            params![journal.repository_id.to_string(), json(journal)?],
        )
        .map_err(storage_error)?;
    }
    for user in state.users.values() {
        conn.execute(
            "INSERT INTO temp.users (login, user_json) VALUES (?1, ?2)",
            params![user.login, json(user)?],
        )
        .map_err(storage_error)?;
    }
    for account in state.accounts.values() {
        conn.execute(
            r#"
            INSERT INTO temp.user_accounts (
              login, display_name, password_hash, role, status, auth_epoch,
              must_change_password, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                account.canonical_login,
                account.display_name,
                account.password_hash,
                text(&account.role)?,
                text(&account.status)?,
                account.auth_epoch,
                bool_int(account.must_change_password),
                time(account.created_at),
                time(account.updated_at),
            ],
        )
        .map_err(storage_error)?;
    }
    for session in state.sessions.values() {
        conn.execute(
            r#"
            INSERT INTO temp.web_sessions (
              id, login, auth_epoch, token_hash, csrf_token, created_at, expires_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                session.id.to_string(),
                session.login,
                session.auth_epoch,
                session.token_hash,
                session.csrf_token,
                time(session.created_at),
                time(session.expires_at),
            ],
        )
        .map_err(storage_error)?;
    }
    for token in state.personal_tokens.values() {
        conn.execute(
            r#"
            INSERT INTO temp.personal_access_tokens (
              id, login, auth_epoch, name, token_hash, created_at, expires_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                token.id.to_string(),
                token.login,
                token.auth_epoch,
                token.name,
                token.token_hash,
                time(token.created_at),
                optional_time(token.expires_at),
            ],
        )
        .map_err(storage_error)?;
    }
    for invitation in state.invitations.values() {
        conn.execute(
            r#"
            INSERT INTO temp.account_invitations (
              id, canonical_login, display_name, activation_secret_hash,
              issuer_principal, intended_role, intended_teams_json, created_at,
              expires_at, consumed_at, revoked_at, attempt_count, bootstrap_owner
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                invitation.id.to_string(),
                invitation.canonical_login,
                invitation.display_name,
                invitation.activation_secret_hash,
                invitation.issuer_principal,
                text(&invitation.intended_bindings.role)?,
                json(&invitation.intended_bindings.teams)?,
                time(invitation.created_at),
                time(invitation.expires_at),
                optional_time(invitation.consumed_at),
                optional_time(invitation.revoked_at),
                invitation.attempt_count,
                bool_int(invitation.bootstrap_owner),
            ],
        )
        .map_err(storage_error)?;
    }
    for challenge in state.activation_challenges.values() {
        conn.execute(
            r#"
            INSERT INTO temp.account_activation_challenges (
              id, invitation_id, challenge_hash, created_at, expires_at, consumed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                challenge.id.to_string(),
                challenge.invitation_id.to_string(),
                challenge.challenge_hash,
                time(challenge.created_at),
                time(challenge.expires_at),
                optional_time(challenge.consumed_at),
            ],
        )
        .map_err(storage_error)?;
    }
    conn.execute(
        "INSERT INTO temp.owner_bootstrap_state (singleton, consumed) VALUES (1, ?1)",
        params![bool_int(state.bootstrap_owner_consumed)],
    )
    .map_err(storage_error)?;
    for organization in state.organizations.values() {
        conn.execute(
            "INSERT INTO temp.organizations (login, organization_json) VALUES (?1, ?2)",
            params![organization.login, json(organization)?],
        )
        .map_err(storage_error)?;
    }
    for ((organization, slug), team) in &state.teams {
        conn.execute(
            "INSERT INTO temp.teams (organization, slug, team_json) VALUES (?1, ?2, ?3)",
            params![organization, slug, json(team)?],
        )
        .map_err(storage_error)?;
    }

    let mut repo_ids = HashMap::new();
    for ((owner, name), repo) in &state.repos {
        repo_ids.insert((owner.clone(), name.clone()), repo.id.to_string());
        conn.execute(
            r#"
            INSERT INTO temp.repositories (
              id, owner, name, full_name, private, description, default_branch,
              archived, disabled, created_at, updated_at, family
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                repo.id.to_string(),
                repo.owner,
                repo.name,
                repo.full_name,
                bool_int(repo.private),
                repo.description,
                repo.default_branch,
                bool_int(repo.archived),
                bool_int(repo.disabled),
                time(repo.created_at),
                time(repo.updated_at),
                repo.family,
            ],
        )
        .map_err(storage_error)?;
    }

    for journal in state.repository_transfers.values() {
        conn.execute(
            r#"
            INSERT INTO temp.repository_transfer_journal (
              transaction_id, idempotency_key, request_fingerprint, repository_id,
              source_owner, source_name, destination_owner, destination_name,
              status, prepared_at, completed_at, failure, receipt_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                journal.transaction_id.to_string(),
                journal.idempotency_key,
                journal.request_fingerprint,
                journal.repository_id.to_string(),
                journal.source_owner,
                journal.source_name,
                journal.destination_owner,
                journal.destination_name,
                text(&journal.status)?,
                time(journal.prepared_at),
                optional_time(journal.completed_at),
                journal.failure,
                optional_json(&journal.receipt)?,
            ],
        )
        .map_err(storage_error)?;
    }
    for alias in state.repository_aliases.values() {
        conn.execute(
            r#"
            INSERT INTO temp.repository_aliases (
              old_owner, old_name, repository_id, canonical_owner, canonical_name,
              created_at, transaction_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                alias.owner,
                alias.name,
                alias.repository_id.to_string(),
                alias.canonical_owner,
                alias.canonical_name,
                time(alias.created_at),
                alias.transaction_id.to_string(),
            ],
        )
        .map_err(storage_error)?;
    }

    for grant in state.repo_grants.values() {
        let repo_id = repo_id(&repo_ids, &grant.owner, &grant.repo)?;
        conn.execute(
            r#"
            INSERT INTO temp.repo_access_grants (
              login, repo_id, access, granted_by, granted_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                grant.login,
                repo_id,
                text(&grant.access)?,
                grant.granted_by,
                time(grant.granted_at),
            ],
        )
        .map_err(storage_error)?;
    }

    for ((owner, repo, name), label) in &state.labels {
        let repo_id = repo_id(&repo_ids, owner, repo)?;
        conn.execute(
            "INSERT INTO temp.labels (repo_id, name, label_json) VALUES (?1, ?2, ?3)",
            params![repo_id, name, json(label)?],
        )
        .map_err(storage_error)?;
    }

    for issue in state.issues.values() {
        let repo_id = repo_id(&repo_ids, &issue.owner, &issue.repo)?;
        conn.execute(
            r#"
            INSERT INTO temp.issues (
              id, repo_id, number, title, body, state, author, labels_json,
              assignees_json, milestone, comments, pull_request_json,
              created_at, updated_at, closed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
            params![
                issue.id.to_string(),
                repo_id,
                issue.number as i64,
                issue.title,
                issue.body,
                text(&issue.state)?,
                issue.author,
                json(&issue.labels)?,
                json(&issue.assignees)?,
                issue.milestone,
                issue.comments as i64,
                optional_json(&issue.pull_request)?,
                time(issue.created_at),
                time(issue.updated_at),
                optional_time(issue.closed_at),
            ],
        )
        .map_err(storage_error)?;
    }

    for ((owner, repo, issue_number), comments) in &state.issue_comments {
        let repo_id = repo_id(&repo_ids, owner, repo)?;
        for comment in comments {
            conn.execute(
                "INSERT INTO temp.issue_comments (id, repo_id, issue_number, comment_json) VALUES (?1, ?2, ?3, ?4)",
                params![comment.id.to_string(), repo_id, *issue_number as i64, json(comment)?],
            )
            .map_err(storage_error)?;
        }
    }

    for pr in state.pulls.values() {
        let repo_id = repo_id(&repo_ids, &pr.owner, &pr.repo)?;
        conn.execute(
            r#"
            INSERT INTO temp.pull_requests (
              id, repo_id, number, issue_number, title, body, state, draft,
              author, head_json, base_json, mergeable, mergeable_state, merged,
              merged_at, merge_commit_sha, commits_json, changed_files_json,
              created_at, updated_at, source_repository
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
            "#,
            params![
                pr.id.to_string(),
                repo_id,
                pr.number as i64,
                pr.issue_number as i64,
                pr.title,
                pr.body,
                text(&pr.state)?,
                bool_int(pr.draft),
                pr.author,
                json(&pr.head)?,
                json(&pr.base)?,
                bool_int(pr.mergeable),
                pr.mergeable_state,
                bool_int(pr.merged),
                optional_time(pr.merged_at),
                pr.merge_commit_sha,
                json(&pr.commits)?,
                json(&pr.changed_files)?,
                time(pr.created_at),
                time(pr.updated_at),
                pr.source_repository,
            ],
        )
        .map_err(storage_error)?;
    }

    for reviews in state.reviews.values() {
        for review in reviews {
            let repo_id = repo_id(&repo_ids, &review.owner, &review.repo)?;
            conn.execute(
                r#"
                INSERT INTO temp.reviews (
                  id, repo_id, pull_number, author, state, body, submitted_at,
                  head_sha, dismissed_review_id
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
                params![
                    review.id.to_string(),
                    repo_id,
                    review.pull_number as i64,
                    review.author,
                    text(&review.state)?,
                    review.body,
                    time(review.submitted_at),
                    review.head_sha,
                    review.dismissed_review_id.map(|id| id.to_string()),
                ],
            )
            .map_err(storage_error)?;
        }
    }

    for ((owner, repo, pull_number), comments) in &state.review_comments {
        let repo_id = repo_id(&repo_ids, owner, repo)?;
        for comment in comments {
            conn.execute(
                r#"
                INSERT INTO temp.review_comments (
                  id, review_id, repo_id, pull_number, comment_json
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
                params![
                    comment.id.to_string(),
                    comment.review_id.to_string(),
                    repo_id,
                    *pull_number as i64,
                    json(comment)?,
                ],
            )
            .map_err(storage_error)?;
        }
    }

    for ((owner, repo, branch), rule) in &state.branch_protections {
        let repo_id = repo_id(&repo_ids, owner, repo)?;
        conn.execute(
            "INSERT INTO temp.branch_protection_rules (repo_id, branch, rule_json) VALUES (?1, ?2, ?3)",
            params![repo_id, branch, json(rule)?],
        )
        .map_err(storage_error)?;
    }

    for ((owner, repo), contents) in &state.codeowners {
        let repo_id = repo_id(&repo_ids, owner, repo)?;
        conn.execute(
            "INSERT INTO temp.codeowners (repo_id, contents) VALUES (?1, ?2)",
            params![repo_id, contents],
        )
        .map_err(storage_error)?;
    }

    for ((owner, repo), contents) in &state.readmes {
        let repo_id = repo_id(&repo_ids, owner, repo)?;
        conn.execute(
            "INSERT INTO temp.repository_readmes (repo_id, contents) VALUES (?1, ?2)",
            params![repo_id, contents],
        )
        .map_err(storage_error)?;
    }

    for statuses in state.statuses.values() {
        for status in statuses {
            let repo_id = repo_id(&repo_ids, &status.owner, &status.repo)?;
            conn.execute(
                "INSERT INTO temp.commit_statuses (id, repo_id, sha, status_json) VALUES (?1, ?2, ?3, ?4)",
                params![status.id.to_string(), repo_id, status.sha, json(status)?],
            )
            .map_err(storage_error)?;
        }
    }

    for runs in state.check_runs.values() {
        for run in runs {
            let repo_id = repo_id(&repo_ids, &run.owner, &run.repo)?;
            conn.execute(
                r#"
                INSERT INTO temp.check_runs (
                  id, repo_id, name, head_sha, status, conclusion, details_url,
                  output_json, started_at, completed_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
                params![
                    run.id.to_string(),
                    repo_id,
                    run.name,
                    run.head_sha,
                    text(&run.status)?,
                    optional_text(&run.conclusion)?,
                    run.details_url,
                    optional_json(&run.output)?,
                    time(run.started_at),
                    optional_time(run.completed_at),
                ],
            )
            .map_err(storage_error)?;
        }
    }

    for scores in state.jankurai_scores.values() {
        for score in scores {
            let repo_id = repo_id(&repo_ids, &score.owner, &score.repo)?;
            conn.execute(
                r#"
                INSERT INTO temp.jankurai_scores (
                  id, repo_id, branch, commit_sha, score, hard_findings,
                  decision, caps_json, report_json, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
                params![
                    score.id.to_string(),
                    repo_id,
                    score.branch,
                    score.commit_sha,
                    score.score,
                    score.hard_findings,
                    score.decision,
                    json(&score.caps_applied)?,
                    score.report_json,
                    time(score.created_at),
                ],
            )
            .map_err(storage_error)?;
        }
    }

    for hooks in state.webhooks.values() {
        for hook in hooks {
            let repo_id = repo_id(&repo_ids, &hook.owner, &hook.repo)?;
            conn.execute(
                r#"
                INSERT INTO temp.webhooks (
                  id, repo_id, config_json, events_json, active, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    hook.id.to_string(),
                    repo_id,
                    json(&hook.config)?,
                    json(&hook.events)?,
                    bool_int(hook.active),
                    time(hook.created_at),
                    time(hook.updated_at),
                ],
            )
            .map_err(storage_error)?;
            conn.execute(
                "INSERT INTO temp.webhook_metadata (id, name) VALUES (?1, ?2)",
                params![hook.id.to_string(), hook.name],
            )
            .map_err(storage_error)?;
        }
    }

    for delivery in &state.webhook_deliveries {
        let repo_id = repo_id(&repo_ids, &delivery.owner, &delivery.repo)?;
        conn.execute(
            r#"
            INSERT INTO temp.webhook_deliveries (
              id, hook_id, repo_id, event, target_url, payload_json,
              signature_256, delivered, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                delivery.id.to_string(),
                delivery.hook_id.to_string(),
                repo_id,
                delivery.event,
                delivery.target_url,
                json(&delivery.payload)?,
                delivery.signature_256,
                bool_int(delivery.delivered),
                time(delivery.created_at),
            ],
        )
        .map_err(storage_error)?;
    }

    for ((owner, repo), counters) in &state.counters {
        let repo_id = repo_id(&repo_ids, owner, repo)?;
        conn.execute(
            "INSERT INTO temp.repo_counters (repo_id, issue_next, pull_next) VALUES (?1, ?2, ?3)",
            params![
                repo_id,
                (counters.issue + 1) as i64,
                (counters.pull + 1) as i64
            ],
        )
        .map_err(storage_error)?;
    }

    for repo in state.repos.values() {
        if !state
            .counters
            .contains_key(&(repo.owner.clone(), repo.name.clone()))
        {
            conn.execute(
                "INSERT INTO temp.repo_counters (repo_id, issue_next, pull_next) VALUES (?1, 1, 1)",
                params![repo.id.to_string()],
            )
            .map_err(storage_error)?;
        }
    }

    Ok(())
}
