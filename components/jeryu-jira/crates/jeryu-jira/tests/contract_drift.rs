use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use jeryu_jira::contracts::{CONTRACT_COUNT, contract_files, export_all_contracts};
use ts_rs::Config;

fn committed_contract_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("contracts")
        .join("generated")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

#[test]
fn generated_work_contracts_are_byte_identical_to_ts_rs_output() {
    let committed = committed_contract_dir();
    assert!(
        committed.is_dir(),
        "committed contracts dir not found at {}",
        committed.display()
    );
    let temp = tempfile::tempdir().expect("temp dir");
    let cfg = Config::new().with_out_dir(temp.path());
    export_all_contracts(&cfg).expect("export contracts");

    let files = contract_files();
    assert_eq!(files.len(), CONTRACT_COUNT);
    let expected: BTreeSet<String> = files.iter().map(|(_, file)| file.clone()).collect();
    let committed_files: BTreeSet<String> = std::fs::read_dir(&committed)
        .expect("read committed contracts")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().into_string().ok()?;
            name.ends_with(".ts").then_some(name)
        })
        .collect();
    assert_eq!(
        committed_files, expected,
        "committed contract file set differs from the ts-rs export list"
    );

    let mut diffs = Vec::new();
    for (_, file) in files {
        let expected_text = read(&temp.path().join(&file));
        let committed_text = read(&committed.join(&file));
        if expected_text != committed_text {
            diffs.push(file);
        }
    }
    assert!(
        diffs.is_empty(),
        "committed Work contracts drifted from ts-rs output: {}",
        diffs.join(", ")
    );
}
