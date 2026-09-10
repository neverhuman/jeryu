use super::*;
use crate::render::tests::{run_fixture_git, test_root};
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{PermissionsExt, symlink};

struct Fixture {
    outer: PathBuf,
    root: PathBuf,
    output: PathBuf,
    base: String,
    identity: (u64, u64),
    links: BTreeMap<PathBuf, PathBuf>,
}

impl Fixture {
    fn new() -> Self {
        let outer = test_root("proof-inventory");
        fs::set_permissions(&outer, fs::Permissions::from_mode(0o700)).expect("private fixture");
        let identity = fs::symlink_metadata(&outer).expect("fixture identity");
        let root = outer.join("repo");
        let evidence = outer.join("evidence");
        fs::create_dir(&root).expect("fixture repository");
        fs::create_dir(&evidence).expect("fixture evidence");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("private repo");
        fs::set_permissions(&evidence, fs::Permissions::from_mode(0o700))
            .expect("private evidence");
        let mut fixture = Self {
            outer,
            root,
            output: evidence.join("inventory.json"),
            base: String::new(),
            identity: (identity.dev(), identity.ino()),
            links: BTreeMap::new(),
        };
        fixture.write(
            b"agent/owner-map.json",
            br#"{"owners":{"components/":"fixture","agent/":"fixture"}}"#,
        );
        fixture.write(
            b"agent/test-map.json",
            br#"{"tests":{"components/":{"command":"true","lane":"fixture"}}}"#,
        );
        fixture.write(
            b"agent/proof-lanes.toml",
            b"[[lane]]\nname='fixture'\ncommand='true'\n",
        );
        fixture.write(b"agent/audit-policy.toml", b"minimum_score=85\n");
        fixture.write(b"components/jeryu-tool/source.rs", b"old\nkeep\n");
        run_fixture_git(&fixture.root, &["init", "-q"]);
        run_fixture_git(
            &fixture.root,
            &["config", "user.name", "Jeryu Inventory Test"],
        );
        run_fixture_git(
            &fixture.root,
            &["config", "user.email", "inventory@invalid"],
        );
        fixture.base = fixture.commit();
        fixture
    }

    fn path(&self, bytes: &[u8]) -> PathBuf {
        self.root.join(OsString::from_vec(bytes.to_vec()))
    }
    fn tool(&self) -> PathBuf {
        self.root.join(TOOL_DIRECTORY)
    }
    fn write(&self, path: &[u8], bytes: &[u8]) {
        let path = self.path(path);
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directories");
        fs::write(path, bytes).expect("fixture bytes");
    }
    fn link(&mut self, path: PathBuf, target: PathBuf) {
        symlink(&target, &path).expect("fixture symlink");
        self.links.insert(path, target);
    }
    fn commit(&self) -> String {
        run_fixture_git(&self.root, &["add", "-A"]);
        run_fixture_git(&self.root, &["commit", "-qm", "inventory fixture change"]);
        git_text(&self.root, &["rev-parse", "HEAD"]).expect("fixture head")
    }
    fn args(&self) -> Vec<String> {
        vec![
            "--monorepo-root".into(),
            self.root.to_str().expect("fixture root").into(),
            "--base".into(),
            self.base.clone(),
            "--head".into(),
            git_text(&self.root, &["rev-parse", "HEAD"]).expect("head"),
            "--out".into(),
            self.output.to_str().expect("fixture output").into(),
        ]
    }
    fn read(&self) -> Value {
        assert_eq!(
            run(&self.tool(), &self.args()).expect("actual inventory command"),
            0
        );
        serde_json::from_slice(&fs::read(&self.output).expect("inventory output"))
            .expect("inventory JSON")
    }
    fn change_head(&self) {
        self.write(b"components/jeryu-tool/source.rs", b"new\nkeep\n");
        self.commit();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "retaining failed inventory fixture: {}",
                self.outer.display()
            );
            return;
        }
        fn inspect(fixture: &Fixture, directory: &Path) -> bool {
            fs::read_dir(directory).is_ok_and(|mut entries| {
                entries.all(|entry| {
                    entry.is_ok_and(|entry| {
                        let path = entry.path();
                        entry.file_type().is_ok_and(|kind| {
                            if kind.is_symlink() {
                                fixture.links.get(&path).is_some_and(|expected| {
                                    fs::read_link(&path).ok().as_ref() == Some(expected)
                                })
                            } else if kind.is_dir() {
                                inspect(fixture, &path)
                            } else {
                                kind.is_file()
                            }
                        })
                    })
                })
            })
        }
        let safe = fs::symlink_metadata(&self.outer).is_ok_and(|m| {
            m.is_dir()
                && !m.file_type().is_symlink()
                && (m.dev(), m.ino()) == self.identity
                && m.mode() & 0o777 == 0o700
        }) && fs::canonicalize(&self.outer).ok().as_ref() == Some(&self.outer)
            && fs::read_to_string("/proc/self/mountinfo").is_ok_and(|mounts| {
                !mounts
                    .lines()
                    .filter_map(|line| line.split_whitespace().nth(4))
                    .any(|mount| Path::new(mount).starts_with(&self.outer))
            })
            && inspect(self, &self.outer);
        if safe {
            fs::remove_dir_all(&self.outer)
                .expect("remove inspected fixture without following leaf links");
        } else {
            eprintln!(
                "retaining changed inventory fixture: {}",
                self.outer.display()
            );
        }
    }
}

fn find<'a>(report: &'a Value, side: &str, path: &[u8]) -> &'a Value {
    report["changes"]
        .as_array()
        .expect("changes")
        .iter()
        .find(|entry| entry[side]["path_hex"] == hex(path))
        .expect("exact byte path")
}

#[test]
fn inventory_binds_exact_commits_maps_and_actual_text_hunks() {
    let fixture = Fixture::new();
    fixture.change_head();
    let report = fixture.read();
    assert_eq!(report["base"]["commit"], fixture.base);
    assert_eq!(
        report["head"]["commit"],
        git_text(&fixture.root, &["rev-parse", "HEAD"]).expect("head")
    );
    assert_eq!(report["qualification"], "change-inventory-only");
    assert_eq!(report["predecessor_authentication"], "not-performed");
    assert_eq!(report["routing"], "not-performed");
    assert_eq!(report["full_proof"], false);
    assert_eq!(report["protected_main"], false);
    assert_eq!(
        report["base"]["root_proof_inputs"]
            .as_array()
            .expect("maps")
            .len(),
        4
    );
    for side in ["base", "head"] {
        let commit = report[side]["commit"].as_str().expect("commit");
        for entry in report[side]["root_proof_inputs"].as_array().expect("maps") {
            let path = entry["path"].as_str().expect("map path");
            let object = git_text(&fixture.root, &["rev-parse", &format!("{commit}:{path}")])
                .expect("map object");
            assert_eq!(entry["git_blob"], object);
            assert_eq!(
                entry["sha256"],
                sha256_bytes(&blob(&fixture.root, &object).expect("map bytes"))
                    .expect("map digest")
            );
        }
    }
    let change = find(&report, "new", b"components/jeryu-tool/source.rs");
    assert_eq!(change["status"], "M");
    assert_eq!(
        change["text_hunks"],
        json!([{"old_start":1,"old_count":1,"new_start":1,"new_count":1}])
    );
    assert_eq!(
        change["old"]["sha256"],
        sha256_bytes(b"old\nkeep\n").expect("old SHA")
    );
    assert_eq!(
        change["new"]["sha256"],
        sha256_bytes(b"new\nkeep\n").expect("new SHA")
    );
    assert_eq!(
        fs::metadata(&fixture.output)
            .expect("output metadata")
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn inventory_preserves_all_filename_bytes_and_cross_component_scope() {
    let fixture = Fixture::new();
    let names: [&[u8]; 6] = [
        b"components/jeryu-cache/space name.rs",
        b"components/jeryu-core/tab\tname.rs",
        b"components/jeryu-deploy/line\nname.rs",
        b"components/jeryu-tool/back\\slash.rs",
        b"components/jeryu-web/nonutf8-\xff.bin",
        b"components/jeryu-tool/ leading-and-trailing ",
    ];
    for (index, name) in names.iter().enumerate() {
        fixture.write(name, format!("unique added {index}\n").as_bytes());
    }
    fixture.commit();
    let report = fixture.read();
    assert_eq!(
        report["changes"].as_array().expect("changes").len(),
        names.len()
    );
    for name in names {
        let change = find(&report, "new", name);
        assert_eq!(
            change["new"]["path_utf8"],
            json!(std::str::from_utf8(name).ok())
        );
        assert!(change["old"].is_null());
    }
}

#[test]
fn inventory_keeps_rename_and_copy_endpoints_and_edited_spans() {
    let mut fixture = Fixture::new();
    fixture.write(
        b"components/jeryu-tool/rename.txt",
        b"unique rename\na\nb\nc\nd\ne\nf\ng\nh\ni\n",
    );
    fixture.write(
        b"components/jeryu-tool/copy.txt",
        b"unique copy\nother contents\n",
    );
    fixture.base = fixture.commit();
    fs::rename(
        fixture.path(b"components/jeryu-tool/rename.txt"),
        fixture.path(b"components/jeryu-tool/moved.txt"),
    )
    .expect("rename");
    fixture.write(
        b"components/jeryu-tool/moved.txt",
        b"unique rename\na\nb\nc\nd\ne\nf\ng\nh\nchanged\n",
    );
    fixture.write(
        b"components/jeryu-tool/copied.txt",
        b"unique copy\nother contents\n",
    );
    fixture.commit();
    let report = fixture.read();
    let renamed = find(&report, "new", b"components/jeryu-tool/moved.txt");
    assert!(
        renamed["status"]
            .as_str()
            .expect("rename status")
            .starts_with('R')
    );
    assert_eq!(
        renamed["old"]["path_hex"],
        hex(b"components/jeryu-tool/rename.txt")
    );
    assert_eq!(renamed["text_hunks"][0]["old_start"], 10);
    let copied = find(&report, "new", b"components/jeryu-tool/copied.txt");
    assert_eq!(copied["status"], "C100");
    assert_eq!(
        copied["old"]["path_hex"],
        hex(b"components/jeryu-tool/copy.txt")
    );
    assert_eq!(copied["old"]["git_object"], copied["new"]["git_object"]);
    assert_eq!(copied["text_hunks"], json!([]));
}

#[test]
fn inventory_handles_pure_rename_deletion_binary_and_empty_files() {
    let mut fixture = Fixture::new();
    fixture.write(b"components/jeryu-tool/binary", b"\0old\xff");
    fixture.write(
        b"components/jeryu-tool/deleted.txt",
        b"delete one\ndelete two",
    );
    fixture.write(b"components/jeryu-tool/binary-delete", b"\0delete binary");
    fixture.base = fixture.commit();
    fs::rename(
        fixture.path(b"components/jeryu-tool/source.rs"),
        fixture.path(b"components/jeryu-tool/pure-rename.rs"),
    )
    .expect("rename");
    fs::remove_file(fixture.path(b"components/jeryu-tool/deleted.txt"))
        .expect("delete fixture file");
    fs::remove_file(fixture.path(b"components/jeryu-tool/binary-delete"))
        .expect("delete binary fixture");
    fixture.write(b"components/jeryu-tool/binary", b"\0new\xfe");
    fixture.write(b"components/jeryu-tool/binary-added", b"\xffnonutf8");
    fixture.write(b"components/jeryu-tool/empty", b"");
    fixture.commit();
    let report = fixture.read();
    assert_eq!(
        find(&report, "new", b"components/jeryu-tool/pure-rename.rs")["status"],
        "R100"
    );
    let deleted = find(&report, "old", b"components/jeryu-tool/deleted.txt");
    assert_eq!(deleted["status"], "D");
    assert_eq!(
        deleted["text_hunks"],
        json!([{"old_start":1,"old_count":2,"new_start":0,"new_count":0}])
    );
    for (side, path) in [
        ("new", b"components/jeryu-tool/binary".as_slice()),
        ("new", b"components/jeryu-tool/binary-added".as_slice()),
        ("old", b"components/jeryu-tool/binary-delete".as_slice()),
    ] {
        let entry = find(&report, side, path);
        assert_eq!(entry[side]["binary"], true);
        assert!(entry["text_hunks"].is_null());
    }
    assert_eq!(
        find(&report, "new", b"components/jeryu-tool/empty")["text_hunks"],
        json!([])
    );
}

#[test]
fn inventory_records_modes_symlink_targets_and_gitlinks_without_following_them() {
    let mut fixture = Fixture::new();
    fixture.write(
        b"components/jeryu-tool/type-change",
        b"former regular file\n",
    );
    fixture.base = fixture.commit();
    fs::set_permissions(
        fixture.path(b"components/jeryu-tool/source.rs"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("mode change");
    let type_path = fixture.path(b"components/jeryu-tool/type-change");
    fs::remove_file(&type_path).expect("remove original regular file");
    fixture.link(type_path, PathBuf::from("source.rs"));
    fs::create_dir(fixture.root.join("components/external"))
        .expect("uninitialized submodule directory");
    run_fixture_git(&fixture.root, &["add", "-A"]);
    run_fixture_git(
        &fixture.root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},components/external", fixture.base),
        ],
    );
    run_fixture_git(
        &fixture.root,
        &["commit", "-qm", "mode symlink and gitlink fixture"],
    );
    let report = fixture.read();
    let mode = find(&report, "new", b"components/jeryu-tool/source.rs");
    assert_eq!(mode["old"]["mode"], "100644");
    assert_eq!(mode["new"]["mode"], "100755");
    assert_eq!(mode["text_hunks"], json!([]));
    let link = find(&report, "new", b"components/jeryu-tool/type-change");
    assert_eq!(link["status"], "T");
    assert_eq!(link["new"]["kind"], "symlink");
    assert_eq!(
        link["new"]["sha256"],
        sha256_bytes(b"source.rs").expect("link blob SHA")
    );
    assert!(link["text_hunks"].is_null());
    let gitlink = find(&report, "new", b"components/external");
    assert_eq!(gitlink["new"]["kind"], "gitlink");
    assert_eq!(gitlink["new"]["git_object"], fixture.base);
    assert!(gitlink["new"]["sha256"].is_null());
    assert!(gitlink["text_hunks"].is_null());
}

#[test]
fn inventory_rejects_mutable_short_duplicate_and_incomplete_arguments() {
    let fixture = Fixture::new();
    fixture.change_head();
    let args = fixture.args();
    for replacement in [
        "HEAD",
        "origin/main",
        "abc123",
        "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
        "000000000000000000000000000000000000000g",
    ] {
        let mut bad = args.clone();
        bad[3] = replacement.into();
        assert!(parse_args(&bad).is_err());
    }
    let mut duplicate = args.clone();
    duplicate[0] = "--head".into();
    assert!(parse_args(&duplicate).is_err());
    let mut same = args.clone();
    same[3] = same[5].clone();
    assert!(parse_args(&same).is_err());
    let mut relative = args.clone();
    relative[1] = "relative".into();
    assert!(parse_args(&relative).is_err());
    assert!(parse_args(&args[..6]).is_err());
    assert!(parse_args(&[]).is_err());
    assert!(!fixture.output.exists());
}

#[test]
fn inventory_rejects_dirty_staged_untracked_suppressed_and_wrong_head_sources() {
    for kind in ["dirty", "staged", "untracked", "suppressed", "wrong-head"] {
        let fixture = Fixture::new();
        fixture.change_head();
        let mut args = fixture.args();
        match kind {
            "dirty" | "staged" => {
                fixture.write(b"components/jeryu-tool/source.rs", b"changed again\n");
                if kind == "staged" {
                    run_fixture_git(&fixture.root, &["add", "-A"]);
                }
            }
            "untracked" => fixture.write(b"untracked", b"not committed"),
            "suppressed" => run_fixture_git(
                &fixture.root,
                &[
                    "update-index",
                    "--assume-unchanged",
                    "components/jeryu-tool/source.rs",
                ],
            ),
            "wrong-head" => args[5] = "f".repeat(40),
            _ => unreachable!(),
        }
        assert!(
            run(&fixture.tool(), &args).is_err(),
            "source drift accepted: {kind}"
        );
        assert!(!fixture.output.exists());
    }
}

#[test]
fn inventory_rejects_missing_unrelated_base_and_history_rewrites() {
    for kind in ["missing", "unrelated", "grafts", "alternates", "shallow"] {
        let fixture = Fixture::new();
        fixture.change_head();
        let mut args = fixture.args();
        match kind {
            "missing" => args[3] = "f".repeat(40),
            "unrelated" => {
                let tree = git_text(&fixture.root, &["rev-parse", "HEAD^{tree}"]).expect("tree");
                let unrelated = git_text(
                    &fixture.root,
                    &[
                        "-c",
                        "user.name=Inventory",
                        "-c",
                        "user.email=inventory@invalid",
                        "commit-tree",
                        &tree,
                        "-m",
                        "unrelated fixture",
                    ],
                )
                .expect("unrelated commit");
                args[3] = unrelated;
            }
            "grafts" => {
                fs::write(fixture.root.join(".git/info/grafts"), "").expect("graft fixture")
            }
            "alternates" => fs::write(fixture.root.join(".git/objects/info/alternates"), "")
                .expect("alternate fixture"),
            "shallow" => fs::write(fixture.root.join(".git/shallow"), "").expect("shallow fixture"),
            _ => unreachable!(),
        }
        assert!(
            run(&fixture.tool(), &args).is_err(),
            "bad history accepted: {kind}"
        );
        assert!(!fixture.output.exists());
    }
}

#[test]
fn inventory_rejects_absent_malformed_or_symlinked_root_maps() {
    for kind in ["missing", "malformed", "symlink"] {
        let mut fixture = Fixture::new();
        let path = fixture.root.join("agent/test-map.json");
        match kind {
            "missing" => fs::remove_file(&path).expect("remove map"),
            "malformed" => fs::write(&path, "{").expect("malformed map"),
            "symlink" => {
                fs::remove_file(&path).expect("remove map");
                fixture.link(path, PathBuf::from("owner-map.json"));
            }
            _ => unreachable!(),
        }
        fixture.commit();
        assert!(run(&fixture.tool(), &fixture.args()).is_err());
        assert!(!fixture.output.exists());
    }
}

#[test]
fn inventory_output_is_create_once_private_and_outside_source() {
    let mut fixture = Fixture::new();
    fixture.change_head();
    fs::write(&fixture.output, "preserved predecessor").expect("existing evidence");
    assert!(run(&fixture.tool(), &fixture.args()).is_err());
    assert_eq!(
        fs::read_to_string(&fixture.output).expect("preserved evidence"),
        "preserved predecessor"
    );
    fs::remove_file(&fixture.output).expect("remove owned fixture evidence");
    let destination = fixture.outer.join("destination");
    fs::write(&destination, "unchanged").expect("link destination");
    fixture.link(fixture.output.clone(), destination.clone());
    assert!(run(&fixture.tool(), &fixture.args()).is_err());
    assert_eq!(
        fs::read_to_string(&destination).expect("destination"),
        "unchanged"
    );
    fs::remove_file(&fixture.output).expect("remove inspected fixture link");
    fixture.links.remove(&fixture.output);
    fs::hard_link(&destination, &fixture.output).expect("hardlink fixture");
    assert!(run(&fixture.tool(), &fixture.args()).is_err());
    fs::remove_file(&fixture.output).expect("remove owned hardlink fixture");
    fs::set_permissions(
        fixture.output.parent().expect("parent"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("broaden fixture parent");
    assert!(run(&fixture.tool(), &fixture.args()).is_err());
    fs::set_permissions(
        fixture.output.parent().expect("parent"),
        fs::Permissions::from_mode(0o700),
    )
    .expect("restore fixture parent");
    let mut inside = fixture.args();
    inside[7] = fixture
        .root
        .join("inside.json")
        .to_str()
        .expect("path")
        .into();
    assert!(run(&fixture.tool(), &inside).is_err());
    assert!(!fixture.root.join("inside.json").exists());
    let alias = fixture.outer.join("alias");
    fixture.link(
        alias.clone(),
        fixture.output.parent().expect("parent").to_path_buf(),
    );
    let mut aliased = fixture.args();
    aliased[7] = alias.join("out.json").to_str().expect("alias").into();
    assert!(run(&fixture.tool(), &aliased).is_err());
}

#[test]
fn inventory_tree_and_raw_parsers_reject_truncation_omission_and_smuggled_status() {
    let old = parse_tree(b"100644 blob 1111111111111111111111111111111111111111\tpath\0")
        .expect("old tree");
    let new = parse_tree(b"100644 blob 2222222222222222222222222222222222222222\tpath\0")
        .expect("new tree");
    let raw = b":100644 100644 1111111111111111111111111111111111111111 2222222222222222222222222222222222222222 M\0path\0";
    assert_eq!(parse_changes(raw, &old, &new).expect("change").len(), 1);
    assert!(parse_changes(b"", &old, &new).is_err());
    assert!(parse_changes(&raw[..raw.len() - 1], &old, &new).is_err());
    let duplicate = [raw.as_slice(), raw.as_slice()].concat();
    assert!(parse_changes(&duplicate, &old, &new).is_err());
    assert!(parse_changes(b":000000 100644 0000000000000000000000000000000000000000 2222222222222222222222222222222222222222 A\0path\0", &old, &new).is_err());
    for malformed in [
        b"100644 blob 1111111111111111111111111111111111111111\t../escape\0".as_slice(),
        b"100644 blob 1111111111111111111111111111111111111111\tpath",
        b"160000 blob 1111111111111111111111111111111111111111\tpath\0",
    ] {
        assert!(parse_tree(malformed).is_err());
    }
}

#[test]
fn inventory_hunk_parser_rejects_invented_line_one_and_malformed_spans() {
    let deleted = b"@@ -2 +1,0 @@\n-deleted\n";
    assert_eq!(
        parse_hunks(deleted, 2, 1).expect("deletion hunk"),
        vec![json!({"old_start":2,"old_count":1,"new_start":1,"new_count":0})]
    );
    for malformed in [
        b"@@ -1 +1 @@\n-old\n".as_slice(),
        b"@@ -0 +1 @@\n-old\n+new\n",
        b"@@ -1 +99 @@\n-old\n+new\n",
        b"@@ -0,0 +0,0 @@\n",
        b"@@ -1 +1 @@\n-old\n+new\n+extra\n",
        b"@@ -1 +1 @@\n-old\n+new\n@@ -1 +1 @@\n-old\n+new\n",
    ] {
        assert!(parse_hunks(malformed, 2, 2).is_err());
    }
    assert!(parse_hunks(b"", 0, 0).expect("empty text diff").is_empty());
    assert_eq!(line_count(b""), 0);
    assert_eq!(line_count(b"last without newline"), 1);
    assert_eq!(line_count(b"a\nb\n"), 2);
}

#[test]
fn inventory_publication_rechecks_parent_permissions_after_admission() {
    let fixture = Fixture::new();
    fixture.change_head();
    let parent = fixture.output.parent().expect("output parent");
    let result = run_with_hook(&fixture.tool(), &fixture.args(), || {
        fs::set_permissions(parent, fs::Permissions::from_mode(0o755))
            .expect("widen admitted parent after write");
    });
    assert!(
        result
            .expect_err("changed parent must fail")
            .contains("custody changed")
    );
    assert!(
        fixture.output.is_file(),
        "failed publication retains diagnostic bytes"
    );
    assert_eq!(
        fs::metadata(parent).expect("parent metadata").mode() & 0o7777,
        0o755
    );
}

#[test]
fn inventory_publication_rejects_output_permission_change_after_write() {
    let fixture = Fixture::new();
    fixture.change_head();
    let result = run_with_hook(&fixture.tool(), &fixture.args(), || {
        fs::set_permissions(&fixture.output, fs::Permissions::from_mode(0o644))
            .expect("widen output after write");
    });
    assert!(
        result
            .expect_err("changed output mode must fail")
            .contains("custody changed")
    );
    assert!(
        fixture.output.is_file(),
        "failed publication retains diagnostic bytes"
    );
    assert_eq!(
        fs::metadata(&fixture.output)
            .expect("output metadata")
            .mode()
            & 0o7777,
        0o644
    );
}

#[test]
fn inventory_publication_rejects_new_output_hardlink_after_write() {
    let fixture = Fixture::new();
    fixture.change_head();
    let alias = fixture.output.with_file_name("inventory-alias.json");
    let result = run_with_hook(&fixture.tool(), &fixture.args(), || {
        fs::hard_link(&fixture.output, &alias).expect("link output after write");
    });
    assert!(
        result
            .expect_err("linked output must fail")
            .contains("custody changed")
    );
    let output = fs::metadata(&fixture.output).expect("retained output");
    let linked = fs::metadata(&alias).expect("retained alias");
    assert_eq!((output.dev(), output.ino()), (linked.dev(), linked.ino()));
    assert_eq!(output.nlink(), 2);
    assert_eq!(
        fs::read(&fixture.output).expect("output bytes"),
        fs::read(&alias).expect("alias bytes")
    );
}

#[test]
fn inventory_publication_admits_unchanged_private_parent_and_output() {
    let fixture = Fixture::new();
    fixture.change_head();
    let parent_path = fixture.output.parent().expect("output parent");
    let before = fs::metadata(parent_path).expect("original parent");
    let seen = std::cell::Cell::new(false);
    assert_eq!(
        run_with_hook(&fixture.tool(), &fixture.args(), || seen.set(true))
            .expect("unchanged publication"),
        0
    );
    assert!(seen.get(), "actual after-write seam ran");
    let after = fs::metadata(parent_path).expect("final parent");
    assert_eq!(
        (
            before.dev(),
            before.ino(),
            before.uid(),
            before.gid(),
            before.mode()
        ),
        (
            after.dev(),
            after.ino(),
            after.uid(),
            after.gid(),
            after.mode()
        )
    );
    let output = fs::symlink_metadata(&fixture.output).expect("published output");
    assert!(output.is_file());
    assert_eq!(output.uid(), before.uid());
    assert_eq!(output.mode() & 0o7777, 0o600);
    assert_eq!(output.nlink(), 1);
    let report: Value = serde_json::from_slice(&fs::read(&fixture.output).expect("output bytes"))
        .expect("inventory JSON");
    assert_eq!(report["head"]["commit"], fixture.args()[5]);
    assert_eq!(report["full_proof"], false);
}
