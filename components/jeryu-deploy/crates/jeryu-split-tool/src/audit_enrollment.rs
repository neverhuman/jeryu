//! Shared source-form and exact-revision enrollment admission.
use super::{Inventory, Source, effective_floor, oid};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeSet;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Version {
    Unversioned,
    Versioned,
}

pub(crate) fn version(schema: &str) -> Result<Version> {
    match schema {
        "jeryu.audit-repositories/v1" => Ok(Version::Unversioned),
        "jeryu.audit-repositories/v2" => Ok(Version::Versioned),
        _ => anyhow::bail!("unsupported audit inventory"),
    }
}

/// Slugs group ownership; this key does not normalize Cargo package source URLs.
pub(crate) fn identity(
    source: &Source,
    version: Version,
) -> Result<(String, String, Option<String>)> {
    ensure!(
        (source.repository.starts_with("neverhuman/")
            || (!source.required
                && source.path.is_none()
                && source.repository.starts_with("unresolved/")))
            && source
                .repository
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"/-_.".contains(&byte)),
        "invalid owned repository slug"
    );
    ensure!(
        matches!(
            source.scope.as_str(),
            "monorepo" | "component" | "standalone" | "dependency" | "optional"
        ),
        "unknown audit scope"
    );
    ensure!(
        (85..=100).contains(&source.minimum)
            && source.minimum
                >= effective_floor(source.repository.rsplit('/').next().unwrap_or("")),
        "audit enrollment cannot lower the effective floor"
    );
    if let Some(commit) = &source.commit {
        oid(commit)?;
    }
    let dependency = matches!(source.scope.as_str(), "dependency" | "optional");
    if dependency && source.commit.is_none() {
        ensure!(
            source.scope == "optional"
                && !source.required
                && source.path.is_none()
                && source.repository.starts_with("unresolved/")
                && source
                    .reason
                    .as_deref()
                    .is_some_and(|reason| !reason.trim().is_empty()),
            "external dependency enrollment requires an exact immutable commit"
        );
    }
    Ok((
        source.repository.clone(),
        source.scope.clone(),
        if version == Version::Versioned && dependency {
            source.commit.clone()
        } else {
            None
        },
    ))
}

pub(super) fn sources(manifest: &toml::Value, inventory: Inventory) -> Result<Vec<Source>> {
    let mut sources = Vec::new();
    for repo in manifest["repo"]
        .as_array()
        .context("repository inventory")?
    {
        let name = repo["name"].as_str().context("repository name")?;
        let slug = repo["github_slug"].as_str().context("GitHub repository")?;
        let path = repo["path"].as_str().context("component path")?;
        ensure!(
            path == "." || path == format!("components/{name}"),
            "noncanonical component path"
        );
        sources.push(Source {
            repository: slug.into(),
            scope: if path == "." { "monorepo" } else { "component" }.into(),
            path: Some(path.into()),
            commit: None,
            minimum: effective_floor(name),
            required: true,
            reason: None,
        });
        if path != "." {
            sources.push(Source { repository: slug.into(), scope: "standalone".into(), path: None,
                commit: None, minimum: effective_floor(name), required: true,
                reason: Some("Published mirror needs an exact independently acquired commit and export provenance; component results cannot substitute.".into()) });
        }
    }
    let version = version(&inventory.schema)?;
    let mut acquisitions = BTreeSet::new();
    for source in inventory.sources {
        ensure!(
            acquisitions.insert(identity(&source, version)?),
            "duplicate audit enrollment/acquisition"
        );
        // External versions are independent instances, never acquisition overlays.
        if matches!(source.scope.as_str(), "dependency" | "optional") {
            sources.push(source);
            continue;
        }
        if let Some(existing) = sources
            .iter_mut()
            .find(|old| old.repository == source.repository && old.scope == source.scope)
        {
            ensure!(
                existing.scope == "standalone"
                    && source.required
                    && source.minimum >= existing.minimum
                    && source.commit.is_some(),
                "acquisition cannot change owning component scope or policy; exact mirror commit required"
            );
            *existing = source;
        } else {
            sources.push(source);
        }
    }
    let mut identities = BTreeSet::new();
    for source in &sources {
        ensure!(
            identities.insert(identity(source, version)?),
            "duplicate audit scope"
        );
    }
    ensure!(
        sources
            .iter()
            .filter(|source| matches!(source.scope.as_str(), "monorepo" | "component"))
            .count()
            == 11,
        "incomplete Jeryu family inventory"
    );
    ensure!(
        sources
            .iter()
            .any(|source| source.repository == "neverhuman/jankurai"
                && source.required
                && source.scope == "dependency"),
        "required auditor dependency is missing"
    );
    Ok(sources)
}

#[cfg(test)]
#[path = "audit_enrollment_tests.rs"]
mod tests;
