use std::path::Path;
use std::process::Command;
#[cfg(unix)]
use std::{
    os::unix::fs::PermissionsExt,
    time::{SystemTime, UNIX_EPOCH},
};

fn command(root: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_jeryu-toolctl"))
        .args(["--tool-root", root.to_str().expect("UTF-8 test root")])
        .args(arguments)
        .output()
        .expect("run jeryu-toolctl")
}

#[test]
fn registry_check_and_closed_arguments() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = command(&root, &["registry-summary", "--check"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("registry ok: "));
    assert!(output.stderr.is_empty());

    let rejected = command(&root, &["registry-summary", "--unknown"]);
    assert!(!rejected.status.success());
    assert!(rejected.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(stderr.starts_with("registry-summary accepts only --check\n"));
    assert!(stderr.contains("purpose: validate the canonical reusable-tool registry"));
    assert!(stderr.contains("reason: registry or task input violated the closed schema"));
    assert!(stderr.contains(
        "common_fixes: fix the named field|remove the duplicate id|correct the status or task reference"
    ));
    assert!(stderr.contains("docs_url: docs/tools-registry.md"));
    assert!(stderr.contains(
        "repair_hint: run ops/registry-summary.sh --check after correcting the named input"
    ));
}

#[test]
fn emitted_verifier_is_bound_to_the_static_template() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = command(&root, &["emit-ensure-script"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let verifier = String::from_utf8(output.stdout).expect("UTF-8 verifier");
    let template = std::fs::read_to_string(root.join("ops/render-assets/require-jankurai.sh"))
        .expect("static verifier template");
    assert!(verifier.contains(template.trim_end()));
    assert!(verifier.contains("/opt/jain-ci/authority/release-bin/jankurai"));
    assert!(verifier.contains("# BEGIN GENERATED JANKURAI PIN"));
}

#[cfg(unix)]
#[test]
fn hosted_askpass_is_path_only_and_prompt_bound() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let fixture_root =
        std::env::temp_dir().join(format!("jeryu-tool-askpass-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&fixture_root).expect("create askpass fixture");
    let token_file = fixture_root.join("token");
    std::fs::write(&token_file, "fixture-hosted-token\n").expect("write fixture token");
    std::fs::set_permissions(&token_file, std::fs::Permissions::from_mode(0o600))
        .expect("set fixture token mode");

    let accepted = Command::new(env!("CARGO_BIN_EXE_jeryu-toolctl"))
        .env("JERYU_TOOL_GIT_ASKPASS", "1")
        .env("JERYU_FORGE_TOKEN_FILE", &token_file)
        .arg("Password for 'https://git@git.neverhuman.org/git/jeryu/jeryu-tool.git': ")
        .output()
        .expect("run hosted askpass");
    assert!(accepted.status.success());
    assert_eq!(accepted.stdout, b"fixture-hosted-token\n");
    assert!(accepted.stderr.is_empty());

    let rejected = Command::new(env!("CARGO_BIN_EXE_jeryu-toolctl"))
        .env("JERYU_TOOL_GIT_ASKPASS", "1")
        .env("JERYU_FORGE_TOKEN_FILE", &token_file)
        .arg("Password for 'https://git@git.neverhuman.org/git/veox/jeryu-tool.git': ")
        .output()
        .expect("run hostile askpass");
    assert!(!rejected.status.success());
    assert!(rejected.stdout.is_empty());
    assert!(rejected.stderr.is_empty());

    std::fs::remove_dir_all(fixture_root).expect("remove askpass fixture");
}

#[cfg(target_os = "linux")]
#[test]
fn candidate_renderer_binds_explicit_roots_to_the_detected_checkout() {
    // Execute the actual selector, replacing only Git discovery and the final
    // renderer dispatch. Empty physical directories exercise realpath checks.
    let script = r#"
tool_root=$1 router=$2
source "$tool_root/ops/test-scratch.sh"
umask 077
test_root=$(mktemp -d -t jeryu-render-selector.XXXXXXXX)
record_test_scratch "$test_root"
cleanup() {
  local status=$?
  trap - EXIT
  if (( status != 0 )); then
    printf 'retained renderer selector fixture: %s\n' "$test_root" >&2
    exit "$status"
  fi
  remove_test_scratch || status=1
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP
fixture="$test_root/monorepo"
fixture_tool="$fixture/components/jeryu-tool"
mkdir -p "$fixture_tool/ops/ci" "$test_root/unrelated/components/jeryu-tool"
export JERYU_MONOREPO_CANDIDATE=1
export JERYU_MONOREPO_EXPECTED_HEAD=1111111111111111111111111111111111111111
run_case() {
  local wanted=$1 expected_output=$2 output status=0
  shift 2
  output=$(bash -euo pipefail -c '
    router=$1 fixture=$2
    fixture_tool="$fixture/components/jeryu-tool"
    shift 2
    dirname() {
      [[ $# == 2 && $1 == -- && $2 == "$router" ]] || return 91
      printf "%s/ops/ci\n" "$fixture_tool"
    }
    git() {
      [[ $# == 4 && $1 == -C && $2 == "$fixture_tool" &&
         $3 == rev-parse && $4 == --show-toplevel ]] || return 92
      printf "%s\n" "$fixture"
    }
    exec() {
      [[ $# == 5 && $1 == bash ]] || return 93
      if [[ $2 == "$fixture_tool/ops/render-monorepo-candidate.sh" ]]; then
        [[ $3 == --check && $4 == --expected-head &&
           $5 == "$JERYU_MONOREPO_EXPECTED_HEAD" ]] || return 94
        printf "candidate\n"
      else
        [[ $2 == "$fixture_tool/ops/render-tool-manifest.sh" &&
           $3 == --check && $4 == --repo && $5 == jeryu-tool ]] || return 95
        printf "protected\n"
      fi
      exit "${RENDERER_EXIT:-0}"
    }
    source "$router" "$@"
  ' renderer-selector "$router" "$fixture" "$@") || status=$?
  [[ $status == "$wanted" && $output == "$expected_output" ]] || {
    printf 'selector expected status=%s output=%s; actual status=%s output=%s\n' \
      "$wanted" "$expected_output" "$status" "$output" >&2
    return 1
  }
}
unset root
run_case 0 candidate --check --repo jeryu-tool --repo-root "jeryu-tool=$fixture_tool"
run_case 0 candidate --check --repo jeryu --repo-root "jeryu=$fixture"
# Ambient state must neither reject a valid detected root nor admit another.
export root="$test_root/unrelated"
run_case 0 candidate --check --repo jeryu-tool --repo-root "jeryu-tool=$fixture_tool"
run_case 1 '' --check --repo jeryu-tool --repo-root "jeryu-tool=$root/components/jeryu-tool"
unset root
run_case 1 '' --check --repo jeryu --repo-root "jeryu=$fixture/../monorepo"
run_case 1 '' --check --repo-root "jeryu-tool=$fixture_tool"
run_case 1 '' --check --repo jeryu-tool --repo-root "jeryu-tool=$fixture_tool" \
  --repo-root "jeryu-tool=$fixture_tool"
export RENDERER_EXIT=23
run_case 23 candidate --check --repo jeryu-tool --repo-root "jeryu-tool=$fixture_tool"
unset RENDERER_EXIT
export JAIN_RELEASE_CI=1
run_case 1 '' --check --repo jeryu-tool --repo-root "jeryu-tool=$fixture_tool"
unset JAIN_RELEASE_CI
export JERYU_MONOREPO_CANDIDATE=0
run_case 0 protected --check --repo jeryu-tool
printf 'Renderer selector: 10 synthetic dispatch cases passed.\n'
"#;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("bash")
        .args(["-euo", "pipefail", "-c", script, "renderer-selector-test"])
        .arg(&root)
        .arg(root.join("ops/ci/check-rendered-identity.sh"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run actual candidate renderer selector");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
