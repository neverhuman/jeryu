use crate::pin::{
    PIN_ENV_FIELDS, PIN_MARKER_BEGIN, PIN_MARKER_END, Pin, WORKFLOW_PIN_MARKER_BEGIN,
    WORKFLOW_PIN_MARKER_END,
};
use regex::{Captures, Regex};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const MANIFEST_REPO: &str = "http://127.0.0.1:8787/git/jeryu/jeryu-tool.git";
const SANDBOX_JANKURAI_PATH: &str = "/opt/rust/cargo/bin/jankurai";
const SANDBOX_RECEIPT_ROOT: &str = "/opt/jeryu/receipts/jankurai/sha256";
const CANONICAL_JANKURAI_WRAPPER: &str = r#"jankurai() {
  require_jankurai || return 1
  command "${JERYU_GOVERNED_JANKURAI_BIN}" "$@"
}"#;

#[derive(Debug, Clone)]
pub(crate) struct ManifestAuthority {
    pub commit: String,
    pub tree: String,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub(crate) struct RenderContext {
    pub authority: ManifestAuthority,
    pub image_receipt_sha256: String,
}

pub(crate) fn sandbox_receipt(pin: &Pin, authority: &ManifestAuthority) -> Result<String, String> {
    let receipt = serde_json::json!({
        "binary": {
            "sha256": pin.get("binary_sha256"),
            "version_output": pin.get("version"),
        },
        "build": {
            "builder_image": pin.get("builder_image"),
            "builder_image_id": pin.get("builder_image_id"),
            "capabilities_dropped": true,
            "cargo": pin.get("cargo_version"),
            "cargo_config_sha256": pin.get("cargo_config_sha256"),
            "cargo_net_offline": true,
            "closed_vendor": true,
            "command": pin.get("build_command"),
            "container_engine_path": "/usr/bin/docker",
            "context_sha256": pin.get("build_context_sha256"),
            "environment": pin.get("build_environment"),
            "git_global_config_disabled": true,
            "git_http_follow_redirects": false,
            "git_system_config_disabled": true,
            "git_terminal_prompt": false,
            "glibc": pin.get("glibc_version"),
            "jankurai_update_check": false,
            "linker": pin.get("linker_version"),
            "mode": pin.get("build_mode"),
            "network_none": true,
            "network_scope": "local-forge-source-plus-closed-vendor-network-none",
            "no_new_privileges": true,
            "no_proxy": "127.0.0.1,localhost,::1",
            "non_root": true,
            "package_path": pin.get("package_path"),
            "read_only_root": true,
            "rustc": pin.get("rustc_version"),
            "rustflags": pin.get("rustflags"),
            "target_triple": pin.get("target_triple"),
            "vendor_file_count": pin.get("vendor_file_count"),
            "vendor_files_sha256": pin.get("vendor_files_sha256"),
        },
        "conclusion": "success",
        "governance": {
            "manifest_commit": authority.commit,
            "manifest_repo": MANIFEST_REPO,
            "manifest_sha256": authority.sha256,
            "manifest_tree": authority.tree,
            "protected_main": true,
            "protection_policy": "immutable-main-v1",
            "status": "governed",
        },
        "installation": {
            "atomic": true,
            "path": SANDBOX_JANKURAI_PATH,
        },
        "operator": "jeryu-agent-sandbox-build",
        "run_id": format!("agent-sandbox-jankurai-{}", pin.get("semver")),
        "schema": "jeryu.jankurai-installation/v2",
        "source": {
            "archive_sha256": pin.get("source_archive_sha256"),
            "cargo_lock_sha256": pin.get("cargo_lock_sha256"),
            "commit": pin.get("rev"),
            "remote": pin.get("repo"),
            "tag": pin.get("tag"),
            "tree": pin.get("source_tree"),
            "verification": "release-authoritative",
        },
        "test_mode": false,
    });
    serde_json::to_string_pretty(&receipt)
        .map(|text| format!("{text}\n"))
        .map_err(|error| format!("failed to render sandbox Jankurai receipt: {error}"))
}

pub(crate) fn require_function(tool_root: &Path) -> Result<String, String> {
    let path = tool_root.join("ops/render-assets/require-jankurai.sh");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    if !text.starts_with("require_jankurai() {\n") || !text.ends_with("}\n") {
        return Err(format!(
            "invalid governed Jankurai function template: {}",
            path.display()
        ));
    }
    Ok(text.trim_end().to_owned())
}

pub(crate) fn ensure_script(pin: &Pin, function: &str) -> String {
    format!(
        concat!(
            "#!/usr/bin/env bash\n",
            "# GENERATED Jankurai verifier. Installation is owned only by jeryu-tool.\n",
            "set -euo pipefail\n\n",
            "{}\n\n",
            "{function}\n\n",
            "require_jankurai\n",
            "printf 'governed jankurai ok: %s sha256=%s at %s\\n' \\\n",
            "  \"${{JERYU_JANKURAI_VERSION}}\" \"${{JERYU_JANKURAI_SHA256}}\" ",
            "\"${{JERYU_GOVERNED_JANKURAI_BIN}}\"\n",
        ),
        pin.shell_block(),
        function = function,
    )
}

#[path = "render_rules_rewrite.rs"]
mod rewrite;
use rewrite::{
    bind_jankurai_wrapper, remove_install_step, replace_ci_bridge_constants,
    replace_governance_test_constants, replace_hex_on_marked_line, replace_native_tool_block,
    replace_pin_block, replace_receipt_path, replace_require_function, replace_workflow_pin,
};
pub(crate) use rewrite::{regex, semantic_identity_rules};

pub(crate) fn render_consumer(
    path: &Path,
    pin: &Pin,
    function: &str,
    context: &RenderContext,
) -> Result<String, String> {
    let mut text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let rel = path.to_string_lossy();
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if name == "jankurai-installation-receipt.json" && rel.contains("agent-sandbox") {
        return sandbox_receipt(pin, &context.authority);
    }
    if name == "test-governed-jankurai.sh" {
        text = replace_hex_on_marked_line(
            &text,
            ".governance.manifest_commit",
            40,
            &context.authority.commit,
        );
        text = replace_hex_on_marked_line(
            &text,
            ".governance.manifest_tree",
            40,
            &context.authority.tree,
        );
        return Ok(replace_hex_on_marked_line(
            &text,
            ".governance.manifest_sha256",
            64,
            &context.authority.sha256,
        ));
    }
    if name == "ensure-jankurai.sh" {
        return Ok(ensure_script(pin, function));
    }
    if name == "lib.sh" && rel.contains("/ops/ci/") {
        let rendered = replace_require_function(&replace_pin_block(&text, pin)?, function);
        let rendered = bind_jankurai_wrapper(&rendered)?;
        return Ok(semantic_identity_rules(&rendered, pin));
    }
    if path.extension().and_then(|value| value.to_str()) == Some("sh") {
        text = replace_pin_block(&text, pin)?;
    }
    if matches!(name, "audit-policy.toml" | "default-audit-policy.toml") {
        return Ok(regex(r#"(required_tool_version\s*=\s*")[^"]*(")"#)
            .replace_all(&text, format!("${{1}}{}${{2}}", pin.get("semver")))
            .into_owned());
    }
    if name == "ci-lanes.toml" {
        return Ok(regex(r#"(jankurai_version\s*=\s*")[^"]*(")"#)
            .replace_all(&text, format!("${{1}}{}${{2}}", pin.get("version")))
            .into_owned());
    }
    if name == "native-cli-manifest.toml" {
        text = replace_native_tool_block(&text, pin);
    }
    if name == "ci_bridge.rs" {
        text = regex(r#"(required_tool_version\s*=\s*\\?")[^"\\]*(\\?")"#)
            .replace_all(&text, format!("${{1}}{}${{2}}", pin.get("semver")))
            .into_owned();
        text = replace_ci_bridge_constants(&text, pin, context);
    }
    if name == "jankurai_governance.rs" {
        return Ok(replace_governance_test_constants(&text, pin, context));
    }
    if name == "release.md" {
        text = regex(r"(?ms)(match SHA-256\s*`)[0-9a-f]{64}(`)")
            .replace_all(&text, format!("${{1}}{}${{2}}", pin.get("binary_sha256")))
            .into_owned();
    }
    if name.ends_with(".yml") && rel.contains("/workflows/") && text.contains("JANKURAI_") {
        text = replace_workflow_pin(&text, pin);
        text = regex(
            r"(?m)^      - name: Cache pinned auditor\n        uses:.*\n        with:\n          path:.*\n          key:.*\n",
        )
        .replace_all(&text, "")
        .into_owned();
        text = remove_install_step(&text);
    }
    if name == "Dockerfile" && rel.contains("agent-sandbox") {
        for (key, manifest_key) in PIN_ENV_FIELDS {
            let value = serde_json::to_string(pin.get(manifest_key)).expect("string JSON");
            let argument = regex(&format!(r"(?m)^ARG {}=.*$", regex::escape(key)));
            if argument.is_match(&text) {
                text = argument
                    .replace_all(&text, format!("ARG {key}={value}"))
                    .into_owned();
            } else {
                text = regex(r"(?m)^(ARG JANKURAI_VERSION=.*)$")
                    .replacen(&text, 1, |capture: &Captures<'_>| {
                        format!("{}\nARG {key}={value}", &capture[1])
                    })
                    .into_owned();
            }
        }
    }
    if name == "README.md" && rel.contains("agent-sandbox") {
        text = regex(r"\(\*\*\d+\.\d+\.\d+\*\*, (?:rev|identity)-locked\)")
            .replace_all(
                &text,
                format!("(**{}**, identity-locked)", pin.get("semver")),
            )
            .into_owned();
    }
    if name == "smoke.sh" && rel.contains("/agent-sandbox/") {
        text = regex(r"pinned \d+\.\d+\.\d+")
            .replace_all(&text, format!("pinned {}", pin.get("semver")))
            .into_owned();
        text = replace_hex_on_marked_line(
            &text,
            "sha256sum /opt/rust/cargo/bin/jankurai",
            64,
            pin.get("binary_sha256"),
        );
    }
    if name == "pr-ci.sh" {
        text = regex(r"(?ms)^# The pinned jankurai .*?^export PATH=.*?\n")
            .replace_all(&text, |_: &Captures<'_>| {
                "# Resolve and verify the absolute governed auditor before any lane runs.\n\
source ops/ci/lib.sh\nrequire_jankurai\n"
                    .to_owned()
            })
            .into_owned();
        text = regex(r#"(?m)^export PATH="\$\{CARGO_HOME:-\$HOME/\.cargo\}/bin:\$PATH"\n?"#)
            .replace_all(&text, "")
            .into_owned();
        if !regex(r"(?m)^\s*require_jankurai(?:\s|$)").is_match(&text) {
            text = text.replacen(
                "cd \"$repo_root\"\n",
                "cd \"$repo_root\"\n\n# Resolve and verify the absolute governed auditor before any lane runs.\n\
source ops/ci/lib.sh\nrequire_jankurai\n",
                1,
            );
        }
        text = text.replace(
            "  bash \"$JERYU_TOOL_RENDER\" --check\n",
            "  consumer_repo=\"$(awk -F'\\\"' '/^workspace =/ {print $2; exit}' agent/audit-policy.toml)\"\n\
  bash \"$JERYU_TOOL_RENDER\" --check --repo \"$consumer_repo\" \\\n\
    --repo-root \"$consumer_repo=$repo_root\"\n",
        );
        if !text.contains("JERYU_TOOL_RENDER=") {
            let block = "# Verify only this PR worktree against the family manifest; the post-rollout\n\
# control-plane check verifies the complete canonical family.\n\
JERYU_TOOL_RENDER=\"${JERYU_TOOL_RENDER:-$repo_root/../jeryu-tool/ops/render-tool-manifest.sh}\"\n\
if [ -x \"$JERYU_TOOL_RENDER\" ]; then\n\
  consumer_repo=\"$(awk -F'\\\"' '/^workspace =/ {print $2; exit}' agent/audit-policy.toml)\"\n\
  bash \"$JERYU_TOOL_RENDER\" --check --repo \"$consumer_repo\" \\\n\
    --repo-root \"$consumer_repo=$repo_root\"\n\
fi\n\n";
            text = text.replacen(
                "echo \"[pr-ci] (jobs=$JOBS) standard lanes\" >&2\n",
                &format!("{block}echo \"[pr-ci] (jobs=$JOBS) standard lanes\" >&2\n"),
                1,
            );
        }
    }
    if name == "ci-doctor.sh" && !text.contains("require_jankurai") {
        text = text.replacen(
            &format!("{PIN_MARKER_END}\n"),
            &format!(
                "{PIN_MARKER_END}\n\nsource \"$(cd \"$(dirname \"${{BASH_SOURCE[0]}}\")/..\" \
&& pwd)/ops/ci/lib.sh\"\nrequire_jankurai\n"
            ),
            1,
        );
    }
    text = replace_receipt_path(&text, &context.image_receipt_sha256);
    Ok(semantic_identity_rules(&text, pin))
}

#[cfg(test)]
#[path = "render_rules_tests.rs"]
mod tests;
