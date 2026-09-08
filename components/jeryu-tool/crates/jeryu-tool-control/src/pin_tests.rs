use super::*;

fn canonical() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::read_to_string(root.join("tool-manifest.toml")).expect("canonical manifest")
}

#[test]
fn manifest_schema_and_shell_fields_are_closed() {
    let text = canonical();
    Pin::parse(&text).expect("canonical pin");

    let unknown_top = text.replacen(
        "schema_version = \"1\"",
        "schema_version = \"1\"\nunknown_top = \"forbidden\"",
        1,
    );
    assert!(Pin::parse(&unknown_top).is_err());

    let unknown_pin = text.replacen(
        "[jankurai]\n",
        "[jankurai]\nunknown_pin = \"forbidden\"\n",
        1,
    );
    assert!(Pin::parse(&unknown_pin).is_err());

    let executable = text.replacen(
        "semver                = \"1.6.11\"",
        "semver                = \"$(touch /tmp/forbidden)\"",
        1,
    );
    assert!(Pin::parse(&executable).is_err());

    let breakout = text.replacen(
        "build_mode            = \"oci-vendor-locked-offline-workspace-member-v2\"",
        r#"build_mode            = "oci-vendor-locked-offline-workspace-member-v2\"; forbidden; echo \""#,
        1,
    );
    assert!(Pin::parse(&breakout).is_err());
}
