use std::collections::BTreeSet;

pub const PIN_MARKER_BEGIN: &str = "# BEGIN GENERATED JANKURAI PIN — DO NOT EDIT";
pub const PIN_MARKER_END: &str = "# END GENERATED JANKURAI PIN";
pub const WORKFLOW_PIN_MARKER_BEGIN: &str = "# BEGIN GENERATED JANKURAI WORKFLOW PIN — DO NOT EDIT";
pub const WORKFLOW_PIN_MARKER_END: &str = "# END GENERATED JANKURAI WORKFLOW PIN";

pub const PIN_ENV_FIELDS: [(&str, &str); 26] = [
    ("JANKURAI_REPO", "repo"),
    ("JANKURAI_TAG", "tag"),
    ("JANKURAI_REV", "rev"),
    ("JANKURAI_VERSION", "version"),
    ("JANKURAI_SEMVER", "semver"),
    ("JANKURAI_SOURCE_TREE", "source_tree"),
    ("JANKURAI_SOURCE_ARCHIVE_SHA256", "source_archive_sha256"),
    ("JANKURAI_CARGO_LOCK_SHA256", "cargo_lock_sha256"),
    ("JANKURAI_BINARY_SHA256", "binary_sha256"),
    ("JANKURAI_RUST_TOOLCHAIN", "rust_toolchain"),
    ("JANKURAI_RUSTC_VERSION", "rustc_version"),
    ("JANKURAI_CARGO_VERSION", "cargo_version"),
    ("JANKURAI_TARGET_TRIPLE", "target_triple"),
    ("JANKURAI_BUILD_MODE", "build_mode"),
    ("JANKURAI_PACKAGE_PATH", "package_path"),
    ("JANKURAI_BUILDER_IMAGE", "builder_image"),
    ("JANKURAI_BUILDER_IMAGE_ID", "builder_image_id"),
    ("JANKURAI_LINKER_VERSION", "linker_version"),
    ("JANKURAI_GLIBC_VERSION", "glibc_version"),
    ("JANKURAI_VENDOR_FILES_SHA256", "vendor_files_sha256"),
    ("JANKURAI_VENDOR_FILE_COUNT", "vendor_file_count"),
    ("JANKURAI_CARGO_CONFIG_SHA256", "cargo_config_sha256"),
    ("JANKURAI_BUILD_ENVIRONMENT", "build_environment"),
    ("JANKURAI_RUSTFLAGS", "rustflags"),
    ("JANKURAI_BUILD_COMMAND", "build_command"),
    ("JANKURAI_BUILD_CONTEXT_SHA256", "build_context_sha256"),
];

pub(super) const TOP_LEVEL_FIELDS: [&str; 4] = ["schema_version", "jankurai", "floors", "tools"];
pub(super) const TOP_LEVEL_FIELDS_V2: [&str; 5] = [
    "schema_version",
    "jankurai",
    "floors",
    "tools",
    "distribution",
];
pub(super) const PUBLIC_SOURCE_REPOSITORY: &str = "https://github.com/neverhuman/jankurai.git";
pub(super) const FLOOR_FIELDS: [&str; 4] =
    ["default", "public-portal", "jeryu-ci-runner", "jeryu-tool"];
pub(super) const TOOL_FIELDS: [&str; 20] = [
    "audit-ci",
    "security",
    "git-bad-behavior",
    "ci-bad-behavior",
    "release-bad-behavior",
    "proof-routing",
    "proofbind",
    "proofmark-rust",
    "copy-code",
    "contract-drift",
    "rust-witness",
    "ux-qa",
    "db-migration-analyze",
    "coverage-evidence",
    "vibe-coverage",
    "authz-matrix",
    "input-boundary",
    "agent-tool-supply",
    "release-readiness",
    "cost-budget",
];

pub(super) fn exact_keys(
    table: &toml::Table,
    expected: &[&str],
    context: &str,
) -> Result<(), String> {
    let actual: BTreeSet<&str> = table.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = expected.iter().copied().collect();
    if actual != expected {
        let missing: Vec<&str> = expected.difference(&actual).copied().collect();
        let unknown: Vec<&str> = actual.difference(&expected).copied().collect();
        return Err(format!(
            "tool-manifest.toml {context} key set mismatch: missing={missing:?} unknown={unknown:?}"
        ));
    }
    Ok(())
}

pub(super) fn shell_double_quote_safe(value: &str) -> bool {
    !value.is_empty()
        && !value
            .chars()
            .any(|character| matches!(character, '"' | '\\' | '$' | '`' | '\n' | '\r' | '\0'))
}
