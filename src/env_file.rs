use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

pub(crate) fn parse_env_text(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(parse_env_line)
        .collect::<HashMap<_, _>>()
}

pub(crate) fn parse_env_line(line: &str) -> Option<(String, String)> {
    let key = env_line_key(line)?;
    if !key
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    let trimmed = line.trim_start();
    let (_, raw_value) = trimmed.split_once('=')?;
    Some((key.to_string(), unquote_env_value(raw_value.trim())))
}

pub(crate) fn unquote_env_value(value: &str) -> String {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

pub(crate) fn upsert_env_text(existing: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut lines = Vec::new();
    for line in existing.lines() {
        if env_line_key(line) == Some(key) {
            if !found {
                lines.push(format!("{key}={}", format_env_value(value)));
                found = true;
            }
        } else {
            lines.push(line.to_string());
        }
    }
    if !found {
        lines.push(format!("{key}={}", format_env_value(value)));
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

pub(crate) fn env_line_key(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (raw_key, _) = trimmed.split_once('=')?;
    let key = raw_key.trim();
    if key.is_empty() { None } else { Some(key) }
}

pub(crate) fn format_env_value(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':' | '/' | '@'))
    {
        value.to_string()
    } else {
        format!("{value:?}")
    }
}

pub(crate) fn read_text_file_optional(path: &Path, read_context: &str) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => {
            normalize_file_permissions_0600(path)?;
            Ok(Some(contents))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("{read_context} {}", path.display())),
    }
}

pub(crate) fn write_text_file_secure(
    path: &Path,
    content: &str,
    open_context: &str,
    write_context: &str,
) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("{open_context} {}", path.display()))?;
        file.write_all(content.as_bytes())
            .with_context(|| format!("{write_context} {}", path.display()))?;
        normalize_file_permissions_0600(path)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, content)
            .with_context(|| format!("{write_context} {}", path.display()))?;
    }
    Ok(())
}

pub(crate) fn normalize_file_permissions_0600(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .with_context(|| format!("stat {}", path.display()))?
            .permissions();
        if perms.mode() & 0o777 != 0o600 {
            perms.set_mode(0o600);
            std::fs::set_permissions(path, perms)
                .with_context(|| format!("chmod 0600 {}", path.display()))?;
        }
    }
    Ok(())
}
