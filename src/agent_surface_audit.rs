use super::*;

pub fn build_audit_report() -> Result<AgentSurfaceAudit> {
    let root = repo_root()?;
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    let token_budget_present = root.join("token-budget.toml").is_file();
    if !token_budget_present {
        issues.push(AuditIssue {
            scope: "root".to_string(),
            path: "token-budget.toml".to_string(),
            detail: "missing token budget configuration".to_string(),
        });
    }

    let root_agents = root.join("AGENTS.md");
    let root_agents_ok = check_sections(&root_agents, REQUIRED_ROOT_SECTIONS, &mut issues)?;

    let rtk_doc = root.join("docs/RTK.md");
    let rtk_doc_present = rtk_doc.is_file();
    if !rtk_doc_present {
        issues.push(AuditIssue {
            scope: "root".to_string(),
            path: "docs/RTK.md".to_string(),
            detail: "missing RTK usage guidance".to_string(),
        });
    }

    let entries = module_entries(&root)?;
    for entry in &entries {
        if entry.owner.trim().is_empty() {
            warnings.push(AuditIssue {
                scope: "module".to_string(),
                path: entry.path.clone(),
                detail: "missing `//! Owner:` header".to_string(),
            });
        }
        if entry.proof.trim().is_empty() {
            warnings.push(AuditIssue {
                scope: "module".to_string(),
                path: entry.path.clone(),
                detail: "missing `//! Proof:` header".to_string(),
            });
        }
        if entry.invariants.trim().is_empty() {
            warnings.push(AuditIssue {
                scope: "module".to_string(),
                path: entry.path.clone(),
                detail: "missing `//! Invariants:` header".to_string(),
            });
        }
    }

    let index = build_index(&root)?;
    let expected_json = serde_json::to_string_pretty(&index)?;
    let expected_markdown = render_markdown(&index);
    let index_current = generated_index_is_current(
        &root.join("agent-index.json"),
        &expected_json,
        &root.join("agent-index.md"),
        &expected_markdown,
    );
    if !index_current {
        issues.push(AuditIssue {
            scope: "root".to_string(),
            path: "agent-index.{json,md}".to_string(),
            detail: "generated index is missing or outdated".to_string(),
        });
    }

    Ok(AgentSurfaceAudit {
        ok: issues.is_empty(),
        token_budget_present,
        root_agents_ok,
        rtk_doc_present,
        index_current,
        modules_checked: entries.len(),
        issues,
        warnings,
    })
}

pub(crate) fn repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("current dir")?;
    Ok(cwd)
}
