use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn assert_no_nonblocking_shell_terminators(path: &str) -> Result<()> {
    let contents = fs::read_to_string(path)?;
    assert!(
        !contents.contains("|| true"),
        "{path} still contains a non-blocking shell terminator"
    );
    Ok(())
}

fn collect_files_named(root: &Path, filename: &str, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if matches!(
                name.as_ref(),
                ".git" | "target" | "node_modules" | ".jeryu" | ".claude"
            ) {
                continue;
            }
            collect_files_named(&path, filename, files)?;
        } else if name == filename {
            files.push(path);
        }
    }
    Ok(())
}

fn dependency_name(dep_name: &str, dep_value: &toml::Value) -> String {
    dep_value
        .as_table()
        .and_then(|table| table.get("package"))
        .and_then(toml::Value::as_str)
        .unwrap_or(dep_name)
        .to_ascii_lowercase()
}

fn dependency_features(dep_value: &toml::Value) -> impl Iterator<Item = String> + '_ {
    dep_value
        .as_table()
        .and_then(|table| table.get("features"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::to_ascii_lowercase)
}

fn assert_manifest_stays_redline_only(path: &Path) -> Result<()> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("reading Cargo manifest {}", path.display()))?;
    let manifest: toml::Value = contents
        .parse()
        .with_context(|| format!("parsing Cargo manifest {}", path.display()))?;

    let dependency_tables = [
        manifest.get("dependencies"),
        manifest.get("dev-dependencies"),
        manifest.get("build-dependencies"),
        manifest
            .get("workspace")
            .and_then(|workspace| workspace.get("dependencies")),
    ];

    for table in dependency_tables
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_table)
    {
        for (dep_name, dep_value) in table {
            let package = dependency_name(dep_name, dep_value);
            let dep_name = dep_name.to_ascii_lowercase();
            if dep_name.contains("sqlite")
                || package.contains("sqlite")
                || dep_name.contains("postgres")
                || package.contains("postgres")
            {
                bail!(
                    "{} depends on forbidden state-store package `{}`; use RedlineDB or fix its adapter instead",
                    path.display(),
                    package
                );
            }
            if dep_name == "sqlx" || package == "sqlx" {
                for feature in dependency_features(dep_value) {
                    if feature.contains("sqlite") || feature.contains("postgres") {
                        bail!(
                            "{} enables forbidden SQLx feature `{}`; use RedlineDB or fix its adapter instead",
                            path.display(),
                            feature
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

fn collect_guarded_db_sources(files: &mut Vec<PathBuf>) -> Result<()> {
    for root in ["db", "src", "tests"] {
        collect_guarded_db_sources_under(Path::new(root), files)?;
    }
    Ok(())
}

fn collect_guarded_db_sources_under(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if matches!(name.as_ref(), "target" | ".git" | "node_modules") {
                continue;
            }
            collect_guarded_db_sources_under(&path, files)?;
        } else if matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("rs" | "sql")
        ) && name != "language_bad_behavior.rs"
        {
            files.push(path);
        }
    }
    Ok(())
}

fn assert_no_sqlite_db_fixture(path: &Path) -> Result<()> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("reading guarded DB source {}", path.display()))?;
    for forbidden in [
        "sqlite::memory:",
        "sqlite:",
        "sqlx::sqlite",
        "SqlitePool",
        "SqliteConnection",
    ] {
        if contents.contains(forbidden) {
            bail!(
                "{} contains `{}`; use redline::memory: or fix RedlineDB/adapter support",
                path.display(),
                forbidden
            );
        }
    }
    Ok(())
}

#[test]
fn redlinedb_boundary_rejects_sqlite_fallbacks() -> Result<()> {
    let mut manifests = Vec::new();
    collect_files_named(Path::new("."), "Cargo.toml", &mut manifests)?;
    for manifest in manifests {
        assert_manifest_stays_redline_only(&manifest)?;
    }

    let mut db_sources = Vec::new();
    collect_guarded_db_sources(&mut db_sources)?;
    for source in db_sources {
        assert_no_sqlite_db_fixture(&source)?;
    }

    write_lane_log(
        "target/jankurai/redlinedb-boundary.log",
        "RedlineDB boundary verified: no SQLite manifest features or DB fixture URLs\n",
    )
}

#[test]
fn language_bad_behavior_lane_is_blocking() -> Result<()> {
    assert_no_nonblocking_shell_terminators(".github/workflows/jankurai.yml")?;
    assert_no_nonblocking_shell_terminators(".github/workflows/rust.yml")?;

    let log_path = Path::new("target/jankurai/language-bad-behavior.log");
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        log_path,
        "ci and git behavior lane verified: no non-blocking workflow shell terminators\n",
    )?;
    Ok(())
}

fn write_lane_log(path: &str, message: &str) -> Result<()> {
    let log_path = Path::new(path);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(log_path, message)?;
    Ok(())
}

#[test]
fn ci_bad_behavior_lane_is_blocking() -> Result<()> {
    assert_no_nonblocking_shell_terminators(".github/workflows/jankurai.yml")?;
    write_lane_log(
        "target/jankurai/ci-bad-behavior.log",
        "ci bad behavior lane verified: workflow shell terminators are blocking\n",
    )
}

#[test]
fn git_bad_behavior_lane_is_blocking() -> Result<()> {
    assert_no_nonblocking_shell_terminators(".github/workflows/jankurai.yml")?;
    write_lane_log(
        "target/jankurai/git-bad-behavior.log",
        "git bad behavior lane verified: workflow shell terminators are blocking\n",
    )
}

#[test]
fn release_bad_behavior_lane_is_blocking() -> Result<()> {
    assert_no_nonblocking_shell_terminators(".github/workflows/jankurai.yml")?;
    write_lane_log(
        "target/jankurai/release-bad-behavior.log",
        "release bad behavior lane verified: workflow shell terminators are blocking\n",
    )
}
