use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::{JeryuMirrorError, Result};
use crate::model::*;

const MARKER: &str = "JERYU_JERYU_MIRROR_BUNDLE_V1\n";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleVerification {
    pub ok: bool,
    pub expected_digest: String,
    pub actual_digest: String,
    pub files_checked: Vec<String>,
    pub errors: Vec<String>,
}

pub fn write_bundle(path: impl AsRef<Path>, archive: &Archive) -> Result<BundleManifest> {
    let path = path.as_ref();
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)?;
    fs::write(path.join("JERYU_BUNDLE"), MARKER)?;

    let mut archive = archive.clone();
    archive.sort_for_determinism();
    let archive_json = serde_json::to_vec_pretty(&archive)?;
    fs::write(path.join("archive.json"), &archive_json)?;

    let mut files = vec!["JERYU_BUNDLE".to_string(), "archive.json".to_string()];
    for repo in &archive.repositories {
        let repo_dir = path
            .join("repos")
            .join(safe(&repo.owner))
            .join(safe(&repo.name));
        fs::create_dir_all(&repo_dir)?;
        write_json(&repo_dir.join("repository.json"), repo)?;
        write_json(&repo_dir.join("issues.json"), &repo.issues)?;
        write_json(&repo_dir.join("pull_requests.json"), &repo.pull_requests)?;
        write_json(&repo_dir.join("releases.json"), &repo.releases)?;
        write_json(&repo_dir.join("artifacts.json"), &repo.artifacts)?;
        write_json(&repo_dir.join("webhooks.json"), &repo.webhooks)?;
        write_json(&repo_dir.join("apps.json"), &repo.app_installations)?;
        let prefix = format!("repos/{}/{}/", safe(&repo.owner), safe(&repo.name));
        files.extend(
            [
                "repository.json",
                "issues.json",
                "pull_requests.json",
                "releases.json",
                "artifacts.json",
                "webhooks.json",
                "apps.json",
            ]
            .into_iter()
            .map(|name| format!("{prefix}{name}")),
        );
    }

    fs::create_dir_all(path.join("docs"))?;
    let instructions = "docs/restore-instructions.md".to_string();
    fs::write(path.join(&instructions), restore_instructions(&archive))?;
    files.push(instructions.clone());

    let manifest = BundleManifest {
        format: BUNDLE_FORMAT.to_string(),
        bundle_id: archive.archive_id,
        created_at: archive.generated_at,
        source: archive.source.clone(),
        counts: archive.counts(),
        archive_digest: archive.canonical_digest(),
        files,
        restore_instructions: instructions,
    };
    write_json(&path.join("manifest.json"), &manifest)?;
    Ok(manifest)
}

pub fn read_bundle(path: impl AsRef<Path>) -> Result<Archive> {
    let path = path.as_ref();
    let marker = fs::read_to_string(path.join("JERYU_BUNDLE"))?;
    if marker != MARKER {
        return Err(JeryuMirrorError::invalid_bundle(
            path,
            "marker does not match JeryuMirror v1",
        ));
    }
    let archive_text = fs::read_to_string(path.join("archive.json"))?;
    let archive: Archive = serde_json::from_str(&archive_text)?;
    if archive.format != BUNDLE_FORMAT {
        return Err(JeryuMirrorError::invalid_bundle(
            path,
            "archive format is not supported",
        ));
    }
    Ok(archive)
}

pub fn verify_bundle(path: impl AsRef<Path>) -> Result<BundleVerification> {
    let path = path.as_ref();
    let manifest_text = fs::read_to_string(path.join("manifest.json"))?;
    let manifest: BundleManifest = serde_json::from_str(&manifest_text)?;
    let archive = read_bundle(path)?;
    let actual = archive.canonical_digest();
    let mut errors = Vec::new();
    if actual != manifest.archive_digest {
        errors.push(format!(
            "archive digest mismatch: expected {}, actual {}",
            manifest.archive_digest, actual
        ));
    }
    for file in &manifest.files {
        if !path.join(file).exists() {
            errors.push(format!("missing file {file}"));
        }
    }
    Ok(BundleVerification {
        ok: errors.is_empty(),
        expected_digest: manifest.archive_digest,
        actual_digest: actual,
        files_checked: manifest.files,
        errors,
    })
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let data = serde_json::to_vec_pretty(value)?;
    fs::write(path, data)?;
    Ok(())
}

fn safe(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn restore_instructions(archive: &Archive) -> String {
    let counts = archive.counts();
    format!(
        "# JeryuMirror restore instructions\n\nBundle `{}` contains {} repositories, {} issues, {} pull requests, and {} releases.\n\n1. Verify `manifest.json` and `archive.json` digest.\n2. Recreate repositories and protected branches.\n3. Restore issues, pull requests, labels, milestones, releases, and artifact metadata.\n4. Rehydrate webhook/app tokens from the target secret store; secret values are not included.\n5. Run mirror drift detection after first sync.\n",
        archive.archive_id,
        counts.repositories,
        counts.issues,
        counts.pull_requests,
        counts.releases
    )
}

pub fn bundle_path(root: impl AsRef<Path>, archive: &Archive) -> PathBuf {
    root.as_ref()
        .join(format!("jeryu_mirror-{}", archive.archive_id))
}
