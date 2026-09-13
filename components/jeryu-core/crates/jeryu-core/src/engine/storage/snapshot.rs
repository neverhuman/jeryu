//! Reconcile only the rows and columns owned by Core's in-memory State.
//! Staging is connection-local; durable parent rows keep their primary keys and
//! rowids. Independent CI/recovery/audit tables are outside this ownership list.

use rusqlite::Connection;

use super::{Result, storage_error};

struct OwnedTable {
    name: &'static str,
    columns: &'static str,
    primary_key: &'static str,
}

// Parent-before-child order. Deletion traverses this same list in reverse.
// These identifiers are compile-time constants, never catalog or request input.
// Explicit columns ensure a future independent column is neither staged nor
// overwritten. New rows receive that column's database default.
const OWNED_TABLES: &[OwnedTable] = &[
    OwnedTable {
        name: "users",
        columns: "login,user_json",
        primary_key: "login",
    },
    OwnedTable {
        name: "user_accounts",
        columns: "login,display_name,password_hash,role,status,auth_epoch,must_change_password,created_at,updated_at",
        primary_key: "login",
    },
    OwnedTable {
        name: "web_sessions",
        columns: "id,login,auth_epoch,token_hash,csrf_token,created_at,expires_at",
        primary_key: "id",
    },
    OwnedTable {
        name: "personal_access_tokens",
        columns: "id,login,auth_epoch,name,token_hash,created_at,expires_at",
        primary_key: "id",
    },
    OwnedTable {
        name: "account_invitations",
        columns: "id,canonical_login,display_name,activation_secret_hash,issuer_principal,intended_role,intended_teams_json,created_at,expires_at,consumed_at,revoked_at,attempt_count,bootstrap_owner",
        primary_key: "id",
    },
    OwnedTable {
        name: "account_activation_challenges",
        columns: "id,invitation_id,challenge_hash,created_at,expires_at,consumed_at",
        primary_key: "id",
    },
    OwnedTable {
        name: "owner_bootstrap_state",
        columns: "singleton,consumed",
        primary_key: "singleton",
    },
    OwnedTable {
        name: "organizations",
        columns: "login,organization_json",
        primary_key: "login",
    },
    OwnedTable {
        name: "teams",
        columns: "organization,slug,team_json",
        primary_key: "organization,slug",
    },
    OwnedTable {
        name: "repositories",
        columns: "id,owner,name,full_name,private,description,default_branch,archived,disabled,created_at,updated_at,family",
        primary_key: "id",
    },
    OwnedTable {
        name: "repository_creation_journal",
        columns: "repository_id,receipt_json",
        primary_key: "repository_id",
    },
    OwnedTable {
        name: "repository_mutation_blocks",
        columns: "repo_id,block_json",
        primary_key: "repo_id",
    },
    OwnedTable {
        name: "repository_transfer_journal",
        columns: "transaction_id,idempotency_key,request_fingerprint,repository_id,source_owner,source_name,destination_owner,destination_name,status,prepared_at,completed_at,failure,receipt_json",
        primary_key: "transaction_id",
    },
    OwnedTable {
        name: "repository_aliases",
        columns: "old_owner,old_name,repository_id,canonical_owner,canonical_name,created_at,transaction_id",
        primary_key: "old_owner,old_name",
    },
    OwnedTable {
        name: "repo_access_grants",
        columns: "login,repo_id,access,granted_by,granted_at",
        primary_key: "login,repo_id",
    },
    OwnedTable {
        name: "labels",
        columns: "repo_id,name,label_json",
        primary_key: "repo_id,name",
    },
    OwnedTable {
        name: "issues",
        columns: "id,repo_id,number,title,body,state,author,labels_json,assignees_json,milestone,comments,pull_request_json,created_at,updated_at,closed_at",
        primary_key: "id",
    },
    OwnedTable {
        name: "issue_comments",
        columns: "id,repo_id,issue_number,comment_json",
        primary_key: "id",
    },
    OwnedTable {
        name: "pull_requests",
        columns: "id,repo_id,number,issue_number,title,body,state,draft,author,head_json,base_json,mergeable,mergeable_state,merged,merged_at,merge_commit_sha,commits_json,changed_files_json,created_at,updated_at,source_repository",
        primary_key: "id",
    },
    OwnedTable {
        name: "reviews",
        columns: "id,repo_id,pull_number,author,state,body,submitted_at,head_sha,dismissed_review_id",
        primary_key: "id",
    },
    OwnedTable {
        name: "review_comments",
        columns: "id,review_id,repo_id,pull_number,comment_json",
        primary_key: "id",
    },
    OwnedTable {
        name: "branch_protection_rules",
        columns: "repo_id,branch,rule_json",
        primary_key: "repo_id,branch",
    },
    OwnedTable {
        name: "codeowners",
        columns: "repo_id,contents",
        primary_key: "repo_id",
    },
    OwnedTable {
        name: "repository_readmes",
        columns: "repo_id,contents",
        primary_key: "repo_id",
    },
    OwnedTable {
        name: "commit_statuses",
        columns: "id,repo_id,sha,status_json",
        primary_key: "id",
    },
    OwnedTable {
        name: "check_runs",
        columns: "id,repo_id,name,head_sha,status,conclusion,details_url,output_json,started_at,completed_at",
        primary_key: "id",
    },
    OwnedTable {
        name: "jankurai_scores",
        columns: "id,repo_id,branch,commit_sha,score,hard_findings,decision,caps_json,report_json,created_at",
        primary_key: "id",
    },
    OwnedTable {
        name: "webhooks",
        columns: "id,repo_id,config_json,events_json,active,created_at,updated_at",
        primary_key: "id",
    },
    OwnedTable {
        name: "webhook_metadata",
        columns: "id,name",
        primary_key: "id",
    },
    OwnedTable {
        name: "webhook_deliveries",
        columns: "id,hook_id,repo_id,event,target_url,payload_json,signature_256,delivered,created_at",
        primary_key: "id",
    },
    OwnedTable {
        name: "repo_counters",
        columns: "repo_id,issue_next,pull_next",
        primary_key: "repo_id",
    },
];

fn identifiers(names: &str) -> String {
    names
        .split(',')
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn create_tables(conn: &Connection) -> Result<()> {
    for table in OWNED_TABLES {
        let name = table.name;
        let columns = identifiers(table.columns);
        let keys = identifiers(table.primary_key);
        // TEMP tables have no inherited main-schema triggers or foreign keys.
        // Their explicit unique key rejects inconsistent duplicate State rows.
        conn.execute_batch(&format!(
            "CREATE TEMP TABLE \"{name}\" AS
                 SELECT {columns} FROM main.\"{name}\" WHERE 0;
             CREATE UNIQUE INDEX temp.\"snapshot_key_{name}\"
                 ON \"{name}\" ({keys});"
        ))
        .map_err(storage_error)?;
    }
    Ok(())
}

pub(super) fn apply(conn: &Connection) -> Result<()> {
    for table in OWNED_TABLES.iter().rev() {
        let name = table.name;
        let same_key = table
            .primary_key
            .split(',')
            .map(|key| format!("owned.\"{key}\" = desired.\"{key}\""))
            .collect::<Vec<_>>()
            .join(" AND ");
        // Only explicit absence from State permits deletion. Stable parent rows
        // never pass this predicate, so their independent FK children survive.
        conn.execute_batch(&format!(
            "DELETE FROM main.\"{name}\" AS owned
             WHERE NOT EXISTS (
                 SELECT 1 FROM temp.\"{name}\" AS desired WHERE {same_key}
             );"
        ))
        .map_err(storage_error)?;
    }

    for table in OWNED_TABLES {
        reconcile_rows(conn, table)?;
    }
    Ok(())
}

fn reconcile_rows(conn: &Connection, table: &OwnedTable) -> Result<()> {
    let name = table.name;
    let columns = identifiers(table.columns);
    let key_names = table.primary_key.split(',').collect::<Vec<_>>();
    let same_key = key_names
        .iter()
        .map(|key| format!("existing.\"{key}\" = desired.\"{key}\""))
        .collect::<Vec<_>>()
        .join(" AND ");
    let desired_columns = table
        .columns
        .split(',')
        .map(|column| format!("desired.\"{column}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let mutable_columns = table
        .columns
        .split(',')
        .filter(|column| !key_names.contains(column))
        .collect::<Vec<_>>();
    if !mutable_columns.is_empty() {
        let changed_columns = identifiers(&mutable_columns.join(","));
        let desired_values = mutable_columns
            .iter()
            .map(|column| format!("desired.\"{column}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let changed = mutable_columns
            .iter()
            .map(|column| format!("existing.\"{column}\" IS NOT desired.\"{column}\""))
            .collect::<Vec<_>>()
            .join(" OR ");
        // Update existing keys first. Reissuing an expired invitation must
        // release its live-login reservation before inserting its successor.
        // UPDATE also preserves valid unknown NOT NULL columns without passing
        // them through INSERT constraint checks. Unchanged rows run no trigger.
        conn.execute_batch(&format!(
            "UPDATE main.\"{name}\" AS existing
             SET ({changed_columns}) = (
                 SELECT {desired_values} FROM temp.\"{name}\" AS desired
                 WHERE {same_key}
             )
             WHERE EXISTS (
                 SELECT 1 FROM temp.\"{name}\" AS desired
                 WHERE {same_key} AND ({changed})
             );"
        ))
        .map_err(storage_error)?;
    }
    // A genuinely new row receives independent-column defaults. Unsupported
    // missing defaults and invalid final uniqueness still fail the transaction;
    // no replacement, temporary constraint bypass or generic key swap occurs.
    conn.execute_batch(&format!(
        "INSERT INTO main.\"{name}\" ({columns})
         SELECT {desired_columns} FROM temp.\"{name}\" AS desired
         WHERE NOT EXISTS (
             SELECT 1 FROM main.\"{name}\" AS existing WHERE {same_key}
         );"
    ))
    .map_err(storage_error)?;
    Ok(())
}
