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
        "schema_version = \"2\"",
        "schema_version = \"2\"\nunknown_top = \"forbidden\"",
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

fn parsed_canonical() -> toml::Value {
    toml::from_str(&canonical()).expect("canonical TOML")
}

fn parse_value(manifest: &toml::Value) -> Result<Pin, String> {
    Pin::parse(&toml::to_string(manifest).expect("manifest TOML"))
}

#[test]
fn public_distribution_preserves_legacy_pin_and_generated_identity() {
    let public = parsed_canonical();
    assert_eq!(public["schema_version"].as_str(), Some("2"));
    assert_eq!(
        public["distribution"]["source_repository"].as_str(),
        Some(PUBLIC_SOURCE_REPOSITORY)
    );
    let public_pin = parse_value(&public).expect("public transport manifest");
    let mut legacy = public.clone();
    legacy["schema_version"] = toml::Value::String("1".to_owned());
    legacy
        .as_table_mut()
        .expect("manifest")
        .remove("distribution");
    let legacy_pin = parse_value(&legacy).expect("legacy manifest remains supported");

    assert_eq!(PIN_ENV_FIELDS.len(), 26);
    for (_, field) in PIN_ENV_FIELDS {
        assert_eq!(public_pin.get(field), legacy_pin.get(field), "{field}");
    }
    assert_eq!(public_pin.env_text(), legacy_pin.env_text());
    assert_eq!(public_pin.shell_block(), legacy_pin.shell_block());
    assert_eq!(
        public_pin.get("repo"),
        "http://127.0.0.1:8787/git/jeryu/jankurai.git"
    );
}

#[test]
fn distribution_schema_rejects_missing_extra_and_mistyped_fields() {
    let public = parsed_canonical();
    let mut legacy_with_distribution = public.clone();
    legacy_with_distribution["schema_version"] = toml::Value::String("1".to_owned());
    assert!(parse_value(&legacy_with_distribution).is_err());

    let mut missing = public.clone();
    missing
        .as_table_mut()
        .expect("manifest")
        .remove("distribution");
    assert!(parse_value(&missing).is_err());

    let mut extra = public.clone();
    extra["distribution"]
        .as_table_mut()
        .expect("distribution")
        .insert("fallback".to_owned(), toml::Value::Boolean(true));
    assert!(parse_value(&extra).is_err());

    for value in [
        toml::Value::String("not a table".to_owned()),
        toml::Value::Table(toml::Table::new()),
    ] {
        let mut malformed = public.clone();
        malformed["distribution"] = value;
        assert!(parse_value(&malformed).is_err());
    }
    let mut non_string = public.clone();
    non_string["distribution"]["source_repository"] = toml::Value::Boolean(true);
    assert!(parse_value(&non_string).is_err());

    for version in [toml::Value::String("3".to_owned()), toml::Value::Integer(2)] {
        let mut unknown = public.clone();
        unknown["schema_version"] = version;
        assert!(parse_value(&unknown).is_err());
    }
}

#[test]
fn public_distribution_rejects_transport_substitution_and_producer_changes() {
    for repository in [
        "http://github.com/neverhuman/jankurai.git",
        "http://127.0.0.1:8787/git/jeryu/jankurai.git",
        "https://github.com.evil.invalid/neverhuman/jankurai.git",
        "https://github.com/another-owner/jankurai.git",
        "https://user@github.com/neverhuman/jankurai.git",
        "https://github.com:443/neverhuman/jankurai.git",
        "https://github.com/neverhuman/jankurai.git/",
        "https://github.com/neverhuman/jankurai.git?ref=main",
        "https://github.com/neverhuman/jankurai.git#main",
        "file:///tmp/jankurai.git",
        "$(touch /tmp/forbidden)",
        "",
    ] {
        let mut manifest = parsed_canonical();
        manifest["distribution"]["source_repository"] = toml::Value::String(repository.to_owned());
        assert!(parse_value(&manifest).is_err(), "accepted {repository:?}");
    }

    let mut replaced_producer = parsed_canonical();
    replaced_producer["jankurai"]["repo"] =
        toml::Value::String(PUBLIC_SOURCE_REPOSITORY.to_owned());
    assert!(parse_value(&replaced_producer).is_err());
}
