use super::*;

fn fixture_context() -> RenderContext {
    RenderContext {
        authority: ManifestAuthority {
            commit: "a".repeat(40),
            tree: "b".repeat(40),
            sha256: "c".repeat(64),
        },
        image_receipt_sha256: "d".repeat(64),
    }
}

#[test]
fn sandbox_receipt_is_v2_path_and_authority_bound() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pin = Pin::load(&root).expect("canonical pin");
    let context = fixture_context();
    let text = sandbox_receipt(&pin, &context.authority).expect("receipt");
    let receipt: serde_json::Value = serde_json::from_str(&text).expect("receipt JSON");
    assert_eq!(receipt["schema"], "jeryu.jankurai-installation/v2");
    assert_eq!(receipt["binary"]["sha256"], pin.get("binary_sha256"));
    assert_eq!(receipt["build"]["mode"], pin.get("build_mode"));
    assert_eq!(receipt["installation"]["path"], SANDBOX_JANKURAI_PATH);
    assert_eq!(receipt["governance"]["manifest_commit"], "a".repeat(40));
    assert_eq!(receipt["test_mode"], false);
}

#[test]
fn targeted_constants_and_receipt_paths_are_replaced() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pin = Pin::load(&root).expect("canonical pin");
    let context = fixture_context();
    let rust = "const TAG: &str = \"old\";\nconst IMAGE_RECEIPT_SHA256: &str =\n    \"old\";\n";
    let rendered = replace_governance_test_constants(rust, &pin, &context);
    assert!(rendered.contains(pin.get("tag")));
    assert!(rendered.contains(&context.image_receipt_sha256));

    let path = format!("{SANDBOX_RECEIPT_ROOT}/{}.json", "0".repeat(64));
    assert_eq!(
        replace_receipt_path(&path, &context.image_receipt_sha256),
        format!(
            "{SANDBOX_RECEIPT_ROOT}/{}.json",
            context.image_receipt_sha256
        )
    );
}

#[test]
fn pin_block_replacement_is_in_place_and_idempotent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pin = Pin::load(&root).expect("canonical pin");
    let stale = pin
        .shell_block()
        .replace(pin.get("version"), "jankurai 0.0.0");
    let input = format!(
        "#!/usr/bin/env bash\nsource before-generated.sh\n\n{stale}\n\necho after-generated\n"
    );

    let once = replace_pin_block(&input, &pin).expect("first render");
    let twice = replace_pin_block(&once, &pin).expect("second render");
    assert_eq!(once, twice);
    assert_eq!(once.matches(PIN_MARKER_BEGIN).count(), 1);
    assert_eq!(once.matches(PIN_MARKER_END).count(), 1);
    assert!(once.contains("source before-generated.sh\n\n"));
    assert!(once.ends_with("\n\necho after-generated\n"));
    assert!(
        once.find("source before-generated.sh").unwrap() < once.find(PIN_MARKER_BEGIN).unwrap()
    );
    assert!(once.find(PIN_MARKER_END).unwrap() < once.find("echo after-generated").unwrap());

    let unmarked = "#!/usr/bin/env bash\nset -euo pipefail\necho retained\n";
    let inserted = replace_pin_block(unmarked, &pin).expect("insert missing block");
    assert!(inserted.contains(&format!("set -euo pipefail\n\n{}", pin.shell_block())));
    assert!(inserted.ends_with("\necho retained\n"));
    assert_eq!(
        replace_pin_block(&inserted, &pin).expect("render inserted block again"),
        inserted
    );

    let unowned = "#!/bin/sh\necho no-pin-consumer\n";
    assert_eq!(
        replace_pin_block(unowned, &pin).expect("leave unowned shell unchanged"),
        unowned
    );
}

#[test]
fn pin_block_replacement_rejects_ambiguous_or_malformed_shell() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pin = Pin::load(&root).expect("canonical pin");
    let block = pin.shell_block();
    let malformed = [
        format!("#!/bin/sh\nset -euo pipefail\n{PIN_MARKER_BEGIN}\n"),
        format!("#!/bin/sh\nset -euo pipefail\n{PIN_MARKER_END}\n{PIN_MARKER_BEGIN}\n"),
        format!("#!/bin/sh\nset -euo pipefail\n{block}\n{block}\n"),
        "#!/bin/sh\nset -euo pipefail\nset -euo pipefail\n".to_owned(),
    ];

    for input in malformed {
        assert!(
            replace_pin_block(&input, &pin).is_err(),
            "accepted {input:?}"
        );
    }
}

#[test]
fn jankurai_wrapper_executes_only_the_verified_governed_binary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let function = require_function(&root).expect("governed verifier template");
    assert_eq!(function.matches("type -P -- jankurai").count(), 2);
    assert!(!function.contains("command -v jankurai"));

    let legacy = r#"readonly JERYU_JANKURAI_BIN="${CARGO_HOME}/bin/jankurai"

jankurai() {
  require_jankurai || return 1
  command "${JERYU_JANKURAI_BIN}" "$@"
}
"#;
    let rendered = bind_jankurai_wrapper(legacy).expect("bind legacy wrapper");
    assert!(rendered.contains(r#"readonly JERYU_JANKURAI_BIN="${CARGO_HOME}/bin/jankurai""#));
    assert!(rendered.contains(r#"command "${JERYU_GOVERNED_JANKURAI_BIN}" "$@""#));
    assert!(!rendered.contains(r#"command "${JERYU_JANKURAI_BIN}" "$@""#));
    assert_eq!(
        bind_jankurai_wrapper(&rendered).expect("canonical wrapper is idempotent"),
        rendered
    );

    let without_wrapper = "require_jankurai\necho no-wrapper\n";
    let rendered_without_wrapper =
        bind_jankurai_wrapper(without_wrapper).expect("bind consumer without wrapper");
    assert_eq!(rendered_without_wrapper.matches("jankurai() {").count(), 1);
    assert!(rendered_without_wrapper.ends_with(&format!("\n{CANONICAL_JANKURAI_WRAPPER}\n")));
    assert_eq!(
        bind_jankurai_wrapper(&rendered_without_wrapper).expect("generated wrapper is idempotent"),
        rendered_without_wrapper
    );
}

#[cfg(unix)]
#[test]
fn executable_lookup_ignores_the_wrapper_function_but_not_path_files() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "jeryu-tool-wrapper-resolution-{}-{nonce}",
        std::process::id()
    ));
    let governed_dir = root.join("governed");
    let alternate_dir = root.join("alternate");
    fs::create_dir_all(&governed_dir).expect("create governed fixture directory");
    fs::create_dir_all(&alternate_dir).expect("create alternate fixture directory");
    let governed = governed_dir.join("jankurai");
    let alternate = alternate_dir.join("jankurai");
    fs::write(
        &governed,
        "#!/usr/bin/env bash\nprintf 'jankurai test 1.0\\n'\n",
    )
    .expect("write governed executable");
    fs::write(
        &alternate,
        "#!/usr/bin/env bash\nprintf 'alternate executable\\n'\n",
    )
    .expect("write alternate executable");
    fs::set_permissions(&governed, fs::Permissions::from_mode(0o755))
        .expect("chmod governed executable");
    fs::set_permissions(&alternate, fs::Permissions::from_mode(0o755))
        .expect("chmod alternate executable");

    let probe = r#"set -euo pipefail
jankurai() { printf 'wrapper function\n'; }
test "$(command -v jankurai)" = jankurai
PATH="${ALTERNATE_DIR}:${PATH}"
bin="${GOVERNED_BIN}"
bin_dir="$(dirname "${bin}")"
export PATH="${bin_dir}:${PATH}"
resolved="$(type -P -- jankurai 2>/dev/null || true)"
test "${resolved}" = "${bin}"
test "$("${resolved}" --version)" = 'jankurai test 1.0'
test "$(sha256sum "${resolved}" | awk '{print $1}')" = \
  '67e807be848534d864c993ae5192d2526e3c2ca5cb83d7ed18c4b5b2600e1782'
"#;
    let output = Command::new("bash")
        .arg("-c")
        .arg(probe)
        .env("GOVERNED_BIN", &governed)
        .env("ALTERNATE_DIR", &alternate_dir)
        .output();
    fs::remove_dir_all(&root).expect("remove wrapper-resolution fixture");
    let output = output.expect("run wrapper-resolution probe");
    assert!(
        output.status.success(),
        "wrapper-resolution probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn jankurai_wrapper_rejects_ambiguous_or_unbound_execution() {
    let malformed = [
        r#"jankurai() {
  command jankurai "$@"
}
"#
        .to_owned(),
        r#"jankurai() {
  require_jankurai
  command "${OTHER_JANKURAI_BIN}" "$@"
}
"#
        .to_owned(),
        r#"jankurai() {
  require_jankurai
  echo bypass
  command "${JERYU_GOVERNED_JANKURAI_BIN}" "$@"
}
"#
        .to_owned(),
        format!(
            "{wrapper}\n{wrapper}",
            wrapper = r#"jankurai() {
  require_jankurai
  command "${JERYU_GOVERNED_JANKURAI_BIN}" "$@"
}"#
        ),
    ];

    for input in malformed {
        assert!(
            bind_jankurai_wrapper(&input).is_err(),
            "accepted ambiguous wrapper {input:?}"
        );
    }
}
