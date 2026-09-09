//! `summary`: print the sibling jeryu-tool registry summary. The authoritative
//! aggregation lives in jeryu-tool's locked Rust control binary (the registry
//! owner); this subcommand delegates through its thin shell entrypoint so
//! there is exactly one summary implementation.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::paths;

#[derive(Args)]
pub struct SummaryArgs {
    /// The sibling jeryu-tool checkout owning the registry.
    #[arg(long = "jeryu-tool", default_value_os_t = paths::default_jeryu_tool())]
    jeryu_tool: PathBuf,
    /// Extra arguments passed through to the Rust registry-summary entrypoint.
    #[arg(trailing_var_arg = true)]
    rest: Vec<String>,
}

pub fn run(args: SummaryArgs) -> Result<()> {
    let tool_root = args
        .jeryu_tool
        .canonicalize()
        .with_context(|| format!("resolve {}", args.jeryu_tool.display()))?;
    let script = tool_root.join("ops").join("registry-summary.sh");
    let metadata = std::fs::symlink_metadata(&script)
        .with_context(|| format!("inspect {}", script.display()))?;
    if !metadata.file_type().is_file() || script.canonicalize()? != script {
        bail!("registry summary script not found: {}", script.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            bail!(
                "registry summary script is multiply linked: {}",
                script.display()
            );
        }
    }
    let status = std::process::Command::new("/usr/bin/bash")
        .arg(&script)
        .args(&args.rest)
        .current_dir(&tool_root)
        .status()
        .context("run registry-summary.sh")?;
    if !status.success() {
        bail!("registry-summary.sh exited with {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_missing_and_propagates_sibling_failure() {
        let root = tempfile::tempdir().expect("temporary sibling");
        let missing = run(SummaryArgs {
            jeryu_tool: root.path().to_owned(),
            rest: vec![],
        })
        .expect_err("missing entrypoint must fail");
        assert!(missing.to_string().contains("inspect"));

        fs::create_dir(root.path().join("ops")).expect("ops directory");
        fs::write(
            root.path().join("ops/registry-summary.sh"),
            "#!/usr/bin/env bash\nexit 7\n",
        )
        .expect("failure fixture");
        let failed = run(SummaryArgs {
            jeryu_tool: root.path().to_owned(),
            rest: vec!["--check".to_owned()],
        })
        .expect_err("sibling status must propagate");
        assert!(failed.to_string().contains("exited with exit status: 7"));
    }

    #[test]
    fn forwards_arguments_to_the_single_link_rust_wrapper() {
        let root = tempfile::tempdir().expect("temporary sibling");
        fs::create_dir(root.path().join("ops")).expect("ops directory");
        fs::write(
            root.path().join("ops/registry-summary.sh"),
            "#!/usr/bin/env bash\n[[ \"$#\" == 2 && \"$1\" == --check && \"$2\" == exact ]]\n",
        )
        .expect("success fixture");
        run(SummaryArgs {
            jeryu_tool: root.path().to_owned(),
            rest: vec!["--check".to_owned(), "exact".to_owned()],
        })
        .expect("arguments must reach sibling wrapper");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_and_multiply_linked_wrappers() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("temporary sibling");
        fs::create_dir(root.path().join("ops")).expect("ops directory");
        let outside = root.path().join("outside.sh");
        fs::write(&outside, "#!/usr/bin/env bash\nexit 0\n").expect("outside fixture");
        let wrapper = root.path().join("ops/registry-summary.sh");
        symlink(&outside, &wrapper).expect("symlink fixture");
        assert!(
            run(SummaryArgs {
                jeryu_tool: root.path().to_owned(),
                rest: vec![],
            })
            .is_err(),
            "symlinked wrapper must fail"
        );

        fs::remove_file(&wrapper).expect("remove symlink");
        fs::hard_link(&outside, &wrapper).expect("hardlink fixture");
        let linked = run(SummaryArgs {
            jeryu_tool: root.path().to_owned(),
            rest: vec![],
        })
        .expect_err("multiply linked wrapper must fail");
        assert!(linked.to_string().contains("multiply linked"));
    }
}
