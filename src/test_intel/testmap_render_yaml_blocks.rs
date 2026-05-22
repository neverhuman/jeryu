use std::path::Path;

fn extract_top_level_yaml_block(content: &str, block_name: &str) -> Option<String> {
    let header = format!("{block_name}:");
    let mut started = false;
    let mut out = Vec::new();

    for line in content.lines() {
        let is_top_level = !line.starts_with(' ') && !line.starts_with('\t');
        if !started {
            if line.trim_start().starts_with(&header) {
                started = true;
                out.push(line.to_string());
            }
            continue;
        }

        if is_top_level && !line.trim().is_empty() && line.split_once(':').is_some() {
            break;
        }
        out.push(line.to_string());
    }

    if out.is_empty() {
        None
    } else {
        Some(format!("{}\n", out.join("\n")))
    }
}

fn top_level_block_names(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            if line.starts_with(' ') || line.starts_with('\t') {
                return None;
            }
            let (name, _) = line.split_once(':')?;
            if name.is_empty() {
                return None;
            }
            Some(name.to_string())
        })
        .collect()
}

pub(crate) fn strip_job_rules(block: &str) -> String {
    let mut out = Vec::new();
    let mut skipping_rules = false;

    for line in block.lines() {
        let indent = line.chars().take_while(|ch| *ch == ' ').count();
        if indent == 2 && line.trim() == "rules:" {
            skipping_rules = true;
            continue;
        }
        if skipping_rules {
            if !line.trim().is_empty() && indent <= 2 {
                skipping_rules = false;
            } else {
                continue;
            }
        }
        out.push(line.to_string());
    }

    format!("{}\n", out.join("\n"))
}

pub(crate) fn collect_ci_blocks(
    workspace: &Path,
) -> (Vec<String>, std::collections::BTreeMap<String, String>) {
    let mut hidden = Vec::new();
    let mut jobs = std::collections::BTreeMap::new();
    let ci_dir = workspace.join("ci/gitlab");
    let Ok(entries) = std::fs::read_dir(ci_dir) else {
        return (hidden, jobs);
    };

    let mut paths = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("yml") {
            paths.push(path);
        }
    }
    paths.sort();

    for path in paths {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for name in top_level_block_names(&content) {
            if let Some(block) = extract_top_level_yaml_block(&content, &name) {
                if name.starts_with('.') {
                    hidden.push(block);
                } else {
                    jobs.entry(name).or_insert(block);
                }
            }
        }
    }

    (hidden, jobs)
}
