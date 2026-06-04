use std::collections::BTreeMap;
use std::path::Path;

use crate::error::missing_auth;
use crate::fs_receipt::{
    auth_dir, copy_private_file, create_private_dir, receipts_for_dir, write_private_file,
};
use crate::{
    AgentAuthError, AgentToolKind, AuthDoctorReport, AuthFileReceipt, AuthImportOptions,
    AuthImportReceipt,
};

/// Import portable auth into `data_home/agent-auth/<tool>/`.
pub fn import_from_host(options: &AuthImportOptions) -> Result<AuthImportReceipt, AgentAuthError> {
    let auth_dir = auth_dir(&options.data_home, options.tool);
    create_private_dir(&auth_dir)?;

    let files = match options.tool {
        AgentToolKind::Codex => import_codex(options, &auth_dir)?,
        AgentToolKind::Claude => import_claude(options, &auth_dir)?,
        AgentToolKind::Jekko => import_jekko(options, &auth_dir)?,
    };

    Ok(AuthImportReceipt {
        tool: options.tool,
        auth_dir,
        files,
    })
}

/// Check whether portable auth for `tool` has been imported.
pub fn doctor(data_home: &Path, tool: AgentToolKind) -> Result<AuthDoctorReport, AgentAuthError> {
    let auth_dir = auth_dir(data_home, tool);
    if !auth_dir.is_dir() {
        return Ok(AuthDoctorReport {
            tool,
            ok: false,
            files: Vec::new(),
        });
    }
    let files = receipts_for_dir(&auth_dir)?;
    Ok(AuthDoctorReport {
        tool,
        ok: !files.is_empty(),
        files,
    })
}

fn import_codex(
    options: &AuthImportOptions,
    auth_dir: &Path,
) -> Result<Vec<AuthFileReceipt>, AgentAuthError> {
    let source = options.host_home.join(".codex/auth.json");
    if source.is_file() {
        return Ok(vec![copy_private_file(
            &source,
            &auth_dir.join("auth.json"),
        )?]);
    }
    if let Some(value) = nonempty_env(&options.env, "OPENAI_API_KEY") {
        return Ok(vec![write_private_file(
            &auth_dir.join("env"),
            format!("OPENAI_API_KEY={value}\n").as_bytes(),
        )?]);
    }
    Err(missing_auth(AgentToolKind::Codex))
}

fn import_claude(
    options: &AuthImportOptions,
    auth_dir: &Path,
) -> Result<Vec<AuthFileReceipt>, AgentAuthError> {
    let env_names = [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN",
    ];
    let mut contents = String::new();
    for name in env_names {
        if let Some(value) = nonempty_env(&options.env, name) {
            contents.push_str(name);
            contents.push('=');
            contents.push_str(value);
            contents.push('\n');
        }
    }
    if !contents.is_empty() {
        return Ok(vec![write_private_file(
            &auth_dir.join("env"),
            contents.as_bytes(),
        )?]);
    }

    let credentials = options.host_home.join(".claude/.credentials.json");
    if credentials.is_file() {
        return Ok(vec![copy_private_file(
            &credentials,
            &auth_dir.join(".credentials.json"),
        )?]);
    }

    let keychain_markers = [
        options.host_home.join(".claude/keychain.json"),
        options.host_home.join(".claude/.credentials.keychain"),
    ];
    if keychain_markers.iter().any(|path| path.exists()) {
        return Err(AgentAuthError::new(
            "agent_auth_host_bound",
            "import portable Claude auth",
            "Claude auth appears to be host-bound keychain material only",
            &[
                "export ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN before importing",
                "refresh Claude auth into a portable .claude/.credentials.json file",
            ],
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-agent-auth --jobs 40",
        ));
    }
    Err(missing_auth(AgentToolKind::Claude))
}

fn import_jekko(
    options: &AuthImportOptions,
    auth_dir: &Path,
) -> Result<Vec<AuthFileReceipt>, AgentAuthError> {
    let source = options.host_home.join(".jekko/users/jeryu-agent/llm.env");
    if source.is_file() {
        return Ok(vec![copy_private_file(&source, &auth_dir.join("llm.env"))?]);
    }
    if let Some(value) = nonempty_env(&options.env, "JEKKO_LLM_ENV") {
        return Ok(vec![write_private_file(
            &auth_dir.join("llm.env"),
            value.as_bytes(),
        )?]);
    }
    Err(missing_auth(AgentToolKind::Jekko))
}

fn nonempty_env<'a>(env: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    env.get(key).map(String::as_str).filter(|v| !v.is_empty())
}
