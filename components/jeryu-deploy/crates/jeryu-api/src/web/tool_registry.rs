//! Reusable-tool registry route for the golden box on `/repos`.
//!
//! Projects `jeryu-tool/tools-registry.toml` (+ its `tasks/` build queue) into
//! the [`ToolRegistrySummary`] the SPA renders. The path is resolved from the
//! split manifest in `serve()`; when it is unset or unreadable the endpoint
//! returns an empty summary so the SPA simply renders nothing.

use std::collections::BTreeSet;
use std::path::Path;

use axum::Json;
use axum::extract::State;
use jeryu_readmodel::contracts::{ToolRegistryEntry, ToolRegistrySummary};
use serde::Deserialize;

use super::tool_status_messages::{
    STATUS_BUILDING, STATUS_PROPOSED, STATUS_PUBLISHED, STATUS_RETIRED,
};
use super::{WebState, server_time};

#[derive(Debug, Default, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    tool: Vec<RegistryTool>,
}

#[derive(Debug, Deserialize)]
struct RegistryTool {
    #[allow(dead_code)]
    id: String,
    name: String,
    kind: String,
    status: String,
    #[serde(default)]
    adopting_repos: Vec<String>,
    #[serde(default)]
    candidate_repos: Vec<String>,
    #[serde(default)]
    loc_saved: u32,
    #[serde(default)]
    loc_saved_estimate: u32,
}

#[derive(Debug, Deserialize)]
struct TaskFile {
    #[serde(default)]
    status: Option<String>,
}

pub(super) async fn summary(
    State(state): State<std::sync::Arc<WebState>>,
) -> Json<ToolRegistrySummary> {
    Json(build_summary(state.tool_registry_path.as_deref()))
}

/// Build the summary from `tools-registry.toml`, or an empty summary if the
/// registry is absent/malformed. Aggregation mirrors
/// `jeryu-tool/ops/registry_summary.py` (kept trivial on purpose). Shared with
/// the MCP backend (`tool_registry.summary`).
pub(super) fn build_summary(registry_path: Option<&Path>) -> ToolRegistrySummary {
    let empty = || ToolRegistrySummary {
        generated_at: server_time(),
        tool_count: 0,
        published_count: 0,
        building_count: 0,
        proposed_count: 0,
        deprecated_count: 0,
        adopting_repo_count: 0,
        candidate_repo_count: 0,
        open_task_count: 0,
        realized_loc_saved: 0,
        anticipated_loc_saved: 0,
        tools: Vec::new(),
    };

    let Some(path) = registry_path else {
        return empty();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return empty();
    };
    let Ok(registry) = toml::from_str::<RegistryFile>(&text) else {
        return empty();
    };

    let mut adopting: BTreeSet<String> = BTreeSet::new();
    let mut candidates: BTreeSet<String> = BTreeSet::new();
    let (mut published, mut building, mut proposed, mut retired) = (0u32, 0u32, 0u32, 0u32);
    let (mut realized, mut anticipated) = (0u32, 0u32);
    let mut tools: Vec<ToolRegistryEntry> = Vec::with_capacity(registry.tool.len());

    for tool in &registry.tool {
        adopting.extend(tool.adopting_repos.iter().cloned());
        candidates.extend(tool.candidate_repos.iter().cloned());
        match tool.status.as_str() {
            STATUS_PUBLISHED => published += 1,
            STATUS_BUILDING => building += 1,
            STATUS_PROPOSED => proposed += 1,
            STATUS_RETIRED => retired += 1,
            _ => {}
        }
        realized = realized.saturating_add(tool.loc_saved);
        anticipated = anticipated.saturating_add(tool.loc_saved_estimate);
        tools.push(ToolRegistryEntry {
            id: tool.id.clone(),
            name: tool.name.clone(),
            kind: tool.kind.clone(),
            status: tool.status.clone(),
            adopting_repo_count: tool.adopting_repos.len() as u32,
            candidate_repo_count: tool.candidate_repos.len() as u32,
            loc_saved: tool.loc_saved,
            loc_saved_estimate: tool.loc_saved_estimate,
        });
    }

    // Headline first: most-realized, then most-anticipated saving.
    tools.sort_by(|a, b| {
        b.loc_saved
            .cmp(&a.loc_saved)
            .then(b.loc_saved_estimate.cmp(&a.loc_saved_estimate))
            .then(a.id.cmp(&b.id))
    });

    ToolRegistrySummary {
        generated_at: server_time(),
        tool_count: registry.tool.len() as u32,
        published_count: published,
        building_count: building,
        proposed_count: proposed,
        deprecated_count: retired,
        adopting_repo_count: adopting.len() as u32,
        candidate_repo_count: candidates.len() as u32,
        open_task_count: count_open_tasks(path),
        realized_loc_saved: realized,
        anticipated_loc_saved: anticipated,
        tools,
    }
}

/// Count `open`/`in-progress` build tasks in the `tasks/` dir beside the registry.
fn count_open_tasks(registry_path: &Path) -> u32 {
    let Some(tasks_dir) = registry_path.parent().map(|dir| dir.join("tasks")) else {
        return 0;
    };
    let Ok(entries) = std::fs::read_dir(&tasks_dir) else {
        return 0;
    };
    let mut open = 0u32;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(task) = toml::from_str::<TaskFile>(&text)
            && matches!(task.status.as_deref(), Some("open") | Some("in-progress"))
        {
            open += 1;
        }
    }
    open
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_when_no_path() {
        let summary = build_summary(None);
        assert_eq!(summary.tool_count, 0);
        assert!(summary.tools.is_empty());
        assert!(!summary.generated_at.is_empty());
    }

    #[test]
    fn aggregates_registry_and_tasks() {
        let dir = std::env::temp_dir().join(format!(
            "jeryu-tool-registry-{}",
            jeryu_runner_core::receipt::now_ms()
        ));
        std::fs::create_dir_all(dir.join("tasks")).unwrap();
        std::fs::write(
            dir.join("tools-registry.toml"),
            r#"
schema_version = "1"
[[tool]]
id = "a"
name = "A"
kind = "rust-crate"
status = "published"
adopting_repos = ["r1", "r2"]
candidate_repos = ["r3"]
loc_saved = 100
loc_saved_estimate = 0
[[tool]]
id = "b"
name = "B"
kind = "shell-lib"
status = "proposed"
adopting_repos = []
candidate_repos = ["r2", "r3", "r4"]
loc_saved = 0
loc_saved_estimate = 56
"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("tasks").join("0001-b.toml"),
            "id = \"0001\"\ntool_id = \"b\"\nstatus = \"open\"\n",
        )
        .unwrap();

        let summary = build_summary(Some(&dir.join("tools-registry.toml")));
        assert_eq!(summary.tool_count, 2);
        assert_eq!(summary.published_count, 1);
        assert_eq!(summary.proposed_count, 1);
        assert_eq!(summary.adopting_repo_count, 2); // r1, r2
        assert_eq!(summary.candidate_repo_count, 3); // r2, r3, r4
        assert_eq!(summary.realized_loc_saved, 100);
        assert_eq!(summary.anticipated_loc_saved, 56);
        assert_eq!(summary.open_task_count, 1);
        // Sorted: published (loc_saved 100) first.
        assert_eq!(summary.tools[0].id, "a");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
