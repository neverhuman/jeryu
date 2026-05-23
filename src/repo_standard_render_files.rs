use crate::repo_standard::{ManagedFile, StandardProvider, StandardSpec};

const JANKURAI_INSTALLER: &str = include_str!("../scripts/install-jankurai.sh");
const JANKURAI_MANIFEST: &str = include_str!("../scripts/jankurai-manifest.json");

use super::*;

pub(crate) fn render_standard_files(spec: &StandardSpec) -> Vec<ManagedFile> {
    let mut files = vec![
        ManagedFile {
            path: ".jeryu/project.toml",
            content: render_project_toml(spec),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/delivery.toml",
            content: render_delivery_toml(spec),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/policies/release.toml",
            content: render_release_policy_toml(spec),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/policies/risk.toml",
            content: render_risk_policy_toml(),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/protected-paths.toml",
            content: render_protected_paths_toml(spec),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/ci/jankurai-manifest.json",
            content: ensure_trailing_newline(JANKURAI_MANIFEST),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/ci/install-jankurai.sh",
            content: ensure_trailing_newline(JANKURAI_INSTALLER),
            executable: true,
        },
        ManagedFile {
            path: ".jeryu/ci/required.sh",
            content: render_required_sh(spec),
            executable: true,
        },
        ManagedFile {
            path: ".jeryu/ci/fast.sh",
            content: render_fast_sh(spec),
            executable: true,
        },
        ManagedFile {
            path: ".jeryu/hooks/pre-push",
            content: render_pre_push_hook(&spec.base_branch, spec.provider),
            executable: true,
        },
        ManagedFile {
            path: ".jeryu/hooks/pre-commit",
            content: render_pre_commit_hook(),
            executable: true,
        },
        ManagedFile {
            path: ".jeryu/autonomy/autonomy.yml",
            content: render_autonomy_yml(spec),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/autonomy/policies/approvals.yml",
            content: render_autonomy_approvals_yml(),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/autonomy/policies/risk.yml",
            content: render_autonomy_risk_yml(),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/autonomy/policies/protected-paths.yml",
            content: render_autonomy_protected_paths_yml(),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/autonomy/policies/release.yml",
            content: render_autonomy_release_yml(spec),
            executable: false,
        },
    ];

    match spec.provider {
        StandardProvider::Github => {
            files.extend([
                ManagedFile {
                    path: ".github/workflows/jeryu-required.yml",
                    content: render_github_required_workflow(),
                    executable: false,
                },
                ManagedFile {
                    path: ".github/AGENTS.md",
                    content: render_github_agents_md(),
                    executable: false,
                },
                ManagedFile {
                    path: ".github/CODEOWNERS",
                    content: render_codeowners(spec),
                    executable: false,
                },
                ManagedFile {
                    path: ".github/PULL_REQUEST_TEMPLATE.md",
                    content: render_pr_template(),
                    executable: false,
                },
            ]);
        }
        StandardProvider::Gitlab => {
            files.push(ManagedFile {
                path: ".gitlab-ci.yml",
                content: render_gitlab_ci_yml(),
                executable: false,
            });
        }
    }

    let lock = render_standard_lock(spec, &files);
    files.push(ManagedFile {
        path: ".jeryu/standard.lock",
        content: lock,
        executable: false,
    });
    files
}
