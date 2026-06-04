use std::path::Path;

use crate::error::missing_auth;
use crate::fs_receipt::{copy_imported_files, copy_private_file, create_private_dir};
use crate::{AgentAuthError, AgentToolKind, RunAuthReceipt, doctor};

/// Materialize imported portable auth into a per-run home.
pub fn materialize_run_home(
    data_home: &Path,
    tool: AgentToolKind,
    run_home: &Path,
) -> Result<RunAuthReceipt, AgentAuthError> {
    let imported = doctor(data_home, tool)?;
    if !imported.ok {
        return Err(missing_auth(tool));
    }
    create_private_dir(run_home)?;
    let mut files = Vec::new();
    match tool {
        AgentToolKind::Codex => {
            let target = run_home.join("codex");
            create_private_dir(&target)?;
            files.extend(copy_imported_files(&imported.files, &target)?);
        }
        AgentToolKind::Claude => {
            let target = run_home.join(".claude");
            create_private_dir(&target)?;
            files.extend(copy_imported_files(&imported.files, &target)?);
        }
        AgentToolKind::Jekko => {
            let target = run_home.join(".jekko/users/jeryu-agent");
            create_private_dir(&target)?;
            for file in imported.files {
                if file.path.file_name().and_then(|n| n.to_str()) == Some("llm.env") {
                    files.push(copy_private_file(&file.path, &target.join("llm.env"))?);
                }
            }
        }
    }
    Ok(RunAuthReceipt {
        tool,
        run_home: run_home.to_path_buf(),
        files,
    })
}
