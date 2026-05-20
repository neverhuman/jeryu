use super::CanonicalBugReport;

pub fn canonical_markdown(report: &CanonicalBugReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", report.title.trim()));
    out.push_str(&format!("Target project: `{}`\n", report.target_project));
    out.push_str(&format!("Source project: `{}`\n", report.source_project));
    if let Some(component) = &report.component {
        out.push_str(&format!("Component: `{}`\n", component));
    }
    out.push('\n');
    section(&mut out, "Current behavior", &report.current_behavior);
    section(&mut out, "Expected behavior", &report.expected_behavior);
    section(&mut out, "Environment", &report.environment);
    section(&mut out, "Frequency", &report.frequency);
    section(&mut out, "Impact", &report.impact);
    section(&mut out, "Security/privacy", &report.security_privacy);
    out.push_str(&format!(
        "\nNo secrets confirmed: {}\n\n",
        report.no_secrets_confirmed
    ));
    out.push_str("## Reproduction\n");
    if report.reproduction_steps.is_empty() {
        out.push_str("- Not provided\n");
    } else {
        for (idx, step) in report.reproduction_steps.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", idx + 1, step));
        }
    }
    out.push_str("\n## Evidence\n");
    if report.evidence.is_empty() {
        out.push_str("- Not provided\n");
    } else {
        for item in &report.evidence {
            out.push_str(&format!("- {}: {}", item.kind, item.summary));
            if let Some(path) = &item.path {
                out.push_str(&format!(" path={path}"));
            }
            if let Some(url) = &item.url {
                out.push_str(&format!(" url={url}"));
            }
            if item.redacted {
                out.push_str(" redacted=true");
            }
            out.push('\n');
        }
    }
    out.push_str("\n## Acceptance criteria\n");
    if report.acceptance_criteria.is_empty() {
        out.push_str("- Fix restores expected behavior\n");
    } else {
        for criterion in &report.acceptance_criteria {
            out.push_str(&format!("- {criterion}\n"));
        }
    }
    out
}

fn section(out: &mut String, title: &str, body: &str) {
    out.push_str(&format!("## {title}\n{}\n\n", body.trim()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bugtracker::{BugPriority, BugSeverity};

    #[test]
    fn markdown_contains_canonical_sections() {
        let report = CanonicalBugReport {
            target_project: "redlinedb".into(),
            source_project: "veox".into(),
            title: "Adapter failure".into(),
            component: None,
            current_behavior: "bad".into(),
            expected_behavior: "good".into(),
            environment: "local".into(),
            frequency: "always".into(),
            impact: "blocks".into(),
            security_privacy: "none".into(),
            no_secrets_confirmed: true,
            reproduction_steps: vec!["run test".into()],
            evidence: Vec::new(),
            acceptance_criteria: Vec::new(),
            severity: BugSeverity::S2,
            priority: BugPriority::P2,
            difficulty: 3,
        };
        let md = canonical_markdown(&report);
        assert!(md.contains("## Current behavior"));
        assert!(md.contains("## Reproduction"));
        assert!(md.contains("No secrets confirmed: true"));
    }
}
