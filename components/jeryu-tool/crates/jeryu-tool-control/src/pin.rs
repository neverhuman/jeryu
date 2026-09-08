use regex::Regex;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[path = "pin_fields.rs"]
mod fields;
use fields::{FLOOR_FIELDS, TOOL_FIELDS, TOP_LEVEL_FIELDS, exact_keys, shell_double_quote_safe};
pub use fields::{
    PIN_ENV_FIELDS, PIN_MARKER_BEGIN, PIN_MARKER_END, WORKFLOW_PIN_MARKER_BEGIN,
    WORKFLOW_PIN_MARKER_END,
};

#[derive(Debug, Clone)]
pub struct Pin(BTreeMap<String, String>);

impl Pin {
    pub fn load(tool_root: &Path) -> Result<Self, String> {
        let manifest = tool_root.join("tool-manifest.toml");
        let text = fs::read_to_string(&manifest)
            .map_err(|error| format!("failed to read {}: {error}", manifest.display()))?;
        Self::parse(&text)
    }

    fn parse(text: &str) -> Result<Self, String> {
        let parsed: toml::Value = toml::from_str(text)
            .map_err(|error| format!("failed to parse tool-manifest.toml: {error}"))?;
        let top = parsed
            .as_table()
            .ok_or_else(|| "tool-manifest.toml must contain a top-level table".to_owned())?;
        exact_keys(top, &TOP_LEVEL_FIELDS, "top-level")?;
        if top.get("schema_version").and_then(toml::Value::as_str) != Some("1") {
            return Err("tool-manifest.toml schema_version must be \"1\"".to_owned());
        }

        let table = top
            .get("jankurai")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| "tool-manifest.toml missing [jankurai]".to_owned())?;
        let pin_fields: Vec<&str> = PIN_ENV_FIELDS.iter().map(|(_, field)| *field).collect();
        exact_keys(table, &pin_fields, "[jankurai]")?;
        let mut fields = BTreeMap::new();
        for (_, key) in PIN_ENV_FIELDS {
            match table.get(key).and_then(toml::Value::as_str) {
                Some(value) if shell_double_quote_safe(value) => {
                    fields.insert(key.to_owned(), value.to_owned());
                }
                _ => {
                    return Err(format!(
                        "tool-manifest.toml [jankurai].{key} must be a non-empty, non-executable string"
                    ));
                }
            }
        }

        let floors = top
            .get("floors")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| "tool-manifest.toml missing [floors]".to_owned())?;
        exact_keys(floors, &FLOOR_FIELDS, "[floors]")?;
        for field in FLOOR_FIELDS {
            let value = floors
                .get(field)
                .and_then(toml::Value::as_integer)
                .ok_or_else(|| format!("tool-manifest.toml [floors].{field} must be an integer"))?;
            if !(0..=100).contains(&value) {
                return Err(format!(
                    "tool-manifest.toml [floors].{field} must be between 0 and 100"
                ));
            }
        }

        let tools = top
            .get("tools")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| "tool-manifest.toml missing [tools]".to_owned())?;
        exact_keys(tools, &TOOL_FIELDS, "[tools]")?;
        for field in TOOL_FIELDS {
            let value = tools
                .get(field)
                .and_then(toml::Value::as_str)
                .ok_or_else(|| format!("tool-manifest.toml [tools].{field} must be a string"))?;
            if !matches!(value, "required" | "advisory" | "disabled") {
                return Err(format!(
                    "tool-manifest.toml [tools].{field} has invalid mode {value:?}"
                ));
            }
        }

        let pin = Self(fields);
        pin.validate()?;
        Ok(pin)
    }

    fn validate(&self) -> Result<(), String> {
        if self.get("repo") != "http://127.0.0.1:8787/git/jeryu/jankurai.git" {
            return Err(
                "Jankurai release source must be the approved local Jeryu forge URL".to_owned(),
            );
        }
        let sha = Regex::new("^[0-9a-f]{40}$").expect("constant regex");
        for key in ["rev", "source_tree"] {
            if !sha.is_match(self.get(key)) {
                return Err(format!(
                    "invalid {key}: expected 40 lowercase hexadecimal characters"
                ));
            }
        }
        let digest = Regex::new("^[0-9a-f]{64}$").expect("constant regex");
        for key in [
            "source_archive_sha256",
            "cargo_lock_sha256",
            "binary_sha256",
            "vendor_files_sha256",
            "cargo_config_sha256",
            "build_context_sha256",
        ] {
            if !digest.is_match(self.get(key)) {
                return Err(format!("invalid {key}: expected SHA-256"));
            }
        }
        let semver = Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+$").expect("constant regex");
        if !semver.is_match(self.get("semver")) {
            return Err("invalid semver: expected MAJOR.MINOR.PATCH".to_owned());
        }
        if self.get("version") != format!("jankurai {}", self.get("semver")) {
            return Err("version must equal \"jankurai <semver>\"".to_owned());
        }
        let tag = Regex::new(r"^v([0-9]+\.[0-9]+\.[0-9]+)-deadlang-precision-split\.[1-9][0-9]*$")
            .expect("constant regex");
        let tag_semver = tag
            .captures(self.get("tag"))
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str());
        if tag_semver != Some(self.get("semver")) {
            return Err(
                "tag must be v<semver>-deadlang-precision-split.<positive integer>".to_owned(),
            );
        }
        if !semver.is_match(self.get("rust_toolchain")) {
            return Err("invalid rust_toolchain: expected MAJOR.MINOR.PATCH".to_owned());
        }
        let tool_version = Regex::new(
            r"^(rustc|cargo) ([0-9]+\.[0-9]+\.[0-9]+) \([0-9a-f]{9} [0-9]{4}-[0-9]{2}-[0-9]{2}\)$",
        )
        .expect("constant regex");
        let rustc = tool_version
            .captures(self.get("rustc_version"))
            .ok_or_else(|| "invalid rustc_version".to_owned())?;
        if rustc.get(1).map(|value| value.as_str()) != Some("rustc")
            || rustc.get(2).map(|value| value.as_str()) != Some(self.get("rust_toolchain"))
        {
            return Err("rustc_version must match rust_toolchain".to_owned());
        }
        let cargo = tool_version
            .captures(self.get("cargo_version"))
            .ok_or_else(|| "invalid cargo_version".to_owned())?;
        if cargo.get(1).map(|value| value.as_str()) != Some("cargo") {
            return Err("invalid cargo_version".to_owned());
        }
        let target = Regex::new(r"^[a-z0-9_]+-[a-z0-9_]+-[a-z0-9_]+(?:-[a-z0-9_]+)?$")
            .expect("constant regex");
        if !target.is_match(self.get("target_triple")) {
            return Err("invalid target_triple".to_owned());
        }
        if self.get("build_mode") != "oci-vendor-locked-offline-workspace-member-v2" {
            return Err("invalid build_mode".to_owned());
        }
        if self.get("package_path") != "crates/jankurai" {
            return Err("invalid package_path".to_owned());
        }
        let image = Regex::new(r"^rust@sha256:[0-9a-f]{64}$").expect("constant regex");
        if !image.is_match(self.get("builder_image")) {
            return Err("invalid builder_image".to_owned());
        }
        let image_id = Regex::new(r"^sha256:[0-9a-f]{64}$").expect("constant regex");
        if !image_id.is_match(self.get("builder_image_id")) {
            return Err("invalid builder_image_id".to_owned());
        }
        if self.get("builder_image").strip_prefix("rust@") != Some(self.get("builder_image_id")) {
            return Err("builder_image must resolve to builder_image_id".to_owned());
        }
        if self.get("linker_version") != "GNU ld (GNU Binutils for Debian) 2.40" {
            return Err("invalid linker_version".to_owned());
        }
        if self.get("glibc_version") != "ldd (Debian GLIBC 2.36-9+deb12u14) 2.36" {
            return Err("invalid glibc_version".to_owned());
        }
        let file_count = self
            .get("vendor_file_count")
            .parse::<usize>()
            .map_err(|_| "invalid vendor_file_count".to_owned())?;
        if file_count == 0 {
            return Err("vendor_file_count must be positive".to_owned());
        }
        if self.get("build_environment")
            != "CARGO_NET_OFFLINE=true,HOME=/tmp,LANG=C,LC_ALL=C,SOURCE_DATE_EPOCH=0,TZ=UTC"
        {
            return Err("invalid build_environment".to_owned());
        }
        if self.get("rustflags")
            != "--remap-path-prefix=/opt/jeryu/jankurai=/jankurai-build/source \
--remap-path-prefix=/opt/jeryu/vendor=/jankurai-build/vendor \
--remap-path-prefix=/opt/jeryu/target=/jankurai-build/target \
--remap-path-prefix=/usr/local/cargo=/jankurai-build/cargo"
        {
            return Err("invalid rustflags".to_owned());
        }
        if self.get("build_command")
            != "cargo install --locked --offline --path \
/opt/jeryu/jankurai/crates/jankurai --root /opt/jeryu/out --bin jankurai"
        {
            return Err("invalid build_command".to_owned());
        }
        Ok(())
    }

    pub fn get(&self, key: &str) -> &str {
        self.0.get(key).map_or("", String::as_str)
    }

    pub fn env_text(&self) -> String {
        let mut lines = vec![
            "# GENERATED by ops/render-tool-manifest.sh from tool-manifest.toml — DO NOT EDIT."
                .to_owned(),
            "# This binds source, build environment, and installed binary identity.".to_owned(),
        ];
        lines.extend(
            PIN_ENV_FIELDS
                .iter()
                .map(|(env, key)| format!("{env}=\"{}\"", self.get(key))),
        );
        format!("{}\n", lines.join("\n"))
    }

    pub fn shell_block(&self) -> String {
        [
            PIN_MARKER_BEGIN.to_owned(),
            format!("export JERYU_JANKURAI_SOURCE_REPO=\"{}\"", self.get("repo")),
            format!("export JERYU_JANKURAI_VERSION=\"{}\"", self.get("version")),
            format!(
                "export JERYU_JANKURAI_SHA256=\"{}\"",
                self.get("binary_sha256")
            ),
            format!("export JERYU_JANKURAI_SOURCE_REV=\"{}\"", self.get("rev")),
            format!("export JERYU_JANKURAI_SOURCE_TAG=\"{}\"", self.get("tag")),
            format!(
                "export JERYU_JANKURAI_SOURCE_TREE=\"{}\"",
                self.get("source_tree")
            ),
            format!(
                "export JERYU_JANKURAI_SOURCE_ARCHIVE_SHA256=\"{}\"",
                self.get("source_archive_sha256")
            ),
            format!(
                "export JERYU_JANKURAI_CARGO_LOCK_SHA256=\"{}\"",
                self.get("cargo_lock_sha256")
            ),
            format!(
                "export JERYU_JANKURAI_RUST_TOOLCHAIN=\"{}\"",
                self.get("rust_toolchain")
            ),
            format!(
                "export JERYU_JANKURAI_RUSTC_VERSION=\"{}\"",
                self.get("rustc_version")
            ),
            format!(
                "export JERYU_JANKURAI_CARGO_VERSION=\"{}\"",
                self.get("cargo_version")
            ),
            format!(
                "export JERYU_JANKURAI_TARGET_TRIPLE=\"{}\"",
                self.get("target_triple")
            ),
            format!(
                "export JERYU_JANKURAI_BUILD_MODE=\"{}\"",
                self.get("build_mode")
            ),
            format!(
                "export JERYU_JANKURAI_PACKAGE_PATH=\"{}\"",
                self.get("package_path")
            ),
            format!(
                "export JERYU_JANKURAI_BUILDER_IMAGE=\"{}\"",
                self.get("builder_image")
            ),
            format!(
                "export JERYU_JANKURAI_BUILDER_IMAGE_ID=\"{}\"",
                self.get("builder_image_id")
            ),
            format!(
                "export JERYU_JANKURAI_LINKER_VERSION=\"{}\"",
                self.get("linker_version")
            ),
            format!(
                "export JERYU_JANKURAI_GLIBC_VERSION=\"{}\"",
                self.get("glibc_version")
            ),
            format!(
                "export JERYU_JANKURAI_VENDOR_FILES_SHA256=\"{}\"",
                self.get("vendor_files_sha256")
            ),
            format!(
                "export JERYU_JANKURAI_VENDOR_FILE_COUNT=\"{}\"",
                self.get("vendor_file_count")
            ),
            format!(
                "export JERYU_JANKURAI_CARGO_CONFIG_SHA256=\"{}\"",
                self.get("cargo_config_sha256")
            ),
            format!(
                "export JERYU_JANKURAI_BUILD_ENVIRONMENT=\"{}\"",
                self.get("build_environment")
            ),
            format!(
                "export JERYU_JANKURAI_RUSTFLAGS=\"{}\"",
                self.get("rustflags")
            ),
            format!(
                "export JERYU_JANKURAI_BUILD_COMMAND=\"{}\"",
                self.get("build_command")
            ),
            format!(
                "export JERYU_JANKURAI_BUILD_CONTEXT_SHA256=\"{}\"",
                self.get("build_context_sha256")
            ),
            PIN_MARKER_END.to_owned(),
        ]
        .join("\n")
    }

    pub fn workflow_block(&self) -> String {
        let mut lines = vec![format!("  {WORKFLOW_PIN_MARKER_BEGIN}")];
        lines.extend(PIN_ENV_FIELDS.iter().map(|(name, key)| {
            format!(
                "  {name}: {}",
                serde_json::to_string(self.get(key)).expect("string JSON")
            )
        }));
        lines.push(format!("  {WORKFLOW_PIN_MARKER_END}"));
        lines.join("\n")
    }
}

#[cfg(test)]
#[path = "pin_tests.rs"]
mod tests;
