use std::process::{Command, Output};

const PROGRAM: &str = "/fixture/ops/split/manifest.sh";

fn run_manifest(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jeryu-split"))
        .env("JERYU_SPLIT_MANIFEST_PROGRAM", PROGRAM)
        .arg("manifest")
        .args(args)
        .output()
        .expect("run manifest compatibility entrypoint")
}

#[test]
fn missing_manifest_value_preserves_legacy_diagnostic_and_status_one() {
    let output = run_manifest(&["--manifest"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn empty_manifest_value_preserves_legacy_diagnostic_and_status_one() {
    let output = run_manifest(&["--manifest", ""]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"manifest error: --manifest requires a path\n"
    );
}

#[test]
fn unknown_manifest_argument_preserves_legacy_usage_and_status_two() {
    for argument in ["--unknown", "--help"] {
        let output = run_manifest(&[argument]);

        assert_eq!(output.status.code(), Some(2), "argument={argument}");
        assert!(output.stdout.is_empty(), "argument={argument}");
        assert_eq!(
            output.stderr,
            format!("usage: {PROGRAM} [--manifest PATH] [--json] [--check-paths]\n").as_bytes(),
            "argument={argument}"
        );
    }
}

#[test]
fn flag_consumed_as_manifest_path_preserves_legacy_unreadable_error() {
    let output = run_manifest(&["--manifest", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"manifest error: manifest not readable: --json\n"
    );
}
