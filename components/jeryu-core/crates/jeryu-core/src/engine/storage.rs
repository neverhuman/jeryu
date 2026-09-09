use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use super::audit::AuditEntry;
use super::{Counters, State};
use crate::errors::{ForgeError, Result};
use crate::model::*;

mod codec;
mod migrations;

use self::codec::*;
use self::migrations::apply_migrations;

#[derive(Debug, Clone)]
pub(super) struct SqliteStore {
    path: PathBuf,
}

impl SqliteStore {
    pub(super) fn open(path: impl AsRef<Path>) -> Result<(Self, State)> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(storage_error)?;
        }
        let store = Self { path };
        let conn = store.connect()?;
        apply_migrations(&conn)?;
        let mut state = load_state(&conn)?;
        let mut backfilled = backfill_missing_counters(&mut state);
        backfilled += super::backfill_default_branch_protections(&mut state);
        drop(conn);
        if backfilled > 0 {
            store.persist(&state)?;
        }
        Ok((store, state))
    }

    pub(super) fn persist(&self, state: &State) -> Result<()> {
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(storage_error)?;
        delete_all(&tx)?;
        persist_state(&tx, state)?;
        tx.commit().map_err(storage_error)?;
        Ok(())
    }

    /// Append one audit receipt through a fresh connection.
    ///
    /// `forge_audit_log` is intentionally NOT part of `persist`/`delete_all`:
    /// the full-state rewrite must never wipe the trail, so audit writes take
    /// this dedicated path instead of riding the state snapshot.
    pub(super) fn append_audit(&self, entry: &AuditEntry) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO forge_audit_log (
              id, occurred_at, actor, action, subject, phase, detail_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                entry.id,
                entry.occurred_at,
                entry.actor,
                entry.action,
                entry.subject,
                entry.phase,
                json(&entry.detail)?,
            ],
        )
        .map_err(storage_error)?;
        Ok(())
    }

    /// All audit entries for one subject, oldest first.
    pub(super) fn list_audit(&self, subject: &str) -> Result<Vec<AuditEntry>> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, occurred_at, actor, action, subject, phase, detail_json
                FROM forge_audit_log
                WHERE subject = ?1
                ORDER BY occurred_at, rowid
                "#,
            )
            .map_err(storage_error)?;
        let mut rows = stmt.query(params![subject]).map_err(storage_error)?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next().map_err(storage_error)? {
            entries.push(AuditEntry {
                id: row.get(0).map_err(storage_error)?,
                occurred_at: row.get(1).map_err(storage_error)?,
                actor: row.get(2).map_err(storage_error)?,
                action: row.get(3).map_err(storage_error)?,
                subject: row.get(4).map_err(storage_error)?,
                phase: row.get(5).map_err(storage_error)?,
                detail: parse_json(row.get(6).map_err(storage_error)?)?,
            });
        }
        Ok(entries)
    }

    fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path).map_err(storage_error)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(storage_error)?;
        Ok(conn)
    }
}

fn delete_all(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        DELETE FROM review_comments;
        DELETE FROM issue_comments;
        DELETE FROM commit_statuses;
        DELETE FROM repository_aliases;
        DELETE FROM repository_transfer_journal;
        DELETE FROM codeowners;
        DELETE FROM repository_readmes;
        DELETE FROM labels;
        DELETE FROM webhook_deliveries;
        DELETE FROM webhook_metadata;
        DELETE FROM webhooks;
        DELETE FROM branch_protection_rules;
        DELETE FROM jankurai_scores;
        DELETE FROM check_runs;
        DELETE FROM reviews;
        DELETE FROM pull_requests;
        DELETE FROM issues;
        DELETE FROM repo_access_grants;
        DELETE FROM repo_counters;
        DELETE FROM repositories;
        DELETE FROM teams;
        DELETE FROM organizations;
        DELETE FROM account_activation_challenges;
        DELETE FROM account_invitations;
        DELETE FROM owner_bootstrap_state;
        DELETE FROM personal_access_tokens;
        DELETE FROM web_sessions;
        DELETE FROM user_accounts;
        DELETE FROM users;
        "#,
    )
    .map_err(storage_error)?;
    Ok(())
}

fn persist_state(conn: &Connection, state: &State) -> Result<()> {
    for user in state.users.values() {
        conn.execute(
            "INSERT INTO users (login, user_json) VALUES (?1, ?2)",
            params![user.login, json(user)?],
        )
        .map_err(storage_error)?;
    }
    for account in state.accounts.values() {
        conn.execute(
            r#"
            INSERT INTO user_accounts (
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
            INSERT INTO web_sessions (
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
            INSERT INTO personal_access_tokens (
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
            INSERT INTO account_invitations (
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
            INSERT INTO account_activation_challenges (
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
        "INSERT INTO owner_bootstrap_state (singleton, consumed) VALUES (1, ?1)",
        params![bool_int(state.bootstrap_owner_consumed)],
    )
    .map_err(storage_error)?;
    for organization in state.organizations.values() {
        conn.execute(
            "INSERT INTO organizations (login, organization_json) VALUES (?1, ?2)",
            params![organization.login, json(organization)?],
        )
        .map_err(storage_error)?;
    }
    for ((organization, slug), team) in &state.teams {
        conn.execute(
            "INSERT INTO teams (organization, slug, team_json) VALUES (?1, ?2, ?3)",
            params![organization, slug, json(team)?],
        )
        .map_err(storage_error)?;
    }

    let mut repo_ids = HashMap::new();
    for ((owner, name), repo) in &state.repos {
        repo_ids.insert((owner.clone(), name.clone()), repo.id.to_string());
        conn.execute(
            r#"
            INSERT INTO repositories (
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
            INSERT INTO repository_transfer_journal (
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
            INSERT INTO repository_aliases (
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
            INSERT INTO repo_access_grants (
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
            "INSERT INTO labels (repo_id, name, label_json) VALUES (?1, ?2, ?3)",
            params![repo_id, name, json(label)?],
        )
        .map_err(storage_error)?;
    }

    for issue in state.issues.values() {
        let repo_id = repo_id(&repo_ids, &issue.owner, &issue.repo)?;
        conn.execute(
            r#"
            INSERT INTO issues (
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
                "INSERT INTO issue_comments (id, repo_id, issue_number, comment_json) VALUES (?1, ?2, ?3, ?4)",
                params![comment.id.to_string(), repo_id, *issue_number as i64, json(comment)?],
            )
            .map_err(storage_error)?;
        }
    }

    for pr in state.pulls.values() {
        let repo_id = repo_id(&repo_ids, &pr.owner, &pr.repo)?;
        conn.execute(
            r#"
            INSERT INTO pull_requests (
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
                INSERT INTO reviews (
                  id, repo_id, pull_number, author, state, body, submitted_at,
                  head_sha
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
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
                INSERT INTO review_comments (
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
            "INSERT INTO branch_protection_rules (repo_id, branch, rule_json) VALUES (?1, ?2, ?3)",
            params![repo_id, branch, json(rule)?],
        )
        .map_err(storage_error)?;
    }

    for ((owner, repo), contents) in &state.codeowners {
        let repo_id = repo_id(&repo_ids, owner, repo)?;
        conn.execute(
            "INSERT INTO codeowners (repo_id, contents) VALUES (?1, ?2)",
            params![repo_id, contents],
        )
        .map_err(storage_error)?;
    }

    for ((owner, repo), contents) in &state.readmes {
        let repo_id = repo_id(&repo_ids, owner, repo)?;
        conn.execute(
            "INSERT INTO repository_readmes (repo_id, contents) VALUES (?1, ?2)",
            params![repo_id, contents],
        )
        .map_err(storage_error)?;
    }

    for statuses in state.statuses.values() {
        for status in statuses {
            let repo_id = repo_id(&repo_ids, &status.owner, &status.repo)?;
            conn.execute(
                "INSERT INTO commit_statuses (id, repo_id, sha, status_json) VALUES (?1, ?2, ?3, ?4)",
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
                INSERT INTO check_runs (
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
                INSERT INTO jankurai_scores (
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
                INSERT INTO webhooks (
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
                "INSERT INTO webhook_metadata (id, name) VALUES (?1, ?2)",
                params![hook.id.to_string(), hook.name],
            )
            .map_err(storage_error)?;
        }
    }

    for delivery in &state.webhook_deliveries {
        let repo_id = repo_id(&repo_ids, &delivery.owner, &delivery.repo)?;
        conn.execute(
            r#"
            INSERT INTO webhook_deliveries (
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
            "INSERT INTO repo_counters (repo_id, issue_next, pull_next) VALUES (?1, ?2, ?3)",
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
                "INSERT INTO repo_counters (repo_id, issue_next, pull_next) VALUES (?1, 1, 1)",
                params![repo.id.to_string()],
            )
            .map_err(storage_error)?;
        }
    }

    Ok(())
}

fn load_state(conn: &Connection) -> Result<State> {
    let mut state = State::default();
    load_users(conn, &mut state)?;
    load_accounts(conn, &mut state)?;
    load_sessions(conn, &mut state)?;
    load_personal_tokens(conn, &mut state)?;
    load_invitations(conn, &mut state)?;
    load_activation_challenges(conn, &mut state)?;
    load_bootstrap_state(conn, &mut state)?;
    load_organizations(conn, &mut state)?;
    load_teams(conn, &mut state)?;
    load_repositories(conn, &mut state)?;
    load_repository_transfers(conn, &mut state)?;
    load_repository_aliases(conn, &mut state)?;
    load_repo_grants(conn, &mut state)?;
    load_labels(conn, &mut state)?;
    load_issues(conn, &mut state)?;
    load_issue_comments(conn, &mut state)?;
    load_pull_requests(conn, &mut state)?;
    load_reviews(conn, &mut state)?;
    load_review_comments(conn, &mut state)?;
    load_branch_protection(conn, &mut state)?;
    load_codeowners(conn, &mut state)?;
    load_readmes(conn, &mut state)?;
    load_commit_statuses(conn, &mut state)?;
    load_check_runs(conn, &mut state)?;
    load_jankurai_scores(conn, &mut state)?;
    load_webhooks(conn, &mut state)?;
    load_webhook_deliveries(conn, &mut state)?;
    load_counters(conn, &mut state)?;
    Ok(state)
}

fn load_users(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT user_json FROM users")
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let user: User = parse_json(row.get(0).map_err(storage_error)?)?;
        state.users.insert(user.login.clone(), user);
    }
    Ok(())
}

fn load_accounts(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            "SELECT login, display_name, password_hash, role, status, auth_epoch, must_change_password, created_at, updated_at FROM user_accounts",
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let account = UserAccount {
            canonical_login: row.get(0).map_err(storage_error)?,
            display_name: row.get(1).map_err(storage_error)?,
            password_hash: row.get(2).map_err(storage_error)?,
            role: from_text(row.get(3).map_err(storage_error)?)?,
            status: from_text(row.get(4).map_err(storage_error)?)?,
            auth_epoch: row.get(5).map_err(storage_error)?,
            must_change_password: int_bool(row.get(6).map_err(storage_error)?),
            created_at: parse_time(row.get(7).map_err(storage_error)?)?,
            updated_at: parse_time(row.get(8).map_err(storage_error)?)?,
        };
        state
            .accounts
            .insert(account.canonical_login.clone(), account);
    }
    Ok(())
}

fn load_sessions(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            "SELECT id, login, auth_epoch, token_hash, csrf_token, created_at, expires_at FROM web_sessions",
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let session = WebSession {
            id: parse_uuid(row.get(0).map_err(storage_error)?)?,
            login: row.get(1).map_err(storage_error)?,
            auth_epoch: row.get(2).map_err(storage_error)?,
            token_hash: row.get(3).map_err(storage_error)?,
            csrf_token: row.get(4).map_err(storage_error)?,
            created_at: parse_time(row.get(5).map_err(storage_error)?)?,
            expires_at: parse_time(row.get(6).map_err(storage_error)?)?,
        };
        state.sessions.insert(session.token_hash.clone(), session);
    }
    Ok(())
}

fn load_personal_tokens(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            "SELECT id, login, auth_epoch, name, token_hash, created_at, expires_at FROM personal_access_tokens",
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let token = PersonalAccessToken {
            id: parse_uuid(row.get(0).map_err(storage_error)?)?,
            login: row.get(1).map_err(storage_error)?,
            auth_epoch: row.get(2).map_err(storage_error)?,
            name: row.get(3).map_err(storage_error)?,
            token_hash: row.get(4).map_err(storage_error)?,
            created_at: parse_time(row.get(5).map_err(storage_error)?)?,
            expires_at: parse_optional_time(row.get(6).map_err(storage_error)?)?,
        };
        state.personal_tokens.insert(token.id, token);
    }
    Ok(())
}

fn load_invitations(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            "SELECT id, canonical_login, display_name, activation_secret_hash, issuer_principal, intended_role, intended_teams_json, created_at, expires_at, consumed_at, revoked_at, attempt_count, bootstrap_owner FROM account_invitations",
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let invitation = AccountInvitation {
            id: parse_uuid(row.get(0).map_err(storage_error)?)?,
            canonical_login: row.get(1).map_err(storage_error)?,
            display_name: row.get(2).map_err(storage_error)?,
            activation_secret_hash: row.get(3).map_err(storage_error)?,
            issuer_principal: row.get(4).map_err(storage_error)?,
            intended_bindings: InvitationBindings {
                role: from_text(row.get(5).map_err(storage_error)?)?,
                teams: parse_json(row.get(6).map_err(storage_error)?)?,
            },
            created_at: parse_time(row.get(7).map_err(storage_error)?)?,
            expires_at: parse_time(row.get(8).map_err(storage_error)?)?,
            consumed_at: parse_optional_time(row.get(9).map_err(storage_error)?)?,
            revoked_at: parse_optional_time(row.get(10).map_err(storage_error)?)?,
            attempt_count: row.get(11).map_err(storage_error)?,
            bootstrap_owner: int_bool(row.get(12).map_err(storage_error)?),
        };
        state.invitations.insert(invitation.id, invitation);
    }
    Ok(())
}

fn load_activation_challenges(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            "SELECT id, invitation_id, challenge_hash, created_at, expires_at, consumed_at FROM account_activation_challenges",
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let challenge = ActivationChallenge {
            id: parse_uuid(row.get(0).map_err(storage_error)?)?,
            invitation_id: parse_uuid(row.get(1).map_err(storage_error)?)?,
            challenge_hash: row.get(2).map_err(storage_error)?,
            created_at: parse_time(row.get(3).map_err(storage_error)?)?,
            expires_at: parse_time(row.get(4).map_err(storage_error)?)?,
            consumed_at: parse_optional_time(row.get(5).map_err(storage_error)?)?,
        };
        state
            .activation_challenges
            .insert(challenge.challenge_hash.clone(), challenge);
    }
    Ok(())
}

fn load_bootstrap_state(conn: &Connection, state: &mut State) -> Result<()> {
    state.bootstrap_owner_consumed = conn
        .query_row(
            "SELECT consumed FROM owner_bootstrap_state WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(int_bool)
        .map_err(storage_error)?;
    Ok(())
}

fn load_organizations(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT organization_json FROM organizations")
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let organization: Organization = parse_json(row.get(0).map_err(storage_error)?)?;
        state
            .organizations
            .insert(organization.login.clone(), organization);
    }
    Ok(())
}

fn load_teams(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT team_json FROM teams")
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let team: Team = parse_json(row.get(0).map_err(storage_error)?)?;
        state
            .teams
            .insert((team.organization.clone(), team.slug.clone()), team);
    }
    Ok(())
}

fn load_repositories(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, owner, name, full_name, private, description, default_branch,
                   archived, disabled, created_at, updated_at, family
            FROM repositories
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let repo = Repository {
            id: parse_uuid(row.get(0).map_err(storage_error)?)?,
            owner: row.get(1).map_err(storage_error)?,
            name: row.get(2).map_err(storage_error)?,
            full_name: row.get(3).map_err(storage_error)?,
            private: int_bool(row.get(4).map_err(storage_error)?),
            description: row.get(5).map_err(storage_error)?,
            default_branch: row.get(6).map_err(storage_error)?,
            family: row.get(11).map_err(storage_error)?,
            archived: int_bool(row.get(7).map_err(storage_error)?),
            disabled: int_bool(row.get(8).map_err(storage_error)?),
            created_at: parse_time(row.get(9).map_err(storage_error)?)?,
            updated_at: parse_time(row.get(10).map_err(storage_error)?)?,
        };
        state
            .repos
            .insert((repo.owner.clone(), repo.name.clone()), repo);
    }
    Ok(())
}

fn load_repository_transfers(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT transaction_id, idempotency_key, request_fingerprint, repository_id,
                   source_owner, source_name, destination_owner, destination_name,
                   status, prepared_at, completed_at, failure, receipt_json
            FROM repository_transfer_journal
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let journal = RepositoryTransferJournal {
            transaction_id: parse_uuid(row.get(0).map_err(storage_error)?)?,
            idempotency_key: row.get(1).map_err(storage_error)?,
            request_fingerprint: row.get(2).map_err(storage_error)?,
            repository_id: parse_uuid(row.get(3).map_err(storage_error)?)?,
            source_owner: row.get(4).map_err(storage_error)?,
            source_name: row.get(5).map_err(storage_error)?,
            destination_owner: row.get(6).map_err(storage_error)?,
            destination_name: row.get(7).map_err(storage_error)?,
            status: from_text(row.get(8).map_err(storage_error)?)?,
            prepared_at: parse_time(row.get(9).map_err(storage_error)?)?,
            completed_at: parse_optional_time(row.get(10).map_err(storage_error)?)?,
            failure: row.get(11).map_err(storage_error)?,
            receipt: parse_optional_json(row.get(12).map_err(storage_error)?)?,
        };
        state
            .repository_transfers
            .insert(journal.idempotency_key.clone(), journal);
    }
    Ok(())
}

fn load_repository_aliases(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT old_owner, old_name, repository_id, canonical_owner, canonical_name,
                   created_at, transaction_id
            FROM repository_aliases
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let alias = RepositoryAlias {
            owner: row.get(0).map_err(storage_error)?,
            name: row.get(1).map_err(storage_error)?,
            repository_id: parse_uuid(row.get(2).map_err(storage_error)?)?,
            canonical_owner: row.get(3).map_err(storage_error)?,
            canonical_name: row.get(4).map_err(storage_error)?,
            created_at: parse_time(row.get(5).map_err(storage_error)?)?,
            transaction_id: parse_uuid(row.get(6).map_err(storage_error)?)?,
        };
        state
            .repository_aliases
            .insert((alias.owner.clone(), alias.name.clone()), alias);
    }
    Ok(())
}

fn load_repo_grants(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT g.login, r.owner, r.name, g.access, g.granted_by, g.granted_at
            FROM repo_access_grants g
            JOIN repositories r ON r.id = g.repo_id
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let grant = RepoAccessGrant {
            login: row.get(0).map_err(storage_error)?,
            owner: row.get(1).map_err(storage_error)?,
            repo: row.get(2).map_err(storage_error)?,
            access: from_text(row.get(3).map_err(storage_error)?)?,
            granted_by: row.get(4).map_err(storage_error)?,
            granted_at: parse_time(row.get(5).map_err(storage_error)?)?,
        };
        state.repo_grants.insert(
            (grant.login.clone(), grant.owner.clone(), grant.repo.clone()),
            grant,
        );
    }
    Ok(())
}

fn load_labels(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, l.name, l.label_json
            FROM labels l
            JOIN repositories r ON r.id = l.repo_id
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let name: String = row.get(2).map_err(storage_error)?;
        let label: Label = parse_json(row.get(3).map_err(storage_error)?)?;
        state.labels.insert((owner, repo, name), label);
    }
    Ok(())
}

fn load_issues(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, i.id, i.number, i.title, i.body, i.state,
                   i.author, i.labels_json, i.assignees_json, i.milestone,
                   i.comments, i.pull_request_json, i.created_at, i.updated_at,
                   i.closed_at
            FROM issues i
            JOIN repositories r ON r.id = i.repo_id
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let number: i64 = row.get(3).map_err(storage_error)?;
        let issue = Issue {
            id: parse_uuid(row.get(2).map_err(storage_error)?)?,
            owner: owner.clone(),
            repo: repo.clone(),
            number: number as u64,
            title: row.get(4).map_err(storage_error)?,
            body: row.get(5).map_err(storage_error)?,
            state: from_text(row.get(6).map_err(storage_error)?)?,
            author: row.get(7).map_err(storage_error)?,
            labels: parse_json(row.get(8).map_err(storage_error)?)?,
            assignees: parse_json(row.get(9).map_err(storage_error)?)?,
            milestone: row.get(10).map_err(storage_error)?,
            comments: row.get::<_, i64>(11).map_err(storage_error)? as u64,
            pull_request: parse_optional_json(row.get(12).map_err(storage_error)?)?,
            created_at: parse_time(row.get(13).map_err(storage_error)?)?,
            updated_at: parse_time(row.get(14).map_err(storage_error)?)?,
            closed_at: parse_optional_time(row.get(15).map_err(storage_error)?)?,
        };
        state.issues.insert((owner, repo, issue.number), issue);
    }
    Ok(())
}

fn load_issue_comments(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, ic.issue_number, ic.comment_json
            FROM issue_comments ic
            JOIN repositories r ON r.id = ic.repo_id
            ORDER BY ic.rowid
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let issue_number: i64 = row.get(2).map_err(storage_error)?;
        let comment: IssueComment = parse_json(row.get(3).map_err(storage_error)?)?;
        state
            .issue_comments
            .entry((owner, repo, issue_number as u64))
            .or_default()
            .push(comment);
    }
    Ok(())
}

fn load_pull_requests(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, p.id, p.number, p.issue_number, p.title, p.body,
                   p.state, p.draft, p.author, p.head_json, p.base_json,
                   p.mergeable, p.mergeable_state, p.merged, p.merged_at,
                   p.merge_commit_sha, p.commits_json, p.changed_files_json,
                   p.created_at, p.updated_at, p.source_repository
            FROM pull_requests p
            JOIN repositories r ON r.id = p.repo_id
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let number: i64 = row.get(3).map_err(storage_error)?;
        let pr = PullRequest {
            id: parse_uuid(row.get(2).map_err(storage_error)?)?,
            owner: owner.clone(),
            repo: repo.clone(),
            number: number as u64,
            issue_number: row.get::<_, i64>(4).map_err(storage_error)? as u64,
            title: row.get(5).map_err(storage_error)?,
            body: row.get(6).map_err(storage_error)?,
            state: from_text(row.get(7).map_err(storage_error)?)?,
            draft: int_bool(row.get(8).map_err(storage_error)?),
            author: row.get(9).map_err(storage_error)?,
            head: parse_json(row.get(10).map_err(storage_error)?)?,
            base: parse_json(row.get(11).map_err(storage_error)?)?,
            mergeable: int_bool(row.get(12).map_err(storage_error)?),
            mergeable_state: row.get(13).map_err(storage_error)?,
            merged: int_bool(row.get(14).map_err(storage_error)?),
            merged_at: parse_optional_time(row.get(15).map_err(storage_error)?)?,
            merge_commit_sha: row.get(16).map_err(storage_error)?,
            commits: parse_json(row.get(17).map_err(storage_error)?)?,
            changed_files: parse_json(row.get(18).map_err(storage_error)?)?,
            created_at: parse_time(row.get(19).map_err(storage_error)?)?,
            updated_at: parse_time(row.get(20).map_err(storage_error)?)?,
            source_repository: {
                let source_repository: String = row.get(21).map_err(storage_error)?;
                if source_repository.trim().is_empty() {
                    format!("{owner}/{repo}")
                } else {
                    source_repository
                }
            },
        };
        state.pulls.insert((owner, repo, pr.number), pr);
    }
    Ok(())
}

fn load_reviews(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, v.id, v.pull_number, v.author, v.state,
                   v.body, v.submitted_at, v.head_sha
            FROM reviews v
            JOIN repositories r ON r.id = v.repo_id
            ORDER BY v.rowid
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let pull_number: i64 = row.get(3).map_err(storage_error)?;
        let review = Review {
            id: parse_uuid(row.get(2).map_err(storage_error)?)?,
            owner: owner.clone(),
            repo: repo.clone(),
            pull_number: pull_number as u64,
            author: row.get(4).map_err(storage_error)?,
            state: from_text(row.get(5).map_err(storage_error)?)?,
            body: row.get(6).map_err(storage_error)?,
            head_sha: row.get(8).map_err(storage_error)?,
            submitted_at: parse_time(row.get(7).map_err(storage_error)?)?,
        };
        state
            .reviews
            .entry((owner, repo, review.pull_number))
            .or_default()
            .push(review);
    }
    Ok(())
}

fn load_review_comments(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, rc.pull_number, rc.comment_json
            FROM review_comments rc
            JOIN repositories r ON r.id = rc.repo_id
            ORDER BY rc.rowid
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let pull_number: i64 = row.get(2).map_err(storage_error)?;
        let comment: ReviewComment = parse_json(row.get(3).map_err(storage_error)?)?;
        state
            .review_comments
            .entry((owner, repo, pull_number as u64))
            .or_default()
            .push(comment);
    }
    Ok(())
}

fn load_branch_protection(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, b.branch, b.rule_json
            FROM branch_protection_rules b
            JOIN repositories r ON r.id = b.repo_id
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let branch: String = row.get(2).map_err(storage_error)?;
        let rule: BranchProtectionRule = parse_json(row.get(3).map_err(storage_error)?)?;
        state.branch_protections.insert((owner, repo, branch), rule);
    }
    Ok(())
}

fn load_codeowners(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, c.contents
            FROM codeowners c
            JOIN repositories r ON r.id = c.repo_id
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let contents: String = row.get(2).map_err(storage_error)?;
        state.codeowners.insert((owner, repo), contents);
    }
    Ok(())
}

fn load_readmes(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, rr.contents
            FROM repository_readmes rr
            JOIN repositories r ON r.id = rr.repo_id
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let contents: String = row.get(2).map_err(storage_error)?;
        state.readmes.insert((owner, repo), contents);
    }
    Ok(())
}

fn load_commit_statuses(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, cs.sha, cs.status_json
            FROM commit_statuses cs
            JOIN repositories r ON r.id = cs.repo_id
            ORDER BY cs.rowid
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let sha: String = row.get(2).map_err(storage_error)?;
        let status: CommitStatus = parse_json(row.get(3).map_err(storage_error)?)?;
        state
            .statuses
            .entry((owner, repo, sha))
            .or_default()
            .push(status);
    }
    Ok(())
}

fn load_check_runs(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, c.id, c.name, c.head_sha, c.status,
                   c.conclusion, c.details_url, c.output_json, c.started_at,
                   c.completed_at
            FROM check_runs c
            JOIN repositories r ON r.id = c.repo_id
            ORDER BY c.rowid
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let run = CheckRun {
            id: parse_uuid(row.get(2).map_err(storage_error)?)?,
            owner: owner.clone(),
            repo: repo.clone(),
            name: row.get(3).map_err(storage_error)?,
            head_sha: row.get(4).map_err(storage_error)?,
            status: from_text(row.get(5).map_err(storage_error)?)?,
            conclusion: from_optional_text(row.get(6).map_err(storage_error)?)?,
            details_url: row.get(7).map_err(storage_error)?,
            output: parse_optional_json(row.get(8).map_err(storage_error)?)?,
            started_at: parse_time(row.get(9).map_err(storage_error)?)?,
            completed_at: parse_optional_time(row.get(10).map_err(storage_error)?)?,
        };
        state.check_runs.entry((owner, repo)).or_default().push(run);
    }
    Ok(())
}

fn load_jankurai_scores(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, s.id, s.branch, s.commit_sha, s.score,
                   s.hard_findings, s.decision, s.caps_json, s.report_json,
                   s.created_at
            FROM jankurai_scores s
            JOIN repositories r ON r.id = s.repo_id
            ORDER BY s.rowid
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let score = JankuraiScore {
            id: parse_uuid(row.get(2).map_err(storage_error)?)?,
            owner: owner.clone(),
            repo: repo.clone(),
            branch: row.get(3).map_err(storage_error)?,
            commit_sha: row.get(4).map_err(storage_error)?,
            score: row.get(5).map_err(storage_error)?,
            hard_findings: row.get::<_, i64>(6).map_err(storage_error)? as u32,
            decision: row.get(7).map_err(storage_error)?,
            caps_applied: parse_json(row.get(8).map_err(storage_error)?)?,
            report_json: row.get(9).map_err(storage_error)?,
            created_at: parse_time(row.get(10).map_err(storage_error)?)?,
        };
        state
            .jankurai_scores
            .entry((owner, repo))
            .or_default()
            .push(score);
    }
    Ok(())
}

fn load_webhooks(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, w.id, COALESCE(m.name, 'web'), w.active,
                   w.events_json, w.config_json, w.created_at, w.updated_at
            FROM webhooks w
            JOIN repositories r ON r.id = w.repo_id
            LEFT JOIN webhook_metadata m ON m.id = w.id
            ORDER BY w.rowid
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let hook = Webhook {
            id: parse_uuid(row.get(2).map_err(storage_error)?)?,
            owner: owner.clone(),
            repo: repo.clone(),
            name: row.get(3).map_err(storage_error)?,
            active: int_bool(row.get(4).map_err(storage_error)?),
            events: parse_json(row.get(5).map_err(storage_error)?)?,
            config: parse_json(row.get(6).map_err(storage_error)?)?,
            created_at: parse_time(row.get(7).map_err(storage_error)?)?,
            updated_at: parse_time(row.get(8).map_err(storage_error)?)?,
        };
        state.webhooks.entry((owner, repo)).or_default().push(hook);
    }
    Ok(())
}

fn load_webhook_deliveries(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, d.id, d.hook_id, d.event, d.target_url,
                   d.payload_json, d.signature_256, d.delivered, d.created_at
            FROM webhook_deliveries d
            JOIN repositories r ON r.id = d.repo_id
            ORDER BY d.rowid
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let delivery = WebhookDelivery {
            owner: row.get(0).map_err(storage_error)?,
            repo: row.get(1).map_err(storage_error)?,
            id: parse_uuid(row.get(2).map_err(storage_error)?)?,
            hook_id: parse_uuid(row.get(3).map_err(storage_error)?)?,
            event: row.get(4).map_err(storage_error)?,
            target_url: row.get(5).map_err(storage_error)?,
            payload: parse_json(row.get(6).map_err(storage_error)?)?,
            signature_256: row.get(7).map_err(storage_error)?,
            delivered: int_bool(row.get(8).map_err(storage_error)?),
            created_at: parse_time(row.get(9).map_err(storage_error)?)?,
        };
        state.webhook_deliveries.push(delivery);
    }
    Ok(())
}

fn load_counters(conn: &Connection, state: &mut State) -> Result<()> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT r.owner, r.name, c.issue_next, c.pull_next
            FROM repo_counters c
            JOIN repositories r ON r.id = c.repo_id
            "#,
        )
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let owner: String = row.get(0).map_err(storage_error)?;
        let repo: String = row.get(1).map_err(storage_error)?;
        let issue_next = row.get::<_, i64>(2).map_err(storage_error)?.max(1) as u64;
        let pull_next = row.get::<_, i64>(3).map_err(storage_error)?.max(1) as u64;
        state.counters.insert(
            (owner, repo),
            Counters {
                issue: issue_next - 1,
                pull: pull_next - 1,
            },
        );
    }
    Ok(())
}

fn backfill_missing_counters(state: &mut State) -> usize {
    let repos: Vec<_> = state
        .repos
        .values()
        .map(|repo| (repo.owner.clone(), repo.name.clone()))
        .collect();
    let mut inserted = 0;
    for (owner, repo) in repos {
        let key = (owner.clone(), repo.clone());
        if state.counters.contains_key(&key) {
            continue;
        }
        state.counters.insert(
            key,
            Counters {
                issue: state
                    .issues
                    .keys()
                    .filter(|(issue_owner, issue_repo, _)| {
                        issue_owner == &owner && issue_repo == &repo
                    })
                    .map(|(_, _, number)| *number)
                    .max()
                    .unwrap_or(0),
                pull: state
                    .pulls
                    .keys()
                    .filter(|(pull_owner, pull_repo, _)| pull_owner == &owner && pull_repo == &repo)
                    .map(|(_, _, number)| *number)
                    .max()
                    .unwrap_or(0),
            },
        );
        inserted += 1;
    }
    inserted
}

fn repo_id(
    repo_ids: &HashMap<(String, String), String>,
    owner: &str,
    repo: &str,
) -> Result<String> {
    repo_ids
        .get(&(owner.to_string(), repo.to_string()))
        .cloned()
        .ok_or_else(|| ForgeError::Storage(format!("missing repository row for {owner}/{repo}")))
}

fn storage_error(error: impl std::fmt::Display) -> ForgeError {
    ForgeError::Storage(error.to_string())
}
