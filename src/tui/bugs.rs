//! Owner: Bugs TUI surface
//! Proof: `cargo test -p jeryu --lib tui::bugs`
//! Invariants: Bugs view state is derived from canonical bug records.

use crate::bugtracker::{BugRecord, BugStatus};
use std::cmp::Reverse;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BugSortMode {
    #[default]
    Rank,
    Severity,
    Priority,
    Difficulty,
    Ready,
    Attempts,
    Updated,
}

impl BugSortMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rank => "rank",
            Self::Severity => "severity",
            Self::Priority => "priority",
            Self::Difficulty => "difficulty",
            Self::Ready => "ready",
            Self::Attempts => "attempts",
            Self::Updated => "updated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BugProjectRow {
    pub name: String,
    pub open: usize,
    pub ready: usize,
    pub blocked: usize,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BugRow {
    pub index: usize,
    pub selected: bool,
    pub id: String,
    pub title: String,
    pub project: String,
    pub source_project: String,
    pub status: BugStatus,
    pub severity: &'static str,
    pub priority: &'static str,
    pub difficulty: u8,
    pub attempt_count: i64,
    pub failed_attempt_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BugInspector {
    pub id: String,
    pub title: String,
    pub route: String,
    pub component: String,
    pub status: BugStatus,
    pub severity: &'static str,
    pub priority: &'static str,
    pub difficulty: u8,
    pub attempts: String,
    pub current_behavior: String,
    pub expected_behavior: String,
    pub impact: String,
    pub reproduction_steps: Vec<String>,
    pub evidence: Vec<String>,
    pub acceptance_criteria: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BugsViewModel {
    pub projects: Vec<BugProjectRow>,
    pub rows: Vec<BugRow>,
    pub selected: Option<BugInspector>,
    pub sort_mode: BugSortMode,
    pub selected_bug_index: usize,
    pub selected_project_index: usize,
}

pub fn clamp_bug_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

pub fn build_view_model(
    bugs: &[BugRecord],
    sort_mode: BugSortMode,
    selected_bug_index: usize,
    selected_project_index: usize,
) -> BugsViewModel {
    let mut project_counts = std::collections::BTreeMap::<String, (usize, usize, usize)>::new();
    for bug in bugs {
        let counts = project_counts
            .entry(bug.target_project.clone())
            .or_default();
        if !bug.status.is_terminal() {
            counts.0 += 1;
        }
        if bug.status == BugStatus::Ready {
            counts.1 += 1;
        }
        if matches!(bug.status, BugStatus::Blocked | BugStatus::NeedsInfo) {
            counts.2 += 1;
        }
    }

    let selected_project_index = clamp_bug_index(selected_project_index, project_counts.len());
    let projects = project_counts
        .into_iter()
        .enumerate()
        .map(|(index, (name, (open, ready, blocked)))| BugProjectRow {
            name,
            open,
            ready,
            blocked,
            selected: index == selected_project_index,
        })
        .collect::<Vec<_>>();

    let mut sorted = bugs.iter().enumerate().collect::<Vec<_>>();
    sort_bug_refs(&mut sorted, sort_mode);
    let selected_bug_index = clamp_bug_index(selected_bug_index, sorted.len());

    let rows = sorted
        .iter()
        .enumerate()
        .map(|(view_index, (source_index, bug))| BugRow {
            index: *source_index,
            selected: view_index == selected_bug_index,
            id: bug.id.clone(),
            title: bug.title.clone(),
            project: bug.target_project.clone(),
            source_project: bug.source_project.clone(),
            status: bug.status,
            severity: bug.severity.label(),
            priority: bug.priority.label(),
            difficulty: bug.difficulty,
            attempt_count: bug.attempt_count,
            failed_attempt_count: bug.failed_attempt_count,
        })
        .collect::<Vec<_>>();

    let selected = sorted
        .get(selected_bug_index)
        .map(|(_, bug)| inspector_from_bug(bug));

    BugsViewModel {
        projects,
        rows,
        selected,
        sort_mode,
        selected_bug_index,
        selected_project_index,
    }
}

fn sort_bug_refs(bugs: &mut [(usize, &BugRecord)], sort_mode: BugSortMode) {
    match sort_mode {
        BugSortMode::Rank => bugs.sort_by_key(|(_, bug)| {
            (
                bug.status != BugStatus::Ready,
                bug.severity,
                bug.priority,
                bug.difficulty,
                Reverse(bug.failed_attempt_count),
                Reverse(bug.updated_at.clone()),
                bug.id.clone(),
            )
        }),
        BugSortMode::Severity => bugs.sort_by_key(|(_, bug)| {
            (
                bug.severity,
                bug.priority,
                bug.status != BugStatus::Ready,
                bug.id.clone(),
            )
        }),
        BugSortMode::Priority => bugs.sort_by_key(|(_, bug)| {
            (
                bug.priority,
                bug.severity,
                bug.status != BugStatus::Ready,
                bug.id.clone(),
            )
        }),
        BugSortMode::Difficulty => bugs
            .sort_by_key(|(_, bug)| (bug.difficulty, bug.severity, bug.priority, bug.id.clone())),
        BugSortMode::Ready => bugs
            .sort_by_key(|(_, bug)| (bug.status != BugStatus::Ready, bug.severity, bug.id.clone())),
        BugSortMode::Attempts => bugs.sort_by_key(|(_, bug)| {
            (
                Reverse(bug.failed_attempt_count),
                Reverse(bug.attempt_count),
                bug.severity,
                bug.id.clone(),
            )
        }),
        BugSortMode::Updated => {
            bugs.sort_by_key(|(_, bug)| (Reverse(bug.updated_at.clone()), bug.id.clone()))
        }
    }
}

fn inspector_from_bug(bug: &BugRecord) -> BugInspector {
    BugInspector {
        id: bug.id.clone(),
        title: bug.title.clone(),
        route: format!("{} -> {}", bug.source_project, bug.target_project),
        component: bug.component.clone().unwrap_or_else(|| "-".into()),
        status: bug.status,
        severity: bug.severity.label(),
        priority: bug.priority.label(),
        difficulty: bug.difficulty,
        attempts: format!(
            "{} total / {} failed",
            bug.attempt_count, bug.failed_attempt_count
        ),
        current_behavior: bug.body.current_behavior.clone(),
        expected_behavior: bug.body.expected_behavior.clone(),
        impact: bug.body.impact.clone(),
        reproduction_steps: bug.body.reproduction_steps.clone(),
        evidence: bug
            .body
            .evidence
            .iter()
            .map(|item| {
                let mut line = format!("{}: {}", item.kind, item.summary);
                if let Some(path) = &item.path {
                    line.push_str(&format!(" path={path}"));
                }
                if let Some(url) = &item.url {
                    line.push_str(&format!(" url={url}"));
                }
                if item.redacted {
                    line.push_str(" redacted");
                }
                line
            })
            .collect(),
        acceptance_criteria: bug.body.acceptance_criteria.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bugtracker::{
        BugEvidenceInput, BugPriority, BugRecord, BugSeverity, CanonicalBugReport,
    };

    fn bug(
        id: &str,
        project: &str,
        status: BugStatus,
        severity: BugSeverity,
        attempts: i64,
        failed_attempts: i64,
        updated_at: &str,
    ) -> BugRecord {
        let body = CanonicalBugReport {
            target_project: project.into(),
            source_project: "source".into(),
            title: format!("{id} title"),
            component: Some("tui".into()),
            current_behavior: "current".into(),
            expected_behavior: "expected".into(),
            environment: "demo".into(),
            frequency: "always".into(),
            impact: "impact".into(),
            security_privacy: "none".into(),
            no_secrets_confirmed: true,
            reproduction_steps: vec!["run repro".into()],
            evidence: vec![BugEvidenceInput {
                kind: "log".into(),
                summary: "panic".into(),
                path: Some("target/log.txt".into()),
                url: None,
                digest: None,
                redacted: true,
            }],
            acceptance_criteria: vec!["passes focused test".into()],
            severity,
            priority: BugPriority::P1,
            difficulty: 3,
        };
        BugRecord {
            id: id.into(),
            title: body.title.clone(),
            source_project: body.source_project.clone(),
            target_project: project.into(),
            component: body.component.clone(),
            status,
            severity,
            priority: BugPriority::P1,
            difficulty: 3,
            impact: body.impact.clone(),
            security: false,
            owner: None,
            body,
            created_at: updated_at.into(),
            updated_at: updated_at.into(),
            attempt_count: attempts,
            failed_attempt_count: failed_attempts,
        }
    }

    #[test]
    fn rank_sort_prefers_ready_then_severity() {
        let bugs = vec![
            bug(
                "S1_BLOCKED",
                "alpha",
                BugStatus::Blocked,
                BugSeverity::S1,
                1,
                0,
                "2026-01-01",
            ),
            bug(
                "S0_READY",
                "alpha",
                BugStatus::Ready,
                BugSeverity::S0,
                0,
                0,
                "2026-01-02",
            ),
            bug(
                "S1_READY",
                "beta",
                BugStatus::Ready,
                BugSeverity::S1,
                2,
                1,
                "2026-01-03",
            ),
        ];

        let vm = build_view_model(&bugs, BugSortMode::Rank, 0, 0);
        let ids = vm
            .rows
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["S0_READY", "S1_READY", "S1_BLOCKED"]);
    }

    #[test]
    fn attempts_sort_places_failed_attempts_first() {
        let bugs = vec![
            bug(
                "A",
                "alpha",
                BugStatus::Ready,
                BugSeverity::S2,
                3,
                1,
                "2026-01-01",
            ),
            bug(
                "B",
                "alpha",
                BugStatus::Ready,
                BugSeverity::S2,
                2,
                2,
                "2026-01-02",
            ),
        ];

        let vm = build_view_model(&bugs, BugSortMode::Attempts, 0, 0);

        assert_eq!(vm.rows[0].id, "B");
    }

    #[test]
    fn selection_indices_are_clamped() {
        let bugs = vec![bug(
            "A",
            "alpha",
            BugStatus::Ready,
            BugSeverity::S2,
            0,
            0,
            "2026",
        )];

        let vm = build_view_model(&bugs, BugSortMode::Rank, 99, 99);

        assert_eq!(vm.selected_bug_index, 0);
        assert_eq!(vm.selected_project_index, 0);
        assert!(vm.rows[0].selected);
    }

    #[test]
    fn empty_state_has_no_selection() {
        let vm = build_view_model(&[], BugSortMode::Rank, 2, 3);

        assert!(vm.projects.is_empty());
        assert!(vm.rows.is_empty());
        assert!(vm.selected.is_none());
        assert_eq!(vm.selected_bug_index, 0);
    }

    #[test]
    fn inspector_details_come_from_canonical_body() {
        let bugs = vec![bug(
            "A",
            "alpha",
            BugStatus::Ready,
            BugSeverity::S2,
            4,
            2,
            "2026",
        )];

        let selected = build_view_model(&bugs, BugSortMode::Rank, 0, 0)
            .selected
            .expect("selected inspector");

        assert_eq!(selected.current_behavior, "current");
        assert_eq!(selected.expected_behavior, "expected");
        assert_eq!(selected.reproduction_steps, vec!["run repro"]);
        assert_eq!(
            selected.evidence,
            vec!["log: panic path=target/log.txt redacted"]
        );
        assert_eq!(selected.attempts, "4 total / 2 failed");
    }
}
