use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

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
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z");
    command
}

pub(super) fn output_text(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

pub(super) fn git(root: &Path, args: &[&str]) -> String {
    output_text(command(root, "/usr/bin/git").args(args).output().unwrap())
        .trim()
        .to_owned()
}

fn input(root: &Path, args: &[&str], bytes: &[u8], index: Option<&Path>) -> String {
    let mut command = command(root, "/usr/bin/git");
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    output_text(child.wait_with_output().unwrap())
        .trim()
        .to_owned()
}

pub(super) fn tag_snapshot(root: &Path) -> String {
    output_text(
        command(root, "/usr/bin/git")
            .args([
                "for-each-ref",
                "--sort=refname",
                "--format=%(objectname) %(refname)",
                "refs/tags/",
            ])
            .output()
            .unwrap(),
    )
}

pub(super) struct Fixture {
    temporary: PathBuf,
    scratch_helper: String,
    scratch_identity: String,
    pub(super) root: PathBuf,
    pub(super) mirror: PathBuf,
    pub(super) tags: PathBuf,
    pub(super) source: String,
    pub(super) tip: String,
}

impl Fixture {
    pub(super) fn new(import_parent: bool) -> Self {
        let temporary = tempfile::Builder::new()
            .prefix("jeryu-mirror-prepare-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
            .keep();
        eprintln!(
            "mirror fixture retained until guarded success: {}",
            temporary.display()
        );
        // The existing root helper is projected into standalone Deploy exports too.
        let package = Path::new(env!("CARGO_MANIFEST_DIR"));
        let helper_path = [2, 4]
            .into_iter()
            .find_map(|depth| {
                let candidate = package.ancestors().nth(depth)?.join("tests/scratch.sh");
                candidate.is_file().then_some(candidate)
            })
            .expect("owning test scratch helper");
        let scratch_helper = fs::read_to_string(helper_path).expect("read owning scratch helper");
        let record = format!(
            "{scratch_helper}\njeryu_record_test_scratch \"$1\"\nprintf '%s' \"$jeryu_test_scratch_identity\"\n"
        );
        let scratch_identity = output_text(
            command(&temporary, "/bin/bash")
                .args([
                    "-euo",
                    "pipefail",
                    "-c",
                    &record,
                    "mirror-fixture",
                    temporary.to_str().unwrap(),
                ])
                .output()
                .expect("record fixture identity"),
        );
        let root = temporary.join("source");
        let mirror = temporary.join("mirror.git");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&mirror).unwrap();
        git(
            &root,
            &["init", "--quiet", "--template=", "--initial-branch=main"],
        );
        git(
            &mirror,
            &[
                "init",
                "--bare",
                "--quiet",
                "--template=",
                "--initial-branch=main",
            ],
        );
        for (name, content) in [
            (
                "Cargo.toml",
                "[workspace]\nmembers=[\"components/jeryu-cache\"]\n[workspace.dependencies]\ncache={path=\"components/jeryu-cache\"}\n",
            ),
            ("Cargo.lock", "version = 4\n"),
            (
                "repos.manifest.toml",
                "[[repo]]\nname=\"jeryu-cache\"\nrole=\"split-mirror\"\npath=\"components/jeryu-cache\"\ndefault_branch=\"main\"\nremote=\"https://github.com/neverhuman/jeryu-cache.git\"\n",
            ),
            ("LICENSE", "Apache-2.0 fixture\n"),
            ("rust-toolchain.toml", "[toolchain]\nchannel=\"1.97.1\"\n"),
            (".cargo/config.toml", "[build]\njobs=2\n"),
            ("scripts/source-build.sh", "# synthetic source helper\n"),
            (
                "components/jeryu-cache/Cargo.toml",
                "[package]\nname=\"jeryu-cache-fixture\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
            ),
            ("components/jeryu-cache/src/lib.rs", "pub fn fixture() {}\n"),
            ("components/jeryu-cache/AGENTS.md", "Synthetic fixture\n"),
            ("components/jeryu-cache/README.md", "Synthetic fixture\n"),
        ] {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        for (name, content) in [
            ("main.rs", include_str!("../../src/main.rs")),
            (
                "mirror_update.rs",
                include_str!("../../src/mirror_update.rs"),
            ),
            ("split_tree.rs", include_str!("../../src/split_tree.rs")),
            ("split_export.rs", include_str!("../../src/split_export.rs")),
            (
                "canonical_json.rs",
                include_str!("../../src/canonical_json.rs"),
            ),
            ("split_ci.sh", include_str!("../../src/split_ci.sh")),
            ("split_ci.yml", include_str!("../../src/split_ci.yml")),
        ] {
            let path = root
                .join("components/jeryu-deploy/crates/jeryu-split-tool/src")
                .join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        git(&root, &["add", "."]);
        git(
            &root,
            &[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "Synthetic monorepo",
            ],
        );
        let source = git(&root, &["rev-parse", "HEAD"]);
        let empty = input(&mirror, &["mktree"], b"", None);
        let tip = git(
            &mirror,
            &["commit-tree", &empty, "-m", "Existing mirror history"],
        );
        git(&mirror, &["update-ref", "refs/heads/main", &tip]);
        git(&mirror, &["tag", "immutable-existing-v1", &tip]);
        if import_parent {
            git(
                &root,
                &[
                    "fetch",
                    "--quiet",
                    "--no-tags",
                    mirror.to_str().unwrap(),
                    &tip,
                ],
            );
        }
        let tags = temporary.join("tags.txt");
        fs::write(&tags, tag_snapshot(&mirror)).unwrap();
        Self {
            temporary,
            scratch_helper,
            scratch_identity,
            root,
            mirror,
            tags,
            source,
            tip,
        }
    }

    pub(super) fn export(&self) -> String {
        let output = command(&self.root, env!("CARGO_BIN_EXE_jeryu-split"))
            .args([
                "export-tree",
                "--source",
                &self.source,
                "--component",
                "jeryu-cache",
            ])
            .output()
            .unwrap();
        let value: Value = serde_json::from_str(&output_text(output)).unwrap();
        let tree = value["tree"].as_str().unwrap();
        let mut descriptor: Value = serde_json::from_str(&git(
            &self.root,
            &["show", &format!("{tree}:.jeryu-source.json")],
        ))
        .unwrap();
        // Synthetic resolved-tree input: this test does not run or attest Cargo resolution.
        descriptor["lock_regeneration_required"] = json!(false);
        self.replace_blob(
            tree,
            ".jeryu-source.json",
            serde_json::to_string(&descriptor).unwrap().as_bytes(),
        )
    }

    pub(super) fn replace_blob(&self, tree: &str, name: &str, bytes: &[u8]) -> String {
        let temp = tempfile::tempdir_in(&self.temporary).unwrap().keep();
        let index = temp.join("index");
        input(&self.root, &["read-tree", tree], b"", Some(&index));
        let blob = input(&self.root, &["hash-object", "-w", "--stdin"], bytes, None);
        input(
            &self.root,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("100644,{blob},{name}"),
            ],
            b"",
            Some(&index),
        );
        input(&self.root, &["write-tree"], b"", Some(&index))
    }

    pub(super) fn prepare(&self, tree: &str, initial: Option<&str>) -> Output {
        let mut cmd = command(&self.root, env!("CARGO_BIN_EXE_jeryu-split"));
        cmd.args([
            "prepare-mirror-update",
            "--source-repo",
            self.root.to_str().unwrap(),
            "--source",
            &self.source,
            "--component",
            "jeryu-cache",
            "--export-tree",
            tree,
            "--mirror",
            self.mirror.to_str().unwrap(),
            "--expected-tip",
            &self.tip,
            "--expected-tags",
            self.tags.to_str().unwrap(),
        ]);
        if let Some(tip) = initial {
            cmd.args(["--initial-tip", tip]);
        }
        cmd.output().unwrap()
    }

    pub(super) fn publish_fixture(&mut self, commit: &str) {
        // Local test setup only. Production prepare never changes mirror refs.
        git(
            &self.mirror,
            &[
                "fetch",
                "--quiet",
                "--no-tags",
                self.root.to_str().unwrap(),
                commit,
            ],
        );
        git(
            &self.mirror,
            &["update-ref", "refs/heads/main", commit, &self.tip],
        );
        self.tip = commit.to_owned();
    }

    pub(super) fn advance_source(&mut self) {
        fs::write(
            self.root.join("components/jeryu-cache/src/lib.rs"),
            "pub fn successor() {}\n",
        )
        .unwrap();
        git(&self.root, &["add", "."]);
        git(
            &self.root,
            &[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "Next source",
            ],
        );
        self.source = git(&self.root, &["rev-parse", "HEAD"]);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "retaining failed mirror fixture: {}",
                self.temporary.display()
            );
            return;
        }
        let cleanup = format!(
            "{}\njeryu_test_scratch=\"$1\"\njeryu_test_scratch_identity=\"$2\"\njeryu_remove_test_scratch\n",
            self.scratch_helper
        );
        let result = command(Path::new("/"), "/bin/bash")
            .args([
                "-euo",
                "pipefail",
                "-c",
                &cleanup,
                "mirror-fixture",
                self.temporary.to_str().unwrap(),
                &self.scratch_identity,
            ])
            .output();
        match result {
            Ok(output) if output.status.success() => {}
            Ok(output) => panic!(
                "mirror fixture cleanup refused; retained {}: {}",
                self.temporary.display(),
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(error) => panic!(
                "mirror fixture cleanup unavailable; retained {}: {error}",
                self.temporary.display()
            ),
        }
    }
}
