use super::*;

pub(super) fn member_paths(
    inventory: &mut Inventory,
    path: &str,
    value: &Value,
    caps: &[&str],
) -> Vec<String> {
    let Some(values) = value.as_array() else {
        inventory.error(path, "workspaces", "invalid_workspace_members", caps);
        return Vec::new();
    };
    let mut selected = BTreeSet::new();
    for value in values {
        let Some(member) = value
            .as_str()
            .filter(|member| relative(member) && !member.contains(['*', '?', '[']))
        else {
            inventory.error(
                path,
                "workspaces",
                "dynamic_or_escaping_workspace_member",
                caps,
            );
            continue;
        };
        if !selected.insert(member.to_owned()) {
            inventory.error(path, "workspaces", "duplicate_workspace_member", caps);
        }
    }
    selected.into_iter().collect()
}

pub(super) fn existing(root: &Path, path: &str) -> bool {
    // Only absence omits an optional slot. Links and permission failures reach load().
    !matches!(fs::symlink_metadata(root.join(path)), Err(error) if error.kind()==std::io::ErrorKind::NotFound)
}
pub(super) fn directory_files(
    inventory: &mut Inventory,
    root: &Path,
    path: &str,
    caps: &[&str],
) -> Vec<String> {
    let directory = root.join(path);
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(_) => {
            inventory.error(path, "directory", "input_directory_unreadable", caps);
            return Vec::new();
        }
    };
    if !relative(path)
        || !metadata.is_dir()
        || fs::canonicalize(&directory).ok().as_ref() != Some(&directory)
    {
        inventory.error(path, "directory", "unsafe_input_directory", caps);
        return Vec::new();
    }
    let entries = match fs::read_dir(&directory) {
        Ok(value) => value,
        Err(_) => {
            inventory.error(path, "directory", "input_directory_unreadable", caps);
            return Vec::new();
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => {
                if let Some(name) = entry.file_name().to_str() {
                    paths.push(format!("{path}/{name}"));
                } else {
                    inventory.error(path, "directory", "non_utf8_input_filename", caps);
                }
            }
            Err(_) => inventory.error(path, "directory", "input_directory_changed", caps),
        }
    }
    paths.sort();
    paths
}
