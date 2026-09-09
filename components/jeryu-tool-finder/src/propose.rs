//! `propose`: promote a finder dossier into a jeryu-tool proposal — a
//! `[[tool]]` registry entry (status=proposed) plus a `tasks/NNNN-*.toml`
//! build task with a per-repo rollout stub. Idempotent on `origin_cluster`:
//! re-proposing the same cluster is a no-op. The registry is APPENDED as
//! text so its comments and formatting survive.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;
use serde::Deserialize;

use crate::paths;

#[derive(Args)]
pub struct ProposeArgs {
    /// Cluster id to promote (from dossiers/index.json).
    cluster_id: String,
    /// The sibling jeryu-tool checkout owning the registry.
    #[arg(long = "jeryu-tool", default_value_os_t = paths::default_jeryu_tool())]
    jeryu_tool: PathBuf,
    /// Dossier index produced by `jeryu-tool-finder dossier`.
    #[arg(long, default_value_os_t = paths::default_dossier_dir().join("index.json"))]
    dossiers: PathBuf,
    /// Override the derived tool id.
    #[arg(long)]
    tool_id: Option<String>,
    /// Override the suggested name.
    #[arg(long)]
    name: Option<String>,
    /// Override the suggested kind.
    #[arg(long)]
    kind: Option<String>,
    /// Print what would be written without writing it.
    #[arg(long)]
    dry_run: bool,
}

/// The dossier fields propose needs.
#[derive(Debug, Deserialize)]
struct DossierRecord {
    cluster_id: String,
    anticipated_loc_saved: usize,
    suggested_kind: String,
    suggested_name: String,
    candidate_repos: Vec<String>,
    #[serde(default)]
    insight: String,
}

#[derive(Debug, Deserialize)]
struct RegistryProbe {
    #[serde(default)]
    tool: Vec<RegistryProbeTool>,
}

#[derive(Debug, Deserialize)]
struct RegistryProbeTool {
    id: String,
    #[serde(default)]
    origin_cluster: Option<String>,
}

pub fn run(args: ProposeArgs) -> Result<()> {
    let dossier = load_dossier(&args.dossiers, &args.cluster_id)?;
    let registry_path = args.jeryu_tool.join("tools-registry.toml");
    let tasks_dir = args.jeryu_tool.join("tasks");
    if !registry_path.is_file() {
        return Err(crate::errors::FinderError::new(
            "propose a tool into the jeryu-tool registry",
            "a propose precondition failed (missing registry/dossiers or id collision)",
            &[
                "pass --jeryu-tool pointing at the sibling jeryu-tool checkout",
                "run `jeryu-tool-finder dossier` first",
                "pass a distinct --tool-id on id collisions",
            ],
            "fix the precondition, then rerun the proposal",
        )
        .into());
    }

    let registry_text = std::fs::read_to_string(&registry_path)
        .with_context(|| format!("read {}", registry_path.display()))?;
    let registry: RegistryProbe =
        toml::from_str(&registry_text).context("parse tools-registry.toml")?;

    if let Some(existing) = registry
        .tool
        .iter()
        .find(|tool| tool.origin_cluster.as_deref() == Some(args.cluster_id.as_str()))
    {
        println!(
            "already proposed: tool {:?} <- cluster {}",
            existing.id, args.cluster_id
        );
        return Ok(());
    }

    let tool_id = slugify(args.tool_id.as_deref().unwrap_or(&dossier.suggested_name));
    if registry.tool.iter().any(|tool| tool.id == tool_id) {
        return Err(crate::errors::FinderError::new(
            "propose a tool into the jeryu-tool registry",
            "a propose precondition failed (missing registry/dossiers or id collision)",
            &[
                "pass --jeryu-tool pointing at the sibling jeryu-tool checkout",
                "run `jeryu-tool-finder dossier` first",
                "pass a distinct --tool-id on id collisions",
            ],
            "fix the precondition, then rerun the proposal",
        )
        .into());
    }

    let name = args.name.unwrap_or_else(|| dossier.suggested_name.clone());
    let kind = args.kind.unwrap_or_else(|| dossier.suggested_kind.clone());
    let estimate = dossier.anticipated_loc_saved;
    let task_index = next_task_index(&tasks_dir);
    let task_id = format!("{task_index:04}");
    let description = dossier.insight.replace('"', "'");
    let repos_toml = toml_list(&dossier.candidate_repos);
    let cluster_id = &args.cluster_id;

    let tool_block = format!(
        "\n# Proposed by jeryu-tool-finder from cluster {cluster_id}.\n\
         [[tool]]\n\
         id = \"{tool_id}\"\n\
         name = \"{name}\"\n\
         kind = \"{kind}\"\n\
         status = \"proposed\"\n\
         source = \"\"\n\
         description = \"{description}\"\n\
         origin_cluster = \"{cluster_id}\"\n\
         adopting_repos = []\n\
         candidate_repos = {repos_toml}\n\
         loc_saved = 0\n\
         loc_saved_estimate = {estimate}\n"
    );
    let task_text = format!(
        "# tasks/{task_id}-{tool_id}.toml — filed by jeryu-tool-finder.\n\n\
         id = \"{task_id}\"\n\
         tool_id = \"{tool_id}\"\n\
         title = \"Extract {name} into a shared {kind}\"\n\
         status = \"open\"\n\
         origin_cluster = \"{cluster_id}\"\n\
         anticipated_loc_saved = {estimate}\n\
         target_repos = {repos_toml}\n\
         rollout = [\n\
         \x20 \"Build the tool in its canonical home and tag it.\",\n\
         \x20 \"Replace each target repo's local copy with the shared tool.\",\n\
         \x20 \"Move migrated repos from candidate_repos to adopting_repos and grow loc_saved.\",\n\
         \x20 \"Confirm each repo's gate lanes stay green after the swap.\",\n\
         ]\n"
    );
    let task_path = tasks_dir.join(format!("{task_id}-{tool_id}.toml"));

    if args.dry_run {
        println!(
            "[dry-run] would append to {}:\n{tool_block}",
            registry_path.display()
        );
        println!(
            "[dry-run] would write {}:\n{task_text}",
            task_path.display()
        );
        return Ok(());
    }

    let mut appended = registry_text;
    appended.push_str(&tool_block);
    std::fs::write(&registry_path, appended)
        .with_context(|| format!("append to {}", registry_path.display()))?;
    std::fs::create_dir_all(&tasks_dir).context("create tasks dir")?;
    std::fs::write(&task_path, task_text)
        .with_context(|| format!("write {}", task_path.display()))?;

    println!("proposed tool {tool_id:?} (+{estimate} LOC anticipated) <- cluster {cluster_id}");
    println!("  registry: {}", registry_path.display());
    println!("  task:     {}", task_path.display());
    println!("Review, then run jeryu-tool's `just check` to validate.");
    Ok(())
}

fn load_dossier(path: &Path, cluster_id: &str) -> Result<DossierRecord> {
    if !path.is_file() {
        return Err(crate::errors::FinderError::new(
            "propose a tool into the jeryu-tool registry",
            "a propose precondition failed (missing registry/dossiers or id collision)",
            &[
                "pass --jeryu-tool pointing at the sibling jeryu-tool checkout",
                "run `jeryu-tool-finder dossier` first",
                "pass a distinct --tool-id on id collisions",
            ],
            "fix the precondition, then rerun the proposal",
        )
        .into());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let dossiers: Vec<DossierRecord> =
        serde_json::from_str(&text).context("parse dossier index")?;
    dossiers
        .into_iter()
        .find(|dossier| dossier.cluster_id == cluster_id)
        .with_context(|| format!("cluster {cluster_id:?} not found in {}", path.display()))
}

fn slugify(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    let mut last_dash = true;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "tool".to_string()
    } else {
        slug
    }
}

fn toml_list(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }
    let inner = items
        .iter()
        .map(|item| format!("  \"{item}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    format!("[\n{inner},\n]")
}

fn next_task_index(tasks_dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(tasks_dir) else {
        return 1;
    };
    let mut highest = 0usize;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.ends_with(".toml") {
            continue;
        }
        let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(index) = digits.parse::<usize>() {
            highest = highest.max(index);
        }
    }
    highest + 1
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{next_task_index, slugify, toml_list};

    proptest! {
        /// Slugs are always non-empty lowercase [a-z0-9-] with no edge dashes,
        /// and slugify is idempotent — registry ids stay stable however the
        /// suggested name was spelled.
        #[test]
        fn slugify_emits_stable_registry_ids(input in ".{0,64}") {
            let slug = slugify(&input);
            prop_assert!(!slug.is_empty());
            prop_assert!(slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
            prop_assert!(!slug.starts_with('-') && !slug.ends_with('-'));
            prop_assert_eq!(slugify(&slug), slug);
        }

        /// Rendered candidate-repo lists always parse back as a TOML array of
        /// the same strings, so a propose append can never corrupt the registry.
        #[test]
        fn toml_list_round_trips(repos in proptest::collection::vec("[a-z0-9-]{1,24}", 0..8)) {
            let rendered = format!("repos = {}", toml_list(&repos));
            let parsed: toml::Value = toml::from_str(&rendered).expect("rendered TOML parses");
            let back: Vec<String> = parsed["repos"]
                .as_array()
                .expect("array")
                .iter()
                .map(|v| v.as_str().expect("string").to_string())
                .collect();
            prop_assert_eq!(back, repos);
        }
    }

    #[test]
    fn task_index_starts_at_one_for_missing_dir() {
        assert_eq!(
            next_task_index(std::path::Path::new("/nonexistent-tasks")),
            1
        );
    }
}
