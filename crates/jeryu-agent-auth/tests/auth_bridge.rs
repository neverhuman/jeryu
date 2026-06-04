use std::collections::BTreeMap;
use std::path::Path;

use jeryu_agent_auth::{AgentToolKind, AuthImportOptions, import_from_host, materialize_run_home};

fn options(tool: AgentToolKind, home: &Path, data: &Path) -> AuthImportOptions {
    AuthImportOptions {
        tool,
        host_home: home.to_path_buf(),
        data_home: data.to_path_buf(),
        env: BTreeMap::new(),
    }
}

#[test]
fn codex_import_copies_auth_json_without_secret_in_receipt() {
    let temp = tempfile::tempdir().expect("tempdir");
    let host = temp.path().join("host");
    let data = temp.path().join("data");
    std::fs::create_dir_all(host.join(".codex")).expect("codex dir");
    std::fs::write(host.join(".codex/auth.json"), r#"{"token":"secret"}"#).expect("auth");

    let receipt =
        import_from_host(&options(AgentToolKind::Codex, &host, &data)).expect("import succeeds");

    assert_eq!(receipt.tool, AgentToolKind::Codex);
    assert_eq!(receipt.files.len(), 1);
    assert!(receipt.files[0].digest.starts_with("sha256:"));
    assert!(!serde_json::to_string(&receipt).unwrap().contains("secret"));
    assert_eq!(receipt.files[0].mode, "0600");
    assert!(data.join("agent-auth/codex/auth.json").is_file());
}

#[test]
fn claude_keychain_only_is_typed_denial() {
    let temp = tempfile::tempdir().expect("tempdir");
    let host = temp.path().join("host");
    let data = temp.path().join("data");
    std::fs::create_dir_all(host.join(".claude")).expect("claude dir");
    std::fs::write(host.join(".claude/keychain.json"), "{}").expect("keychain");

    let err = import_from_host(&options(AgentToolKind::Claude, &host, &data))
        .expect_err("host-bound auth denied");

    assert_eq!(err.code, "agent_auth_host_bound");
    assert_eq!(err.repair.common_fixes.len(), 2);
}

#[test]
fn missing_auth_returns_required_repair_shape() {
    let temp = tempfile::tempdir().expect("tempdir");
    let err = import_from_host(&options(
        AgentToolKind::Jekko,
        &temp.path().join("host"),
        &temp.path().join("data"),
    ))
    .expect_err("missing auth");

    assert_eq!(err.code, "agent_auth_missing");
    assert!(!err.repair.purpose.is_empty());
    assert!(!err.repair.reason.is_empty());
    assert!(!err.repair.common_fixes.is_empty());
    assert!(!err.repair.docs_url.is_empty());
    assert!(!err.repair.repair_hint.is_empty());
}

#[test]
fn jekko_materializes_llm_env_under_per_run_home() {
    let temp = tempfile::tempdir().expect("tempdir");
    let host = temp.path().join("host");
    let data = temp.path().join("data");
    let run = temp.path().join("run");
    let jekko_dir = host.join(".jekko/users/jeryu-agent");
    std::fs::create_dir_all(&jekko_dir).expect("jekko dir");
    std::fs::write(jekko_dir.join("llm.env"), "TOKEN=secret\n").expect("llm env");

    import_from_host(&options(AgentToolKind::Jekko, &host, &data)).expect("import");
    let receipt = materialize_run_home(&data, AgentToolKind::Jekko, &run).expect("materialize");

    assert_eq!(receipt.files.len(), 1);
    assert!(run.join(".jekko/users/jeryu-agent/llm.env").is_file());
    assert!(!serde_json::to_string(&receipt).unwrap().contains("secret"));
}
