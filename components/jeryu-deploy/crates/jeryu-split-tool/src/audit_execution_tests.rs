//! Real executor confinement controls using an explicitly unqualified synthetic producer.
//! These tests exercise no Jankurai installation, receipt or release qualification.

use super::*;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const VERSION: &str = "fixture-unqualified";
const CHILD_TEST: &str = "JERYU_AUDIT_EXECUTOR_FIXTURE_ENTRY";

fn command(root: &Path, program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Executor Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Executor Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z");
    command
}

fn successful(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

// Reexecute this exact test with hostile process environment. This avoids
// process-global set_var/unsafe code and cannot race parallel Rust tests.
fn isolated(test_name: &str, test: impl FnOnce()) {
    if std::env::var(CHILD_TEST).as_deref() == Ok(test_name) {
        test();
        return;
    }
    let output = command(Path::new("/"), "/usr/bin/timeout")
        .args(["--signal=TERM", "--kill-after=5s", "40s"])
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", test_name, "--nocapture", "--test-threads=1"])
        .env(CHILD_TEST, test_name)
        .env("JERYU_EXECUTOR_HOSTILE", "must-not-reach-producer")
        .env("GIT_DIR", "/nonexistent-executor-hostile-git")
        .env("GIT_WORK_TREE", "/nonexistent-executor-hostile-tree")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.bare")
        .env("GIT_CONFIG_VALUE_0", "true")
        .env("BASH_ENV", "/nonexistent-executor-hostile-bashenv")
        .env("LD_LIBRARY_PATH", "/nonexistent-executor-hostile-libraries")
        .env("HOME", "/nonexistent-executor-hostile-home")
        .env("TMPDIR", "/nonexistent-executor-hostile-temp")
        .env("PATH", "/nonexistent-executor-hostile-path")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated executor fixture failed or timed out; scratch retained\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "exact child test was not executed"
    );
}

struct Fixture {
    temporary: PathBuf,
    root: PathBuf,
    out: PathBuf,
    auditor: PathBuf,
    binary_hash: String,
    head: String,
    tree: String,
    helper: String,
    identity: String,
}

impl Fixture {
    fn new(mode: &str) -> Self {
        assert!(matches!(mode, "success" | "timeout" | "failure"));
        // Deliberately beneath /tmp: the executor replaces /tmp and must remount
        // the complete parent Git repository, not only the component directory.
        let temporary = tempfile::Builder::new()
            .prefix("jeryu-audit-executor-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in("/tmp")
            .unwrap()
            .keep();
        eprintln!(
            "unqualified executor fixture retained until guarded success: {}",
            temporary.display()
        );
        let package = Path::new(env!("CARGO_MANIFEST_DIR"));
        let helper = [2, 4]
            .into_iter()
            .find_map(|depth| {
                let path = package.ancestors().nth(depth)?.join("tests/scratch.sh");
                path.is_file().then_some(path)
            })
            .expect("owning scratch custody helper");
        let helper = fs::read_to_string(helper).unwrap();
        let record = format!(
            "{helper}\njeryu_record_test_scratch \"$1\"\nprintf '%s' \"$jeryu_test_scratch_identity\"\n"
        );
        let identity = successful(
            command(&temporary, "/bin/bash")
                .args([
                    "-euo",
                    "pipefail",
                    "-c",
                    &record,
                    "executor-fixture",
                    temporary.to_str().unwrap(),
                ])
                .output()
                .unwrap(),
        );
        let root = temporary.join("repository");
        fs::write(
            temporary.join("unmounted-private"),
            b"outside the admitted source and output mounts\n",
        )
        .unwrap();
        let component = root.join("components/jeryu-jira");
        fs::create_dir_all(component.join("agent")).unwrap();
        fs::write(
            component.join("agent/audit-policy.toml"),
            "workspace = \"jeryu-jira\"\nminimum_score = 91\nhard_findings_allowed = 0\n",
        )
        .unwrap();
        fs::write(
            component.join("source-sentinel"),
            b"committed source remains unchanged\n",
        )
        .unwrap();
        fs::write(component.join("fixture-mode"), format!("{mode}\n")).unwrap();
        successful(
            command(&root, "/usr/bin/git")
                .args(["init", "--quiet", "--template=", "--initial-branch=main"])
                .output()
                .unwrap(),
        );
        successful(
            command(&root, "/usr/bin/git")
                .args(["add", "components"])
                .output()
                .unwrap(),
        );
        successful(
            command(&root, "/usr/bin/git")
                .args([
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "--quiet",
                    "-m",
                    "unqualified executor fixture",
                ])
                .output()
                .unwrap(),
        );
        let head = git(&root, &["rev-parse", "HEAD"]).unwrap();
        let tree = git(&root, &["rev-parse", "HEAD:components/jeryu-jira"]).unwrap();
        let out = temporary.join("evidence");
        fs::create_dir(&out).unwrap();
        let auditor = temporary.join("synthetic-producer");
        fs::write(&auditor, PRODUCER).unwrap();
        fs::set_permissions(&auditor, fs::Permissions::from_mode(0o500)).unwrap();
        Self {
            temporary,
            root,
            out,
            auditor,
            binary_hash: hash(PRODUCER.as_bytes()),
            head,
            tree,
            helper,
            identity,
        }
    }

    fn execute(&self, timeout_seconds: u64) -> (Row, Result<()>) {
        let mut row = Row {
            acquisition: None,
            source: Source {
                repository: "neverhuman/jeryu-jira".into(),
                scope: "component".into(),
                path: Some("components/jeryu-jira".into()),
                commit: None,
                minimum: 91,
                required: true,
                reason: None,
            },
            status: "not_executed",
            reason: String::new(),
            commit: None,
            tree: None,
            policy_sha256: None,
            governing_policy_sha256: None,
            command_exit: None,
            report_sha256: None,
            summary: None,
        };
        let executor = execution::Executor {
            root: &self.root,
            out: &self.out,
            auditor: &self.auditor,
            binary_hash: &self.binary_hash,
            version: VERSION,
            timeout_seconds,
            governing: None,
        };
        let result = execution::execute(&executor, &mut row, 0);
        (row, result)
    }

    fn verify_source_and_child_closure(&self) {
        assert!(
            git(
                &self.root,
                &["status", "--porcelain=v1", "--untracked-files=all"]
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(git(&self.root, &["rev-parse", "HEAD"]).unwrap(), self.head);
        assert_eq!(
            fs::read(self.root.join("components/jeryu-jira/source-sentinel")).unwrap(),
            b"committed source remains unchanged\n"
        );
        let attempt = self.out.join("0000");
        assert_eq!(
            fs::read(attempt.join("child.ready")).unwrap(),
            b"unqualified fixture child has acquired its lock\n"
        );
        // A surviving shell or sleep inherits this descriptor and keeps flock
        // held. Nonblocking reacquisition proves closure before scratch cleanup.
        let output = command(&self.temporary, "/usr/bin/flock")
            .arg("--nonblock")
            .arg(attempt.join("child.lock"))
            .arg("/usr/bin/true")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "producer descendant retained its lock; fixture must remain preserved"
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "retaining failed executor fixture: {}",
                self.temporary.display()
            );
            return;
        }
        let cleanup = format!(
            "{}\njeryu_test_scratch=\"$1\"\njeryu_test_scratch_identity=\"$2\"\njeryu_remove_test_scratch\n",
            self.helper
        );
        let output = command(Path::new("/"), "/bin/bash")
            .args([
                "-euo",
                "pipefail",
                "-c",
                &cleanup,
                "executor-fixture",
                self.temporary.to_str().unwrap(),
                &self.identity,
            ])
            .output()
            .expect("scratch cleanup command");
        assert!(
            output.status.success(),
            "guarded cleanup refused; retained {}: {}",
            self.temporary.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn component_git_source_is_read_only_and_environment_is_cleared() {
    isolated(
        "audit_census::execution_tests::component_git_source_is_read_only_and_environment_is_cleared",
        || {
            let f = Fixture::new("success");
            let (row, result) = f.execute(15);
            result.unwrap();
            assert_eq!(row.command_exit, Some(0));
            assert_eq!(row.commit.as_deref(), Some(f.head.as_str()));
            assert_eq!(row.tree.as_deref(), Some(f.tree.as_str()));
            assert!(row.summary.as_ref().unwrap().passed);
            // Local report success does not authenticate a governing predecessor.
            assert_eq!(row.status, "failed_policy");
            assert!(
                row.reason
                    .contains("authenticated protected governing policy")
            );
            let bytes = fs::read(f.out.join("0000/report.json")).unwrap();
            assert_eq!(row.report_sha256.as_deref(), Some(hash(&bytes).as_str()));
            let report: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(report["synthetic_fixture"], true);
            assert_eq!(report["auditor_version"], VERSION);
            assert_eq!(report["git"]["head"], f.head);
            f.verify_source_and_child_closure();
        },
    );
}

#[test]
fn timeout_after_passing_report_stays_timed_out_and_closes_descendants() {
    isolated(
        "audit_census::execution_tests::timeout_after_passing_report_stays_timed_out_and_closes_descendants",
        || {
            let f = Fixture::new("timeout");
            let started = Instant::now();
            let (row, result) = f.execute(2);
            result.unwrap();
            assert!(started.elapsed() < Duration::from_secs(20));
            assert_eq!(row.status, "timed_out");
            assert!(matches!(row.command_exit, Some(124 | 137)));
            assert!(row.summary.is_none());
            let report: Value =
                serde_json::from_slice(&fs::read(f.out.join("0000/report.json")).unwrap()).unwrap();
            assert_eq!(report["decision"]["passed"], true);
            assert_eq!(report["synthetic_fixture"], true);
            f.verify_source_and_child_closure();
        },
    );
}

#[test]
fn producer_nonzero_after_passing_report_cannot_be_admitted() {
    isolated(
        "audit_census::execution_tests::producer_nonzero_after_passing_report_cannot_be_admitted",
        || {
            let f = Fixture::new("failure");
            let (row, result) = f.execute(15);
            assert!(format!("{:#}", result.unwrap_err()).contains("producer reported success"));
            assert_eq!(row.command_exit, Some(42));
            assert!(row.summary.is_none());
            assert!(row.report_sha256.is_some());
            f.verify_source_and_child_closure();
        },
    );
}

const PRODUCER: &str = r#"#!/bin/bash
set -euo pipefail
# UNQUALIFIED SYNTHETIC FIXTURE: no source audit or receipt qualification occurs.
[[ $HOME == /tmp && $TMPDIR == /tmp && $PATH == /usr/bin:/bin ]]
for name in JERYU_AUDIT_EXECUTOR_FIXTURE_ENTRY JERYU_EXECUTOR_HOSTILE GIT_DIR GIT_WORK_TREE GIT_CONFIG_COUNT GIT_CONFIG_KEY_0 GIT_CONFIG_VALUE_0 BASH_ENV LD_LIBRARY_PATH; do
  [[ ! -v $name ]] || exit 71
done
[[ $1 == audit && $2 == . ]]
report= markdown= floor=
while (($#)); do
  case $1 in
    --json) report=$2; shift 2;;
    --md) markdown=$2; shift 2;;
    --fail-under) floor=$2; shift 2;;
    *) shift;;
  esac
done
[[ -n $report && -n $markdown && $floor == 91 ]]
parent=$(cd ../.. && pwd -P)
[[ ! -e ${parent%/*}/unmounted-private ]]
[[ $(/usr/bin/git -c core.fsmonitor=false rev-parse --show-toplevel) == "$parent" ]]
[[ -d $parent/.git ]]
head=$(/usr/bin/git -c core.fsmonitor=false rev-parse HEAD)
[[ $head =~ ^[0-9a-f]{40}$ ]]
if { printf 'attempted source mutation\n' > source-sentinel; } 2>/dev/null; then
  exit 72
fi
[[ $(/usr/bin/cat source-sentinel) == 'committed source remains unchanged' ]]
attempt=${report%/*}
(
  exec 9>"$attempt/child.lock"
  /usr/bin/flock --exclusive 9
  trap '' TERM
  printf 'unqualified fixture child has acquired its lock\n' > "$attempt/child.ready"
  /usr/bin/sleep 30
) &
while [[ ! -f $attempt/child.ready ]]; do /usr/bin/sleep 0.01; done
printf '{"standard":"jankurai","schema_version":"1.9.0","auditor_version":"fixture-unqualified","repo":".","synthetic_fixture":true,"score":95,"dirty_worktree":false,"scope":{"mode":"full","paths":[]},"git":{"head":"%s","mode":"full","dirty_worktree":false},"policy":{"minimum_score":91,"mode":"standard","path":"./agent/audit-policy.toml","fail_on":["critical","high"]},"decision":{"status":"pass","passed":true,"minimum_score":91,"hard_findings":0,"soft_findings":0,"ratchet":{"passed":true}},"conformance_decision":"pass","conformance_blockers":[],"findings":[],"caps_applied":[],"hard_findings":0}\n' "$head" > "$report"
printf 'Unqualified synthetic executor fixture; no source audit was performed.\n' > "$markdown"
case $(/usr/bin/cat fixture-mode) in
  success) exit 0;;
  failure) exit 42;;
  timeout) /usr/bin/sleep 30;;
  *) exit 73;;
esac
"#;
