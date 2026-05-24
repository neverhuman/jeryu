use super::*;
use crate::jekko_llm_pool_usage::record_key_failure;
use std::collections::BTreeSet;

fn write_user(root: &Path, user: &str, env_body: &str) -> PathBuf {
    let dir = root.join(user);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("llm.env"), env_body).unwrap();
    dir
}

#[tokio::test]
async fn discovers_immediate_users_and_selects_ready_candidate() {
    let temp = tempfile::tempdir().unwrap();
    write_user(temp.path(), "user_1", "OPENROUTER_API_KEY=one\n");
    write_user(temp.path(), "user_2", "OPENROUTER_API_KEY=two\n");
    std::fs::create_dir_all(temp.path().join("user_1").join("nested")).unwrap();
    std::fs::write(
        temp.path().join("user_1").join("nested").join("llm.env"),
        "OPENROUTER_API_KEY=nested\n",
    )
    .unwrap();

    let pool = JekkoKeyPool::new(temp.path());
    let users = pool.candidate_users("OPENROUTER_API_KEY").unwrap();
    assert_eq!(
        users,
        BTreeSet::from(["user_1".to_string(), "user_2".to_string()])
    );
    let selected = pool
        .select("OPENROUTER_API_KEY", "openrouter", "model-a")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.user_id, "user_1");
    assert_eq!(selected.key_source_path, temp.path().join("user_1/llm.env"));
}

#[tokio::test]
async fn selection_skips_cooldown_and_auth_failed() {
    let temp = tempfile::tempdir().unwrap();
    let user_1 = write_user(temp.path(), "user_1", "OPENROUTER_API_KEY=one\n");
    let user_2 = write_user(temp.path(), "user_2", "OPENROUTER_API_KEY=two\n");
    let now = Utc::now().timestamp();
    record_key_failure(
        &user_1.join("state.sqlite"),
        "openrouter",
        "model-a",
        &FailureUpdate {
            status: "rate_limited".to_string(),
            failed_at: now,
            cooldown_until: Some(now + 600),
        },
    )
    .await
    .unwrap();
    record_key_failure(
        &user_2.join("state.sqlite"),
        "openrouter",
        "model-a",
        &FailureUpdate {
            status: "auth_failed".to_string(),
            failed_at: now,
            cooldown_until: None,
        },
    )
    .await
    .unwrap();

    let pool = JekkoKeyPool::new(temp.path());
    assert!(
        pool.select("OPENROUTER_API_KEY", "openrouter", "model-a")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn high_attempt_key_is_deweighted() {
    let temp = tempfile::tempdir().unwrap();
    let user_1 = write_user(temp.path(), "user_1", "OPENROUTER_API_KEY=one\n");
    write_user(temp.path(), "user_2", "OPENROUTER_API_KEY=two\n");
    for _ in 0..5 {
        record_key_success(&user_1.join("state.sqlite"), "openrouter", "model-a")
            .await
            .unwrap();
    }

    let pool = JekkoKeyPool::new(temp.path());
    let selected = pool
        .select("OPENROUTER_API_KEY", "openrouter", "model-a")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.user_id, "user_2");
}

#[tokio::test]
async fn success_clears_cooldown_after_selection() {
    let temp = tempfile::tempdir().unwrap();
    let user_1 = write_user(temp.path(), "user_1", "OPENROUTER_API_KEY=one\n");
    let state = user_1.join("state.sqlite");
    let now = Utc::now().timestamp();
    record_key_failure(
        &state,
        "openrouter",
        "model-a",
        &FailureUpdate {
            status: "server_error".to_string(),
            failed_at: now - 3600,
            cooldown_until: Some(now - 3500),
        },
    )
    .await
    .unwrap();
    record_key_success(&state, "openrouter", "model-a")
        .await
        .unwrap();
    let usage = load_key_usage(&state, "openrouter", "model-a")
        .await
        .unwrap();
    assert_eq!(usage.status, "ready");
    assert_eq!(usage.cooldown_until, None);
    assert_eq!(usage.failures, 0);
}
