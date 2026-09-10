//! Selected-commit ancestry, independent of remote default branches and tags.
use super::{Session, oid};
use anyhow::{Result, ensure};
use std::{collections::BTreeSet, path::Path};

fn object_set(value: &str) -> Result<BTreeSet<String>> {
    value
        .lines()
        .map(|line| {
            oid(line)?;
            Ok(line.to_owned())
        })
        .collect()
}

pub(super) fn verify(
    session: &mut Session,
    path: &Path,
    commit: &str,
    bare: bool,
) -> Result<String> {
    oid(commit)?;
    ensure!(
        session.run(path, &["rev-parse", "--is-bare-repository"])?
            == if bare { "true" } else { "false" },
        "unexpected source repository form"
    );
    ensure!(
        session.run(path, &["rev-parse", "HEAD"])? == commit,
        "selected source commit changed"
    );
    ensure!(
        session.run(path, &["for-each-ref"])?.is_empty(),
        "selected source must have no branch, tag, remote or replacement refs"
    );
    ensure!(
        session.run(path, &["rev-parse", "--is-shallow-repository"])? == "false",
        "selected source history is shallow"
    );
    let metadata = if bare {
        path.to_owned()
    } else {
        path.join(".git")
    };
    for forbidden in ["shallow", "info/grafts", "objects/info/alternates", "logs"] {
        ensure!(
            !metadata.join(forbidden).try_exists()?,
            "selected source has additional or redirected history inputs"
        );
    }
    session.run(path, &["fsck", "--full", "--no-reflogs"])?;
    let reachable = object_set(&session.run(
        path,
        &["rev-list", "--objects", "--no-object-names", commit],
    )?)?;
    let stored = object_set(&session.run(
        path,
        &[
            "cat-file",
            "--batch-all-objects",
            "--batch-check=%(objectname)",
        ],
    )?)?;
    ensure!(
        !reachable.is_empty() && stored == reachable,
        "selected source object database includes missing or unrelated objects"
    );
    Ok(super::super::hash(
        reachable
            .into_iter()
            .collect::<Vec<_>>()
            .join("\n")
            .as_bytes(),
    ))
}
