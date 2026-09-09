#![cfg(target_os = "linux")]

use serde_json::{Value, json};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

fn tool_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("physical Tool root")
}

// These invented identities only exercise the shared JSON predicate. They never
// enter an installer, execute an auditor, or become installation evidence.
struct ReceiptFixture {
    renderer: Value,
    pin: Value,
    receipt: Value,
}

impl ReceiptFixture {
    fn new() -> Self {
        let renderer = json!({
            "source": {"repository":"https://source.fixture.invalid/jeryu.git", "commit":"1".repeat(40), "tree":"2".repeat(40)},
            "distribution": {"repository":"https://source.fixture.invalid/jankurai.git"},
            "manifest": {"sha256":"3".repeat(64)},
        });
        let pin = json!({
            "JANKURAI_REPO":"https://producer.fixture.invalid/jankurai.git",
            "JANKURAI_TAG":"synthetic-contract-fixture",
            "JANKURAI_REV":"4".repeat(40), "JANKURAI_SOURCE_TREE":"5".repeat(40),
            "JANKURAI_SOURCE_ARCHIVE_SHA256":"6".repeat(64),
            "JANKURAI_CARGO_LOCK_SHA256":"7".repeat(64),
            "JANKURAI_BINARY_SHA256":"8".repeat(64),
            "JANKURAI_VERSION":"synthetic auditor; never executable",
            "JANKURAI_RUSTC_VERSION":"synthetic rustc", "JANKURAI_CARGO_VERSION":"synthetic cargo",
            "JANKURAI_TARGET_TRIPLE":"synthetic target", "JANKURAI_BUILD_MODE":"synthetic build",
            "JANKURAI_PACKAGE_PATH":"synthetic package", "JANKURAI_BUILDER_IMAGE":"synthetic image",
            "JANKURAI_BUILDER_IMAGE_ID":"synthetic image identity",
            "JANKURAI_LINKER_VERSION":"synthetic linker", "JANKURAI_GLIBC_VERSION":"synthetic libc",
            "JANKURAI_VENDOR_FILES_SHA256":"9".repeat(64), "JANKURAI_VENDOR_FILE_COUNT":"1",
            "JANKURAI_CARGO_CONFIG_SHA256":"a".repeat(64),
            "JANKURAI_BUILD_ENVIRONMENT":"synthetic environment", "JANKURAI_RUSTFLAGS":"synthetic flags",
            "JANKURAI_BUILD_COMMAND":"synthetic command; never executed",
            "JANKURAI_BUILD_CONTEXT_SHA256":"b".repeat(64),
        });
        let mut build = json!({
            "cargo_net_offline":true, "closed_vendor":true, "network_none":true,
            "read_only_root":true, "non_root":true, "capabilities_dropped":true,
            "no_new_privileges":true, "container_engine_path":"/usr/bin/docker",
            "git_global_config_disabled":true, "git_system_config_disabled":true,
            "git_http_follow_redirects":false, "git_terminal_prompt":false,
            "jankurai_update_check":false,
            "network_scope":"public-source-fetch-plus-closed-vendor-network-none", "no_proxy":"",
        });
        for (field, input) in [
            ("rustc", "RUSTC_VERSION"),
            ("cargo", "CARGO_VERSION"),
            ("target_triple", "TARGET_TRIPLE"),
            ("mode", "BUILD_MODE"),
            ("package_path", "PACKAGE_PATH"),
            ("builder_image", "BUILDER_IMAGE"),
            ("builder_image_id", "BUILDER_IMAGE_ID"),
            ("linker", "LINKER_VERSION"),
            ("glibc", "GLIBC_VERSION"),
            ("vendor_files_sha256", "VENDOR_FILES_SHA256"),
            ("vendor_file_count", "VENDOR_FILE_COUNT"),
            ("cargo_config_sha256", "CARGO_CONFIG_SHA256"),
            ("environment", "BUILD_ENVIRONMENT"),
            ("rustflags", "RUSTFLAGS"),
            ("command", "BUILD_COMMAND"),
            ("context_sha256", "BUILD_CONTEXT_SHA256"),
        ] {
            build[field] = pin[format!("JANKURAI_{input}")].clone();
        }
        let receipt = json!({
            "schema":"jeryu.jankurai-public-candidate-installation/v1",
            "timestamp":"2000-01-01T00:00:00Z", "operator":"synthetic test fixture",
            "run_id":"contract-test-only", "test_mode":false, "conclusion":"success",
            "renderer_metadata":renderer, "renderer_metadata_sha256":"fixture-renderer-sha",
            "verification":{"build":"verified", "installation":"verified", "public_readback":"verified"},
            "inputs":{
                "pin":{"path":"components/jeryu-tool/generated/jankurai-pin.env", "blob":"fixture-pin-blob", "sha256":"fixture-pin-sha"},
                "builder":{"path":"components/jeryu-tool/ops/build-jankurai-hermetic.sh", "blob":"fixture-builder-blob", "sha256":"fixture-builder-sha"},
            },
            "source":{
                "remote":renderer["distribution"]["repository"], "producer_repository":pin["JANKURAI_REPO"],
                "tag":pin["JANKURAI_TAG"], "commit":pin["JANKURAI_REV"], "tree":pin["JANKURAI_SOURCE_TREE"],
                "archive_sha256":pin["JANKURAI_SOURCE_ARCHIVE_SHA256"], "cargo_lock_sha256":pin["JANKURAI_CARGO_LOCK_SHA256"],
                "verification":"public-candidate",
            },
            "governance":{
                "status":"public-candidate", "manifest_repo":renderer["source"]["repository"],
                "manifest_commit":renderer["source"]["commit"], "manifest_tree":renderer["source"]["tree"],
                "manifest_sha256":renderer["manifest"]["sha256"], "protected_main":false,
                "protection_policy":"not-applicable", "handover":"pending", "predecessor_authentication":"not-performed",
            },
            "binary":{"sha256":pin["JANKURAI_BINARY_SHA256"], "version_output":pin["JANKURAI_VERSION"]},
            "installation":{
                "path":"/synthetic-test-install/bin/jankurai", "atomic":true,
                "previous_binary_sha256":"", "rollback_artifact":"",
                "lock":{"path":"/synthetic-test-install/.jankurai-install.lock", "identity":"fixture-lock-inode",
                    "exclusive":true, "held_through_receipt":true},
            },
            "build":build,
        });
        Self {
            renderer,
            pin,
            receipt,
        }
    }

    fn evaluate(&self, receipt: &Value) -> Output {
        let mut command = Command::new("jq");
        command.env_clear().env("PATH", "/usr/bin:/bin");
        command.args(["-e", "--argjson", "renderer", &self.renderer.to_string()]);
        command.args(["--argjson", "pin", &self.pin.to_string()]);
        for (name, value) in [
            ("renderer_sha", "fixture-renderer-sha"),
            ("pin_blob", "fixture-pin-blob"),
            ("pin_sha", "fixture-pin-sha"),
            ("builder_blob", "fixture-builder-blob"),
            ("builder_sha", "fixture-builder-sha"),
            ("binary_path", "/synthetic-test-install/bin/jankurai"),
            ("install_root", "/synthetic-test-install"),
            ("lock_identity", "fixture-lock-inode"),
        ] {
            command.args(["--arg", name, value]);
        }
        let mut child = command
            .arg("-f")
            .arg(tool_root().join("ops/public-candidate-receipt.jq"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("required jq executable");
        child
            .stdin
            .take()
            .expect("predicate input pipe")
            .write_all(receipt.to_string().as_bytes())
            .expect("write synthetic predicate input");
        child.wait_with_output().expect("wait for predicate")
    }

    fn assert_refused(&self, receipt: &Value, attack: &str) {
        let output = self.evaluate(receipt);
        assert_eq!(
            output.status.code(),
            Some(1),
            "predicate must reject {attack} with false, not a jq/tool error: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

#[test]
fn candidate_receipt_binds_source_build_installation_and_rollback() {
    let fixture = ReceiptFixture::new();
    check(fixture.evaluate(&fixture.receipt));
    for (pointer, replacement) in [
        (
            "/source/remote",
            json!("https://substitute.fixture.invalid"),
        ),
        (
            "/source/producer_repository",
            json!("https://substitute.fixture.invalid"),
        ),
        ("/source/commit", json!("0".repeat(40))),
        ("/source/archive_sha256", json!("0".repeat(64))),
        ("/source/cargo_lock_sha256", json!("0".repeat(64))),
        ("/governance/manifest_commit", json!("0".repeat(40))),
        ("/governance/manifest_sha256", json!("0".repeat(64))),
        ("/renderer_metadata/source/tree", json!("0".repeat(40))),
        ("/renderer_metadata_sha256", json!("0".repeat(64))),
        ("/inputs/pin/blob", json!("0".repeat(40))),
        ("/inputs/builder/sha256", json!("0".repeat(64))),
        ("/binary/sha256", json!("0".repeat(64))),
        ("/binary/version_output", json!("different version")),
        ("/build/builder_image_id", json!("different image")),
        ("/build/context_sha256", json!("0".repeat(64))),
        ("/build/vendor_files_sha256", json!("0".repeat(64))),
        ("/build/network_none", json!(false)),
        ("/build/cargo_net_offline", json!("true")),
        ("/build/git_http_follow_redirects", json!(true)),
        ("/build/container_engine_path", json!("/tmp/docker")),
        ("/installation/path", json!("/other-install/bin/jankurai")),
        ("/installation/atomic", json!(false)),
        ("/installation/lock/identity", json!("substituted inode")),
        ("/installation/lock/held_through_receipt", json!(false)),
        ("/verification/public_readback", json!("not-performed")),
    ] {
        let mut mutated = fixture.receipt.clone();
        *mutated.pointer_mut(pointer).expect("existing attack field") = replacement;
        fixture.assert_refused(&mutated, pointer);
    }
    let digest = "c".repeat(64);
    let mut rollback = fixture.receipt.clone();
    rollback["installation"]["previous_binary_sha256"] = json!(digest);
    fixture.assert_refused(&rollback, "missing rollback artifact");
    rollback["installation"]["rollback_artifact"] = json!(format!(
        "/synthetic-test-install/rollback/jankurai/{digest}"
    ));
    check(fixture.evaluate(&rollback));
    rollback["installation"]["rollback_artifact"] =
        json!(format!("/other-install/rollback/jankurai/{digest}"));
    fixture.assert_refused(&rollback, "foreign rollback root");
}

#[test]
fn candidate_receipt_has_closed_fields_and_cannot_claim_other_authority() {
    let fixture = ReceiptFixture::new();
    for pointer in [
        "",
        "/source",
        "/build",
        "/governance",
        "/binary",
        "/installation",
        "/installation/lock",
        "/inputs",
        "/inputs/pin",
        "/inputs/builder",
        "/verification",
    ] {
        let mut unknown = fixture.receipt.clone();
        unknown
            .pointer_mut(pointer)
            .expect("object under test")
            .as_object_mut()
            .expect("closed receipt object")
            .insert("unreviewed_override".into(), json!(true));
        fixture.assert_refused(&unknown, &format!("unknown field at {pointer}"));
        let object = fixture
            .receipt
            .pointer(pointer)
            .unwrap()
            .as_object()
            .unwrap();
        for key in object.keys() {
            let mut missing = fixture.receipt.clone();
            missing
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            fixture.assert_refused(&missing, &format!("missing {pointer}/{key}"));
        }
    }
    for (pointer, replacement) in [
        ("/schema", json!("jeryu.jankurai-installation/v2")),
        ("/test_mode", json!(true)),
        ("/governance/status", json!("production")),
        ("/governance/protected_main", json!(true)),
        ("/governance/handover", json!("complete")),
        ("/governance/predecessor_authentication", json!("verified")),
    ] {
        let mut mutated = fixture.receipt.clone();
        *mutated.pointer_mut(pointer).unwrap() = replacement;
        fixture.assert_refused(&mutated, pointer);
    }
}

const CUSTODY_FIXTURE: &str = r#"
umask 077
test_root=$(mktemp -d /tmp/jeryu-candidate-verifier-test.XXXXXXXX)
test_identity=$(stat -c '%d:%i:%u' "$test_root")
cleanup() {
  local result=$? mount_point link target
  [[ -d $test_root && ! -L $test_root && $(realpath -e "$test_root") == "$test_root" &&
    $(stat -c '%d:%i:%u' "$test_root") == "$test_identity" ]] || exit 1
  while read -r _ _ _ _ mount_point _; do
    printf -v mount_point '%b' "$mount_point"
    [[ $mount_point != "$test_root" && $mount_point != "$test_root/"* ]] || exit 1
  done </proc/self/mountinfo
  while IFS= read -r -d '' link; do
    target=$(readlink -- "$link")
    [[ $target == "$test_root/"* && $target != *'/../'* && $target != */.. ]] || exit 1
  done < <(find "$test_root" -xdev -type l -print0)
  rm -rf --one-file-system --preserve-root=all -- "$test_root"
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT TERM HUP
source "$1/ops/verify-public-candidate.sh"
declare -a JERYU_CANDIDATE_HELD_FDS=() candidate_paths=() candidate_identities=()
mkdir "$test_root/inputs"
printf 'original fixture bytes\n' > "$test_root/inputs/evidence"
refused() {
  local reason=$1
  shift
  if ( "$@" ) > "$test_root/refusal.log" 2>&1; then
    printf 'unexpected verifier success: %s\n' "$reason" >&2
    exit 1
  fi
  grep -F -- "$reason" "$test_root/refusal.log" >/dev/null || {
    cat "$test_root/refusal.log" >&2
    exit 1
  }
}
"#;

fn check(output: Output) {
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn custody(script: &str) {
    check(
        Command::new("bash")
            .args(["-euo", "pipefail", "-c"])
            .arg(format!("{CUSTODY_FIXTURE}\n{script}"))
            .arg("candidate-verifier-hostile-test")
            .arg(tool_root())
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("run verifier custody fixture"),
    );
}

#[test]
fn candidate_inputs_refuse_link_aliases_and_shared_writes() {
    custody(
        r#"
input="$test_root/inputs/evidence"
ln -s "$input" "$test_root/file-link"
ln -s "$test_root/inputs" "$test_root/parent-link"
ln -s "$test_root/absent" "$test_root/dangling-link"
refused 'physical absolute regular file' jeryu_candidate_open_file "$test_root/file-link" selected
refused 'physical absolute regular file' jeryu_candidate_open_file "$test_root/parent-link/evidence" selected
refused 'physical absolute regular file' jeryu_candidate_open_file "$test_root/dangling-link" selected
ln "$input" "$test_root/hard-link"
refused 'owner-held and single-link' jeryu_candidate_open_file "$input" selected
rm "$test_root/hard-link"
chmod 660 "$input"
refused 'writable by another identity' jeryu_candidate_open_file "$input" selected
chmod 600 "$input"
chmod 770 "$test_root/inputs"
refused 'writable input parent' jeryu_candidate_open_file "$input" selected
chmod 700 "$test_root/inputs"
jeryu_candidate_open_file "$input" selected
[[ $(cat "/proc/$BASHPID/fd/$selected") == 'original fixture bytes' ]]
jeryu_candidate_recheck_files
"#,
    );
}

#[test]
fn retained_descriptor_reads_original_after_replacement_but_custody_refuses_it() {
    custody(
        r#"
input="$test_root/inputs/evidence"
jeryu_candidate_open_file "$input" selected
process=$BASHPID
held="/proc/$process/fd/$selected"
jeryu_candidate_recheck_files
mv "$input" "$test_root/inputs/original"
printf 'substituted fixture bytes\n' > "$input"
[[ $(cat "$held") == 'original fixture bytes' ]]
[[ $(cat "$input") == 'substituted fixture bytes' ]]
refused 'input custody changed during verification' jeryu_candidate_recheck_files
rm "$input"
mv "$test_root/inputs/original" "$input"
jeryu_candidate_recheck_files
ln "$input" "$test_root/additional-link"
refused 'input custody changed during verification' jeryu_candidate_recheck_files
rm "$test_root/additional-link"
chmod 660 "$input"
refused 'input custody changed during verification' jeryu_candidate_recheck_files
chmod 600 "$input"
chmod 770 "$test_root/inputs"
refused 'writable input parent' jeryu_candidate_recheck_files
chmod 700 "$test_root/inputs"
jeryu_candidate_recheck_files
# Inode custody is distinct from content: require_public_candidate_jankurai
# also hashes the held bytes. Do not claim the identity helper detects writes.
before=$(sha256sum "$held" | cut -d' ' -f1)
printf 'same-inode content mutation\n' > "$input"
jeryu_candidate_recheck_files
[[ $(sha256sum "$held" | cut -d' ' -f1) != "$before" ]]
"#,
    );
}

#[test]
fn candidate_verifier_rejects_release_and_test_authority_before_inputs() {
    custody(
        r#"
unset JERYU_MONOREPO_CANDIDATE JAIN_RELEASE_CI JERYU_JANKURAI_ALLOW_TEST_RECEIPT JERYU_INSTALL_TEST_MODE
refused 'candidate mode cannot satisfy release-broker or test authority' require_public_candidate_jankurai
export JERYU_MONOREPO_CANDIDATE=1
for selector in JAIN_RELEASE_CI JERYU_JANKURAI_ALLOW_TEST_RECEIPT JERYU_INSTALL_TEST_MODE; do
  printf -v "$selector" '%s' 1
  export "$selector"
  refused 'candidate mode cannot satisfy release-broker or test authority' require_public_candidate_jankurai
  unset "$selector"
done
# Use the actual caller identity. Never replace id(1) to simulate admission.
if [[ $(id -u) == 0 ]]; then
  refused 'candidate CI must run as an unprivileged user' require_public_candidate_jankurai
else
  refused 'exact candidate source commit is required' require_public_candidate_jankurai
fi
"#,
    );
}
