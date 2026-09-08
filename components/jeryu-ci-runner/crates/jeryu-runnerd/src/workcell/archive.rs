//! Private quarantine-first archive path validation helpers.

use super::*;

pub(super) fn validate_archive_entry(
    entry: &ArchiveEntry,
    destination_root: &Path,
    allowed_repo_roots: &[PathBuf],
) -> WorkcellResult<()> {
    validate_repository_path(&entry.path)?;
    match entry.kind {
        ArchiveEntryKind::File | ArchiveEntryKind::Directory => {}
        _ => {
            return Err(WorkcellError::tar_path_denied(format!(
                "archive entry {} is a {} and cannot be unpacked",
                entry.path.display(),
                entry.kind.as_str()
            )));
        }
    }

    let extracted_path = destination_root.join(&entry.path);
    if !is_within_any_root(&extracted_path, allowed_repo_roots) {
        return Err(WorkcellError::tar_path_denied(format!(
            "archive entry {} extracts outside the approved repo roots",
            entry.path.display()
        )));
    }
    Ok(())
}

fn validate_repository_path(path: &Path) -> WorkcellResult<()> {
    if path.is_absolute() {
        return Err(WorkcellError::tar_path_denied(format!(
            "absolute path {} is not allowed",
            path.display()
        )));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(WorkcellError::tar_path_denied(format!(
                    "path {} contains a forbidden component",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_export_path(path: &Path) -> WorkcellResult<()> {
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::RootDir | Component::Prefix(_) => {}
            _ => {
                return Err(WorkcellError::tar_path_denied(format!(
                    "path {} contains a forbidden component",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

pub(super) fn is_within_any_root(path: &Path, allowed_roots: &[PathBuf]) -> bool {
    allowed_roots.iter().any(|root| path.starts_with(root))
}
