use rusqlite::Connection;

use super::super::{Counters, State};
use super::codec::*;
use super::storage_error;
use crate::errors::Result;
use crate::model::*;

pub(super) fn load_state(conn: &Connection) -> Result<State> {
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
    load_repository_creations(conn, &mut state)?;
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

fn load_repository_creations(conn: &Connection, state: &mut State) -> Result<()> {
    let mut statement = conn
        .prepare("SELECT repository_id, receipt_json FROM repository_creation_journal")
        .map_err(storage_error)?;
    let mut rows = statement.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let id: String = row.get(0).map_err(storage_error)?;
        let journal: super::super::RepositoryCreation =
            parse_json(row.get(1).map_err(storage_error)?)?;
        if journal.repository_id.is_nil() || journal.repository_id.to_string() != id {
            return Err(crate::ForgeError::Storage(
                "creation journal UUID mismatch".into(),
            ));
        }
        state
            .repository_creations
            .insert(journal.repository_id, journal);
    }
    Ok(())
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
                   v.body, v.submitted_at, v.head_sha, v.dismissed_review_id
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
            dismissed_review_id: row
                .get::<_, Option<String>>(9)
                .map_err(storage_error)?
                .map(parse_uuid)
                .transpose()?,
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

pub(super) fn backfill_missing_counters(state: &mut State) -> usize {
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
