use super::*;
use clap::Parser;
use std::{
    ffi::OsString,
    os::unix::fs::{MetadataExt, symlink},
};

#[test]
fn create_once_is_deterministic_and_conflicting_bytes_preserve_originals() {
    let fixture = Fixture::new();
    let bundle = fixture.prepare();
    let again = fixture.prepare();
    assert_eq!(bundle.destination(), again.destination());
    assert_eq!(bundle.files, again.files);
    let stored = create_once(&fixture.output, &bundle).unwrap();
    assert!(!stored.already_present);
    assert_eq!(fs::metadata(&stored.path).unwrap().mode() & 0o777, 0o700);
    let originals: BTreeMap<_, _> = fs::read_dir(&stored.path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert_eq!(entry.metadata().unwrap().mode() & 0o777, 0o600);
            (entry.file_name(), fs::read(entry.path()).unwrap())
        })
        .collect();
    assert!(
        create_once(&fixture.output, &again)
            .unwrap()
            .already_present
    );
    // Equivalent receipt semantics but different bytes collide at the immutable
    // expected attempt address; even benign formatting cannot replace evidence.
    let mut receipt = fixture.receipt();
    receipt.push(b'\n');
    let conflict = prepare(&fixture.source, &fixture.expected, fixture.inputs(&receipt)).unwrap();
    assert_eq!(bundle.destination(), conflict.destination());
    assert!(create_once(&fixture.output, &conflict).is_err());
    for (name, bytes) in originals {
        assert_eq!(fs::read(stored.path.join(name)).unwrap(), bytes);
    }
    assert!(create_once(&fixture.source, &bundle).is_err());
    let mut crossing = fixture.prepare();
    crossing.source_root = fixture.temporary.join("neverhuman");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&crossing.source_root)
        .unwrap();
    assert!(create_once(&fixture.temporary, &crossing).is_err());
    assert_eq!(fs::read_dir(&crossing.source_root).unwrap().count(), 0);
}

#[test]
fn linked_or_nonregular_inputs_and_destinations_are_refused_without_cleanup() {
    let fixture = Fixture::new();
    let original = fixture.temporary.join("ordinary");
    write(&original, b"evidence");
    assert_eq!(storage::read_file(&original, 8).unwrap(), b"evidence");
    assert!(storage::read_file(&original, 7).is_err());
    let linked = fixture.temporary.join("symlink");
    symlink(&original, &linked).unwrap();
    assert!(storage::read_file(&linked, 8).is_err());
    fs::hard_link(&original, fixture.temporary.join("hardlink")).unwrap();
    assert!(storage::read_file(&original, 8).is_err());
    let fifo = fixture.temporary.join("fifo");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    assert!(storage::read_file(&fifo, 8).is_err());
    let bundle = fixture.prepare();
    symlink(&fixture.source, fixture.output.join("neverhuman")).unwrap();
    assert!(create_once(&fixture.output, &bundle).is_err());
    assert!(!fixture.source.join("jeryu").exists());
}

pub(super) fn arguments(
    fixture: &Fixture,
    missing_report: bool,
    missing_renderer: bool,
) -> cli::Arguments {
    let context = json!({"schema_version":"jeryu.audit-package-local-context/v1",
        "identity":fixture.expected.identity,"command_exit":fixture.expected.command_exit,
        "report_sha256":fixture.expected.report_sha256,"renderer":fixture.expected.renderer});
    for (name, bytes) in [
        ("context", serde_json::to_vec(&context).unwrap()),
        ("receipt", fixture.receipt()),
        ("report", fixture.report.clone()),
        ("candidate-policy", fixture.policy.clone()),
        ("governing-policy", fixture.policy.clone()),
        ("dependency-lock", fixture.lock.clone()),
        ("execution-config", fixture.config.clone()),
        ("auditor-receipt", fixture.auditor.clone()),
    ] {
        write(&fixture.temporary.join(name), &bytes);
    }
    let mut args = vec![
        OsString::from("jeryu-split"),
        OsString::from("audit-package"),
    ];
    for name in [
        "context",
        "receipt",
        "candidate-policy",
        "governing-policy",
        "dependency-lock",
        "execution-config",
        "auditor-receipt",
    ] {
        args.push(format!("--{name}").into());
        args.push(fixture.temporary.join(name).into_os_string());
    }
    args.push("--report".into());
    args.push(
        fixture
            .temporary
            .join(if missing_report {
                "missing-report"
            } else {
                "report"
            })
            .into_os_string(),
    );
    args.push("--output-root".into());
    args.push(fixture.output.clone().into_os_string());
    if missing_renderer {
        args.push("--renderer-output".into());
        args.push(fixture.temporary.join("missing-renderer").into_os_string());
    }
    match crate::Cli::try_parse_from(args).unwrap().command {
        crate::Command::AuditPackage(arguments) => arguments,
        _ => panic!("owning CLI must route audit-package"),
    }
}

#[test]
fn owning_cli_stores_pending_or_error_and_never_returns_required_success() {
    for (missing_report, missing_renderer) in [(false, false), (true, false), (false, true)] {
        let fixture = Fixture::new();
        let expected_path = fixture.output.join(fixture.prepare().destination());
        let args = arguments(&fixture, missing_report, missing_renderer);
        let error = cli::run(&fixture.source, args).unwrap_err().to_string();
        assert!(error.contains("publication"), "{error}");
        let metadata: Value =
            serde_json::from_slice(&fs::read(expected_path.join("provenance.json")).unwrap())
                .unwrap();
        assert_eq!(
            metadata["display_state"],
            if missing_report || missing_renderer {
                "ERROR"
            } else {
                "PENDING"
            }
        );
        assert!(metadata["display_score"].is_null());
        assert_eq!(metadata["publication_qualified"], false);
        assert_eq!(metadata["renderer_file_unavailable"], missing_renderer);
        assert_eq!(
            expected_path.join("raw-report.json").exists(),
            !missing_report
        );
        assert!(!expected_path.join("output.svg").exists());
    }
    let parsed = crate::Cli::try_parse_from(["jeryu-split", "audit-package", "--trusted", "true"]);
    assert!(parsed.is_err());
}
