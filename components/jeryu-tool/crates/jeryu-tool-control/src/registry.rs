use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[path = "registry_fields.rs"]
mod fields;
use fields::*;

fn build_summary(root: &Path) -> Result<Value, String> {
    let registry_path = root.join("tools-registry.toml");
    if !registry_path.exists() {
        return Err(fail("missing tools-registry.toml"));
    }
    let data = load_toml(&registry_path)?;
    let top = data
        .as_table()
        .ok_or_else(|| fail("tools-registry.toml must contain a top-level table"))?;
    exact_keys(top, &TOP_LEVEL_FIELDS, "tools-registry.toml top-level")?;
    if top.get("schema_version").and_then(toml::Value::as_str) != Some("1") {
        return Err(fail("schema_version must be \"1\""));
    }
    let tools = top
        .get("tool")
        .and_then(toml::Value::as_array)
        .cloned()
        .ok_or_else(|| fail("tools-registry.toml field \"tool\" must be a list"))?;
    let mut seen_ids = BTreeSet::new();
    let mut adopting = BTreeSet::new();
    let mut candidates = BTreeSet::new();
    let mut counts: BTreeMap<&str, u64> = VALID_TOOL_STATUS
        .into_iter()
        .map(|name| (name, 0))
        .collect();
    let mut rows = Vec::new();
    let mut realized = 0_u64;
    let mut anticipated = 0_u64;

    for value in &tools {
        let table = value
            .as_table()
            .ok_or_else(|| fail("every [[tool]] must be a table"))?;
        let display_id = table.get("id").and_then(toml::Value::as_str).unwrap_or("?");
        exact_tool_keys(table, &format!("tool {display_id:?}"))?;
        let id = string(table, "id", "tool")?;
        if !valid_slug(id) {
            return Err(fail(format!("tool id {id:?} is not a canonical slug")));
        }
        if !seen_ids.insert(id.to_owned()) {
            return Err(fail(format!("duplicate tool id {id:?}")));
        }
        let name = string(table, "name", &format!("tool {id:?}"))?;
        let description = string(table, "description", &format!("tool {id:?}"))?;
        let source = string(table, "source", &format!("tool {id:?}"))?;
        if !nonempty_text(name)
            || !nonempty_text(description)
            || source.chars().any(char::is_control)
        {
            return Err(fail(format!(
                "tool {id:?} has invalid name, description, or source text"
            )));
        }
        let kind = string(table, "kind", &format!("tool {id:?}"))?;
        if !VALID_KINDS.contains(&kind) {
            return Err(fail(format!("tool {id:?} has invalid kind {kind:?}")));
        }
        let status = string(table, "status", &format!("tool {id:?}"))?;
        if !VALID_TOOL_STATUS.contains(&status) {
            return Err(fail(format!("tool {id:?} has invalid status {status:?}")));
        }
        if table.get("origin_cluster").is_some_and(|value| {
            value
                .as_str()
                .is_none_or(|origin| !valid_origin_cluster(origin))
        }) {
            return Err(fail(format!(
                "tool {id:?} origin_cluster must be toolbuild- plus 16 lowercase hex characters"
            )));
        }
        if status == "published" && source.is_empty() {
            return Err(fail(format!(
                "published tool {id:?} must declare a non-empty source"
            )));
        }
        let adopting_repos = strings(table, "adopting_repos", &format!("tool {id:?}"))?;
        let candidate_repos = strings(table, "candidate_repos", &format!("tool {id:?}"))?;
        validate_unique_repos(&adopting_repos, &format!("tool {id:?} adopting_repos"))?;
        validate_unique_repos(&candidate_repos, &format!("tool {id:?} candidate_repos"))?;
        if adopting_repos
            .iter()
            .any(|repo| candidate_repos.contains(repo))
        {
            return Err(fail(format!(
                "tool {id:?} cannot list one repository as both adopting and candidate"
            )));
        }
        adopting.extend(adopting_repos.iter().cloned());
        candidates.extend(candidate_repos.iter().cloned());
        let loc_saved = integer(table, "loc_saved", &format!("tool {id:?}"))?;
        let loc_estimate = integer(table, "loc_saved_estimate", &format!("tool {id:?}"))?;
        realized = realized
            .checked_add(loc_saved)
            .ok_or_else(|| fail("realized LOC sum overflow"))?;
        anticipated = anticipated
            .checked_add(loc_estimate)
            .ok_or_else(|| fail("anticipated LOC sum overflow"))?;
        *counts.get_mut(status).expect("validated status") += 1;
        rows.push(json!({
            "adopting_repo_count": adopting_repos.len(),
            "candidate_repo_count": candidate_repos.len(),
            "id": id,
            "kind": kind,
            "loc_saved": loc_saved,
            "loc_saved_estimate": loc_estimate,
            "name": name,
            "status": status,
        }));
    }

    let tasks_dir = root.join("tasks");
    let mut task_ids = BTreeSet::new();
    let mut open_tasks = 0_u64;
    if tasks_dir.is_dir() {
        let mut paths = fs::read_dir(&tasks_dir)
            .map_err(|error| fail(format!("{}: {error}", tasks_dir.display())))?
            .map(|entry| entry.map(|item| item.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| fail(format!("{}: {error}", tasks_dir.display())))?;
        paths.retain(|path| path.extension().and_then(|value| value.to_str()) == Some("toml"));
        paths.sort();
        for path in paths {
            let task = load_toml(&path)?;
            let table = task
                .as_table()
                .ok_or_else(|| fail(format!("{} must contain a table", path.display())))?;
            let filename = path.file_name().unwrap_or_default().to_string_lossy();
            exact_keys(table, &TASK_FIELDS, &format!("task file {filename}"))?;
            let id = string(table, "id", &format!("task file {filename}"))?;
            if id.len() != 4
                || !id.bytes().all(|byte| byte.is_ascii_digit())
                || !filename.starts_with(&format!("{id}-"))
            {
                return Err(fail(format!(
                    "task {id:?} must use a four-digit id matching its filename"
                )));
            }
            if !task_ids.insert(id.to_owned()) {
                return Err(fail(format!("duplicate task id {id:?}")));
            }
            let status = string(table, "status", &format!("task {id:?}"))?;
            if !VALID_TASK_STATUS.contains(&status) {
                return Err(fail(format!("task {id:?} has invalid status {status:?}")));
            }
            let tool_id = string(table, "tool_id", &format!("task {id:?}"))?;
            if !valid_slug(tool_id) {
                return Err(fail(format!("task {id:?} has invalid tool_id")));
            }
            if !seen_ids.contains(tool_id) {
                return Err(fail(format!(
                    "task {id:?} references unknown tool_id {tool_id:?}"
                )));
            }
            for field in ["title", "origin_cluster"] {
                if !nonempty_text(string(table, field, &format!("task {id:?}"))?) {
                    return Err(fail(format!("task {id:?} has invalid {field}")));
                }
            }
            if !valid_origin_cluster(string(table, "origin_cluster", &format!("task {id:?}"))?) {
                return Err(fail(format!(
                    "task {id:?} origin_cluster must be toolbuild- plus 16 lowercase hex characters"
                )));
            }
            integer(table, "anticipated_loc_saved", &format!("task {id:?}"))?;
            let target_repos = strings(table, "target_repos", &format!("task {id:?}"))?;
            validate_unique_repos(&target_repos, &format!("task {id:?} target_repos"))?;
            if target_repos.is_empty() {
                return Err(fail(format!(
                    "task {id:?} must declare at least one target repository"
                )));
            }
            let rollout = strings(table, "rollout", &format!("task {id:?}"))?;
            if rollout.is_empty() || rollout.iter().any(|step| !nonempty_text(step)) {
                return Err(fail(format!(
                    "task {id:?} rollout must contain non-empty text steps"
                )));
            }
            if matches!(status, "open" | "in-progress") {
                open_tasks += 1;
            }
        }
    }

    let mut summary = Map::new();
    summary.insert("anticipated_loc_saved".to_owned(), json!(anticipated));
    summary.insert("adopting_repo_count".to_owned(), json!(adopting.len()));
    summary.insert("building_count".to_owned(), json!(counts["building"]));
    summary.insert("candidate_repo_count".to_owned(), json!(candidates.len()));
    summary.insert("deprecated_count".to_owned(), json!(counts["deprecated"]));
    summary.insert("open_task_count".to_owned(), json!(open_tasks));
    summary.insert("proposed_count".to_owned(), json!(counts["proposed"]));
    summary.insert("published_count".to_owned(), json!(counts["published"]));
    summary.insert("realized_loc_saved".to_owned(), json!(realized));
    summary.insert("tool_count".to_owned(), json!(tools.len()));
    summary.insert("tools".to_owned(), Value::Array(rows));
    Ok(Value::Object(summary))
}

pub fn run(root: &Path, args: &[String]) -> Result<i32, String> {
    if args.iter().any(|arg| arg != "--check") {
        return Err("registry-summary accepts only --check".to_owned());
    }
    let summary = build_summary(root)?;
    if args.iter().any(|arg| arg == "--check") {
        println!(
            "registry ok: {} tool(s), {} open task(s), {} LOC anticipated, {} LOC realized",
            summary["tool_count"],
            summary["open_task_count"],
            summary["anticipated_loc_saved"],
            summary["realized_loc_saved"]
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&summary)
                .map_err(|error| format!("failed to serialize registry summary: {error}"))?
        );
    }
    Ok(0)
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
