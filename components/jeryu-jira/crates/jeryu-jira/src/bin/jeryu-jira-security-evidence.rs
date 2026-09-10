//! Convert the Work security lane's four-column TSV into its existing report schema.

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Serialize)]
struct Check<'a> {
    detail: &'a str,
    name: &'a str,
    policy: &'a str,
    status: &'a str,
}

#[derive(Serialize)]
struct Evidence<'a> {
    checks: Vec<Check<'a>>,
    schema_version: &'static str,
}

fn render(input: &str) -> Result<Vec<u8>> {
    // Match the former text reader's CRLF handling and splitlines boundaries.
    let normalized = input.replace("\r\n", "\n");
    let lines = normalized.split_terminator([
        '\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
        '\u{2029}',
    ]);
    let mut checks = Vec::new();
    for (index, line) in lines.enumerate() {
        let mut columns = line.splitn(4, '\t');
        let [Some(name), Some(status), Some(policy), Some(detail)] =
            std::array::from_fn(|_| columns.next())
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("security evidence row {} requires four columns", index + 1),
            )
            .into());
        };
        checks.push(Check {
            detail,
            name,
            policy,
            status,
        });
    }
    // Failed and not_run checks are data. The shell lane owns the gate decision.
    let mut output = serde_json::to_vec_pretty(&Evidence {
        checks,
        schema_version: "jeryu.split.security/v1",
    })?;
    output.push(b'\n');
    Ok(output)
}

fn write_evidence(input: &Path, output: &Path) -> Result<()> {
    // Read and validate the complete input before opening or truncating the report.
    let report = render(&fs::read_to_string(input)?)?;
    fs::write(output, report)?;
    Ok(())
}

fn paths(arguments: impl IntoIterator<Item = OsString>) -> Result<(PathBuf, PathBuf)> {
    let mut arguments = arguments.into_iter();
    match (arguments.next(), arguments.next(), arguments.next()) {
        (Some(input), Some(output), None) => Ok((input.into(), output.into())),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: jeryu-jira-security-evidence <checks.tsv> <evidence.json>",
        )
        .into()),
    }
}

fn main() -> Result<()> {
    let (input, output) = paths(std::env::args_os().skip(1))?;
    write_evidence(&input, &output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{File, Metadata};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    // The Work database test guard owns different filenames and cleans during
    // unwind. Keep this flat evidence fixture on failure instead.
    struct Fixture {
        root: PathBuf,
        held: File,
        identity: Metadata,
        cleaned: bool,
    }

    impl Fixture {
        fn new() -> Self {
            let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
            let root = tempfile::Builder::new()
                .prefix("jeryu-work-security-test-")
                .permissions(fs::Permissions::from_mode(0o700))
                .tempdir_in(parent)
                .unwrap()
                .keep();
            let held = File::open(&root).unwrap();
            let identity = held.metadata().unwrap();
            Self {
                root,
                held,
                identity,
                cleaned: false,
            }
        }

        fn path(&self) -> &Path {
            &self.root
        }

        fn check_root(&self) -> io::Result<()> {
            let current = fs::symlink_metadata(&self.root)?;
            let held = self.held.metadata()?;
            let identity = |m: &Metadata| (m.dev(), m.ino(), m.uid(), m.gid(), m.mode());
            if !current.is_dir()
                || current.file_type().is_symlink()
                || fs::canonicalize(&self.root)? != self.root
                || identity(&current) != identity(&self.identity)
                || identity(&held) != identity(&self.identity)
                || current.mode() & 0o7777 != 0o700
            {
                return Err(io::Error::other("evidence fixture root custody changed"));
            }
            Ok(())
        }

        fn cleanup(&mut self) -> io::Result<()> {
            if self.cleaned {
                return Ok(());
            }
            self.check_root()?;
            let root = self
                .root
                .to_str()
                .ok_or_else(|| io::Error::other("non-UTF-8 fixture root"))?
                .replace('\\', "\\134")
                .replace(' ', "\\040")
                .replace('\t', "\\011")
                .replace('\n', "\\012");
            for line in fs::read_to_string("/proc/self/mountinfo")?.lines() {
                let mount = line
                    .split_whitespace()
                    .nth(4)
                    .ok_or_else(|| io::Error::other("malformed mount inventory"))?;
                if mount == root || mount.starts_with(&format!("{root}/")) {
                    return Err(io::Error::other("evidence fixture contains a mount"));
                }
            }
            let mut files = Vec::new();
            for entry in fs::read_dir(&self.root)? {
                let entry = entry?;
                let metadata = fs::symlink_metadata(entry.path())?;
                if !matches!(
                    entry.file_name().to_str(),
                    Some("checks.tsv" | "evidence.json")
                ) || !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata.dev() != self.identity.dev()
                    || metadata.uid() != self.identity.uid()
                    || metadata.gid() != self.identity.gid()
                    || metadata.nlink() != 1
                {
                    return Err(io::Error::other(
                        "evidence fixture contains unexpected content or links",
                    ));
                }
                files.push(entry.file_name());
            }
            self.check_root()?;
            // Unlink only flat files through the held directory, so substituting
            // its original pathname cannot redirect deletion into another root.
            let held_root = PathBuf::from(format!("/proc/self/fd/{}", self.held.as_raw_fd()));
            for file in files {
                fs::remove_file(held_root.join(file))?;
            }
            self.check_root()?;
            fs::remove_dir(&self.root)?;
            self.cleaned = true;
            Ok(())
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if std::thread::panicking() {
                eprintln!(
                    "retained failing Work evidence fixture: {}",
                    self.root.display()
                );
                return;
            }
            if let Err(error) = self.cleanup() {
                panic!(
                    "retained Work evidence fixture {}: {error}",
                    self.root.display()
                );
            }
        }
    }

    #[test]
    fn preserves_schema_order_and_failed_or_unrun_checks() {
        let report = render(
            "first\tpass\trequired\tclean\nsecond\tfail\taudit\thigh finding\nthird\tnot_run\ttool-missing\tmissing\n",
        )
        .unwrap();
        assert!(report.ends_with(b"\n"));
        let parsed: serde_json::Value = serde_json::from_slice(&report).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!({
                "schema_version": "jeryu.split.security/v1",
                "checks": [
                    {"name": "first", "status": "pass", "policy": "required", "detail": "clean"},
                    {"name": "second", "status": "fail", "policy": "audit", "detail": "high finding"},
                    {"name": "third", "status": "not_run", "policy": "tool-missing", "detail": "missing"},
                ],
            })
        );
    }

    #[test]
    fn preserves_empty_report() {
        assert_eq!(
            render("").unwrap(),
            b"{\n  \"checks\": [],\n  \"schema_version\": \"jeryu.split.security/v1\"\n}\n"
        );
    }

    #[test]
    fn preserves_extra_tabs_unicode_quotes_and_empty_fields() {
        let report = render("\t\t\t\nname\tpass\tpolicy\t\"quoted\" café\tmore").unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&report).unwrap();
        assert_eq!(
            parsed["checks"][0],
            serde_json::json!({"name": "", "status": "", "policy": "", "detail": ""})
        );
        assert_eq!(parsed["checks"][1]["detail"], "\"quoted\" café\tmore");
    }

    #[test]
    fn accepts_existing_line_endings_and_missing_final_newline() {
        for separator in [
            "\n", "\r\n", "\r", "\u{b}", "\u{c}", "\u{1c}", "\u{1d}", "\u{1e}", "\u{85}",
            "\u{2028}", "\u{2029}",
        ] {
            let input = format!("a\tpass\tp\tone{separator}b\tfail\tp\ttwo");
            for suffix in ["", separator] {
                let parsed: serde_json::Value =
                    serde_json::from_slice(&render(&format!("{input}{suffix}")).unwrap()).unwrap();
                assert_eq!(parsed["checks"].as_array().unwrap().len(), 2);
                assert_eq!(parsed["checks"][1]["detail"], "two");
            }
        }
    }

    #[test]
    fn refuses_malformed_rows_without_replacing_existing_report() {
        let temp = Fixture::new();
        let input = temp.path().join("checks.tsv");
        let output = temp.path().join("evidence.json");
        for malformed in ["name", "name\tpass", "name\tpass\tpolicy", ""] {
            fs::write(&input, format!("ok\tpass\tpolicy\tdetail\n{malformed}\n")).unwrap();
            fs::write(&output, b"previous report\n").unwrap();
            let error = write_evidence(&input, &output).unwrap_err();
            assert!(error.to_string().contains("row 2 requires four columns"));
            assert_eq!(fs::read(&output).unwrap(), b"previous report\n");
        }
    }

    #[test]
    fn refuses_truncated_utf8_without_replacing_existing_report() {
        let temp = Fixture::new();
        let input = temp.path().join("checks.tsv");
        let output = temp.path().join("evidence.json");
        fs::write(&input, b"ok\tpass\tpolicy\tdetail\nnext\tfail\tp\t\xc3").unwrap();
        fs::write(&output, b"previous report\n").unwrap();
        assert!(write_evidence(&input, &output).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"previous report\n");
    }

    #[test]
    fn refuses_missing_input_without_replacing_existing_report() {
        let temp = Fixture::new();
        let output = temp.path().join("evidence.json");
        fs::write(&output, b"previous report\n").unwrap();
        assert!(write_evidence(&temp.path().join("absent.tsv"), &output).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"previous report\n");
    }

    #[test]
    fn propagates_filesystem_output_failures_and_writes_valid_report() {
        let temp = Fixture::new();
        let input = temp.path().join("checks.tsv");
        let bytes = b"check\tfail\trequired\tactual failed check\n";
        fs::write(&input, bytes).unwrap();
        // A directory and a missing parent fail independently of caller privileges.
        assert!(write_evidence(&input, temp.path()).is_err());
        let absent = temp.path().join("absent");
        assert!(write_evidence(&input, &absent.join("evidence.json")).is_err());
        assert!(!absent.exists());
        assert_eq!(fs::read(&input).unwrap(), bytes);

        let output = temp.path().join("evidence.json");
        write_evidence(&input, &output).unwrap();
        assert_eq!(
            fs::read(&output).unwrap(),
            render(std::str::from_utf8(bytes).unwrap()).unwrap()
        );
    }

    #[test]
    fn requires_exactly_two_path_arguments() {
        for count in [0, 1, 3, 4] {
            let error = paths(vec![OsString::from("path"); count]).unwrap_err();
            assert!(error.to_string().starts_with("usage:"));
        }
        assert_eq!(
            paths([
                OsString::from("checks.tsv"),
                OsString::from("evidence.json")
            ])
            .unwrap(),
            (PathBuf::from("checks.tsv"), PathBuf::from("evidence.json"))
        );
    }

    #[test]
    fn fixture_cleanup_refuses_links_and_unexpected_content() {
        let outside = Fixture::new();
        let mut fixture = Fixture::new();
        let target = outside.path().join("checks.tsv");
        let link = fixture.path().join("evidence.json");
        fs::write(&target, b"outside sentinel").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(fixture.cleanup().is_err());
        assert_eq!(fs::read_link(&link).unwrap(), target);
        assert_eq!(fs::read(&target).unwrap(), b"outside sentinel");
        fs::remove_file(&link).unwrap(); // Exact synthetic link, never its target.

        fs::hard_link(&target, &link).unwrap();
        assert!(fixture.cleanup().is_err());
        assert_eq!(fs::symlink_metadata(&link).unwrap().nlink(), 2);
        fs::remove_file(&link).unwrap();

        let unexpected = fixture.path().join("unexpected");
        fs::create_dir(&unexpected).unwrap();
        assert!(fixture.cleanup().is_err());
        fs::remove_dir(&unexpected).unwrap(); // Known empty synthetic directory.
        fixture.cleanup().unwrap();
        assert!(!fixture.path().exists());
        assert_eq!(fs::read(&target).unwrap(), b"outside sentinel");
    }

    #[test]
    fn fixture_retains_failed_test_bytes_until_explicit_custody_cleanup() {
        let failing = Fixture::new();
        fs::write(failing.path().join("checks.tsv"), b"failed test evidence").unwrap();
        // Keep separate custody only for this synthetic failure. Normal failed
        // tests leave their one guard to retain the directory during unwind.
        let mut retained = Fixture {
            root: failing.root.clone(),
            held: failing.held.try_clone().unwrap(),
            identity: failing.identity.clone(),
            cleaned: false,
        };
        assert!(
            std::panic::catch_unwind(move || {
                let _failing = failing;
                panic!("synthetic failed test");
            })
            .is_err()
        );
        retained.check_root().unwrap();
        assert_eq!(
            fs::read(retained.path().join("checks.tsv")).unwrap(),
            b"failed test evidence"
        );
        retained.cleanup().unwrap();
    }

    #[test]
    fn shell_dispatch_preserves_scan_and_writer_failures() {
        use std::process::Command;

        let script = include_str!("../../../../ops/ci/security.sh");
        let body = script
            .split_once("write_evidence() {\n")
            .unwrap()
            .1
            .split_once("\n}\n")
            .unwrap()
            .0;
        let tail = script.rsplit_once("\nwrite_evidence\n").unwrap().1;
        // Exercise the actual script's writer dispatch and final gate without
        // running scanners or compiling another Cargo process inside the test.
        let harness = format!(
            "set -euo pipefail\nchecks_tsv=checks.tsv\nevidence_json=evidence.json\n\
             cargo() {{ printf 'argument=%s\\n' \"$@\"; return \"$writer_status\"; }}\n\
             cp() {{ printf 'evidence-copy\\n'; }}\n\
             write_evidence() {{\n{body}\n}}\nwrite_evidence\n{tail}"
        );
        for (failed, writer_status, jobs) in [
            (0, 0, None),
            (0, 0, Some("3")),
            (1, 0, None),
            (0, 42, None),
            (1, 42, None),
        ] {
            let mut command = Command::new("/bin/bash");
            command
                .args(["--noprofile", "--norc", "-c", &harness])
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("failed", failed.to_string())
                .env("writer_status", writer_status.to_string());
            if let Some(jobs) = jobs {
                command.env("JERYU_CI_JOBS", jobs);
            }
            let output = command.output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(if writer_status == 0 {
                    failed
                } else {
                    writer_status
                })
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            let arguments: Vec<_> = stdout
                .lines()
                .filter_map(|line| line.strip_prefix("argument="))
                .collect();
            assert_eq!(
                arguments,
                [
                    "run",
                    "--locked",
                    "--quiet",
                    "--manifest-path",
                    "crates/jeryu-jira/Cargo.toml",
                    "--package",
                    "jeryu-jira",
                    "--bin",
                    "jeryu-jira-security-evidence",
                    "--jobs",
                    jobs.unwrap_or("2"),
                    "--",
                    "checks.tsv",
                    "evidence.json",
                ]
            );
            assert_eq!(stdout.contains("evidence-copy"), writer_status == 0);
            assert_eq!(
                stdout.contains("security ok"),
                failed == 0 && writer_status == 0
            );
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert_eq!(
                stderr.contains("security check failed"),
                failed != 0 && writer_status == 0
            );
        }
    }
}
