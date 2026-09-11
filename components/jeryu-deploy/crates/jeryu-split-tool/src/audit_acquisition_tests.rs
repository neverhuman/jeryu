use super::*;
use std::{collections::BTreeMap, os::unix::fs::symlink};

struct Fixture {
    root: PathBuf,
    identity: (u64, u64, u32),
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("jeryu-public-input-test-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
            .keep();
        eprintln!(
            "public acquisition fixture retained until guarded success: {}",
            root.display()
        );
        let identity = custody::owned_directory(&root).unwrap();
        Self { root, identity }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "retaining failed public acquisition fixture: {}",
                self.root.display()
            );
        } else {
            custody::remove(&self.root, self.identity).unwrap();
        }
    }
}

fn source(scope: &str, commit: Option<&str>) -> Source {
    Source {
        repository: "neverhuman/jeryu-tool".into(),
        scope: scope.into(),
        path: None,
        commit: commit.map(str::to_owned),
        minimum: 85,
        required: true,
        reason: None,
    }
}

#[test]
fn public_urls_cannot_select_other_hosts_protocols_users_or_paths() {
    assert_eq!(
        url("neverhuman/RedlineDB").unwrap(),
        "https://github.com/neverhuman/RedlineDB.git"
    );
    for rejected in [
        "unresolved/redline",
        "someone/repo",
        "neverhuman/",
        "neverhuman/../repo",
        "neverhuman/repo.git",
        "neverhuman/repo/other",
        "neverhuman/repo?x=y",
        "neverhuman/repo#x",
        "neverhuman/user@host",
        "neverhuman/-option",
        "https://github.com/neverhuman/repo",
    ] {
        assert!(url(rejected).is_err(), "accepted {rejected}");
    }
}

#[test]
fn only_enrolled_unpinned_mirrors_observe_the_maintained_ref() {
    let f = Fixture::new();
    fs::write(
        f.root.join("repos.manifest.toml"),
        "[[repo]]\ngithub_slug='neverhuman/jeryu-tool'\ndefault_branch='release/5.x'\n",
    )
    .unwrap();
    assert_eq!(
        maintained_ref(&f.root, &source("standalone", None))
            .unwrap()
            .as_deref(),
        Some("refs/heads/release/5.x")
    );
    let pin = "a".repeat(40);
    for scope in ["standalone", "dependency", "optional"] {
        assert!(
            maintained_ref(&f.root, &source(scope, Some(&pin)))
                .unwrap()
                .is_none()
        );
    }
    assert!(maintained_ref(&f.root, &source("dependency", None)).is_err());
    assert!(maintained_ref(&f.root, &source("optional", None)).is_err());
    assert!(maintained_ref(&f.root, &source("standalone", Some("main"))).is_err());
    for branch in [
        "../main",
        "main.lock",
        "a//b",
        "main:other",
        "a/.hidden",
        "main\nother",
    ] {
        fs::write(
            f.root.join("repos.manifest.toml"),
            format!("[[repo]]\ngithub_slug='neverhuman/jeryu-tool'\ndefault_branch={branch:?}\n"),
        )
        .unwrap();
        assert!(maintained_ref(&f.root, &source("standalone", None)).is_err());
    }
    for invalid in [
        "repo=[]",
        "repo=['malformed']",
        "[[repo]]\ngithub_slug='neverhuman/jeryu-tool'",
        "[[repo]]\ngithub_slug='neverhuman/other'\ndefault_branch='main'",
        "[[repo]]\ngithub_slug='neverhuman/jeryu-tool'\ndefault_branch='main'\n[[repo]]\ngithub_slug='neverhuman/jeryu-tool'\ndefault_branch='main'",
    ] {
        fs::write(f.root.join("repos.manifest.toml"), invalid).unwrap();
        assert!(maintained_ref(&f.root, &source("standalone", None)).is_err());
    }
}

#[test]
fn public_ref_resolution_is_exact_and_unambiguous() {
    let pin = "b".repeat(40);
    let reference = "refs/heads/main";
    let valid = format!("{pin}\t{reference}\n");
    assert_eq!(observed_commit(&valid, reference).unwrap(), pin);
    for rejected in [
        String::new(),
        format!("{valid}{valid}"),
        format!("{pin}\tHEAD\n"),
        format!("{pin} {reference}\n"),
        format!("{}\t{reference}", "b".repeat(39)),
        format!("{}\t{reference}", "B".repeat(40)),
        format!("{valid}unexpected"),
    ] {
        assert!(observed_commit(&rejected, reference).is_err());
    }
}

#[test]
fn transport_command_pins_tools_and_has_no_credential_or_user_configuration_inputs() {
    let command = command(Path::new("/tmp"), 23);
    assert_eq!(command.get_program(), "/usr/bin/timeout");
    let args: Vec<_> = command
        .get_args()
        .map(|value| value.to_str().unwrap())
        .collect();
    for required in [
        "23",
        "/usr/bin/prlimit",
        "/usr/bin/git",
        "protocol.allow=never",
        "protocol.https.allow=always",
        "credential.helper=",
        "core.hooksPath=/dev/null",
        "http.followRedirects=false",
        "submodule.recurse=false",
    ] {
        assert!(args.contains(&required), "missing {required}");
    }
    let env: BTreeMap<_, _> = command
        .get_envs()
        .map(|(name, value)| (name.to_str().unwrap(), value.unwrap().to_str().unwrap()))
        .collect();
    assert_eq!(env["HOME"], "/nonexistent");
    assert_eq!(env["GIT_CONFIG_GLOBAL"], "/dev/null");
    assert_eq!(env["GIT_CONFIG_SYSTEM"], "/dev/null");
    assert_eq!(env["GIT_NO_LAZY_FETCH"], "1");
    assert_eq!(env["GIT_CEILING_DIRECTORIES"], "/");
    for forbidden in [
        "GITHUB_TOKEN",
        "GH_TOKEN",
        "GIT_CONFIG_COUNT",
        "GIT_SSH_COMMAND",
        "HTTPS_PROXY",
        "LD_PRELOAD",
    ] {
        assert!(!env.contains_key(forbidden));
    }
}

#[test]
fn custody_refuses_path_replacement_links_open_handles_and_excess_size() {
    let f = Fixture::new();
    let file = f.root.join("source.txt");
    fs::write(&file, "public source").unwrap();
    assert!(custody::inspect(&f.root, 1, MAX_SOURCE_ENTRIES).is_err());
    assert!(custody::inspect(&f.root, MAX_SOURCE_BYTES, 1).is_err());
    let wrong_identity = (f.identity.0, f.identity.1 + 1, f.identity.2);
    assert!(custody::remove(&f.root, wrong_identity).is_err());
    let held = fs::File::open(&file).unwrap();
    assert!(custody::remove(&f.root, f.identity).is_err());
    drop(held);
    let link = f.root.join("internal-link");
    symlink("source.txt", &link).unwrap();
    assert!(custody::inspect(&f.root, MAX_SOURCE_BYTES, MAX_SOURCE_ENTRIES).is_ok());
    // The external link is a known flat fixture entry. Remove only this exact
    // symlink after the denial; never recursively clean a failed fixture.
    let external = f.root.join("external-link");
    symlink("/etc/passwd", &external).unwrap();
    assert!(custody::inspect(&f.root, MAX_SOURCE_BYTES, MAX_SOURCE_ENTRIES).is_err());
    assert!(
        fs::symlink_metadata(&external)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::remove_file(&external).unwrap();
}

#[test]
fn expired_acquisition_is_a_typed_timeout_without_running_git() {
    let f = Fixture::new();
    let mut session = Session {
        directory: f.root.clone(),
        deadline: Instant::now() - Duration::from_secs(1),
        step: 0,
    };
    let error = session.run(&f.root, &["version"]).unwrap_err();
    assert!(error.downcast_ref::<TimedOut>().is_some());
    assert_eq!(session.step, 0);
    assert_eq!(fs::read_dir(&f.root).unwrap().count(), 0);
}

#[test]
fn source_tree_cannot_read_sibling_acquisition_logs_through_a_tracked_link() {
    let f = Fixture::new();
    let source = f.root.join("source");
    fs::DirBuilder::new().mode(0o700).create(&source).unwrap();
    fs::write(f.root.join("git-00.stdout"), "outside the audited source\n").unwrap();
    symlink("../git-00.stdout", source.join("linked-policy")).unwrap();
    // Acquisition-directory containment alone is insufficient.
    custody::inspect(&f.root, MAX_SOURCE_BYTES, MAX_SOURCE_ENTRIES).unwrap();
    assert!(custody::inspect(&source, MAX_SOURCE_BYTES, MAX_SOURCE_ENTRIES).is_err());
    let mut session = Session {
        directory: f.root.clone(),
        deadline: Instant::now() + Duration::from_secs(30),
        step: 0,
    };
    assert!(prove(&mut session, &source, &"a".repeat(40)).is_err());
    assert_eq!(
        session.step, 0,
        "source custody must precede any source Git command"
    );
}

#[test]
fn initial_transport_cannot_discover_an_unrelated_parent_git_configuration() {
    let f = Fixture::new();
    let fixture_git = |args: &[&str]| {
        let output = crate::split_tree::source_git_command(&f.root)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "fixture Git failed");
    };
    fixture_git(&["init", "--template="]);
    fixture_git(&[
        "config",
        "url.https://fixture-credential@invalid.example/.insteadOf",
        "https://github.com/neverhuman/",
    ]);
    fixture_git(&[
        "config",
        "http.extraHeader",
        "Authorization: fixture-canary",
    ]);
    let output = f.root.join("output");
    fs::DirBuilder::new().mode(0o700).create(&output).unwrap();
    let mut session = Session {
        directory: output.clone(),
        deadline: Instant::now() + Duration::from_secs(30),
        step: 0,
    };
    let config = session
        .run(&output, &["config", "--list", "--show-origin"])
        .unwrap();
    assert!(!config.contains("fixture-canary") && !config.contains("fixture-credential"));
    // The boundary permits only the explicitly supplied repository itself,
    // which is how a fresh clone reads its generated remote.origin.url.
    assert_eq!(
        session
            .run(&f.root, &["config", "--get", "http.extraHeader"])
            .unwrap(),
        "Authorization: fixture-canary"
    );
}

#[test]
fn mapped_file_custody_preserves_spaces_and_refuses_ambiguous_backslashes() {
    let prefix = "00000000-00001000 r--p 00000000 00:01 42 ";
    let root = Path::new("/tmp/cache name");
    assert!(custody::check_mapping(&format!("{prefix}/tmp/cache name/source"), root).is_err());
    assert!(custody::check_mapping(&format!("{prefix}/tmp/other/source"), root).is_ok());
    assert!(
        custody::check_mapping(
            &format!("{prefix}/tmp/cache\\040name/source"),
            Path::new("/tmp/cache\\040name")
        )
        .is_err()
    );
    assert!(custody::check_mapping(&format!("{prefix}/tmp/other\\012name/source"), root).is_err());
}

#[test]
fn clone_from_selected_bare_input_has_exact_graph_and_rejects_additional_objects() {
    let f = Fixture::new();
    let scratch = f.root.join("scratch");
    fs::DirBuilder::new().mode(0o700).create(&scratch).unwrap();
    let input = scratch.join("input.git");
    fs::DirBuilder::new().mode(0o700).create(&input).unwrap();
    let fixture_git = |args: &[&str]| {
        let output = crate::split_tree::source_git_command(&input)
            .args([
                "-c",
                "user.name=Public source fixture",
                "-c",
                "user.email=fixture@jeryu.invalid",
                "-c",
                "core.logAllRefUpdates=false",
                "-c",
                "commit.gpgSign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "fixture Git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .trim_end()
            .to_owned()
    };
    fixture_git(&["init", "--bare", "--template="]);
    let tree = fixture_git(&["mktree"]);
    let commit = fixture_git(&["commit-tree", &tree, "-m", "selected fixture"]);
    // Git 2.55+ refuses update-ref --no-deref on the dangling symbolic HEAD
    // created by init --bare. Delete it, then attach a detached HEAD.
    let _ = crate::split_tree::source_git_command(&input)
        .args(["symbolic-ref", "--delete", "HEAD"])
        .output();
    fixture_git(&["update-ref", "--no-deref", "HEAD", &commit]);
    let mut session = Session {
        directory: f.root.clone(),
        deadline: Instant::now() + Duration::from_secs(30),
        step: 0,
    };
    let (source, expected_graph, source_identity) = clone_input(
        &mut session,
        &scratch,
        custody::owned_directory(&input).unwrap(),
        &commit,
        "https://github.com/neverhuman/jeryu.git",
    )
    .unwrap();
    assert_eq!(prove(&mut session, &source, &commit).unwrap(), tree);
    assert_eq!(
        graph::verify(&mut session, &source, &commit, false).unwrap(),
        expected_graph
    );
    assert!(session.run(&source, &["for-each-ref"]).unwrap().is_empty());
    assert_eq!(
        session
            .run(&source, &["config", "--get", "remote.origin.url"])
            .unwrap(),
        "https://github.com/neverhuman/jeryu.git"
    );
    // An unreferenced newer commit is still an additional audit input and must
    // not share the selected graph identity, even when HEAD/tree are unchanged.
    fixture_git(&["commit-tree", &tree, "-m", "unselected fixture"]);
    assert!(graph::verify(&mut session, &input, &commit, true).is_err());
    assert_eq!(
        graph::verify(&mut session, &source, &commit, false).unwrap(),
        expected_graph
    );
    // This source still has the admitted object set: only a new ref changes.
    session
        .run(&source, &["update-ref", "refs/heads/unselected", &commit])
        .unwrap();
    assert!(graph::verify(&mut session, &source, &commit, false).is_err());
    custody::unchanged_directory(&source, source_identity).unwrap();
    fs::rename(&source, scratch.join("original-source")).unwrap();
    fs::DirBuilder::new().mode(0o700).create(&source).unwrap();
    assert!(custody::unchanged_directory(&source, source_identity).is_err());
}

#[test]
fn actual_git_proof_rejects_wrong_commit_dirty_source_grafts_and_unacquired_gitlinks() {
    let f = Fixture::new();
    let path = f.root.join("source");
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    let fixture_git = |args: &[&str]| {
        let output = crate::split_tree::source_git_command(&path)
            .env("GIT_AUTHOR_NAME", "Public source fixture")
            .env("GIT_COMMITTER_NAME", "Public source fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@jeryu.invalid")
            .env("GIT_COMMITTER_EMAIL", "fixture@jeryu.invalid")
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgSign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "fixture Git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .trim_end()
            .to_owned()
    };
    fixture_git(&["init", "--initial-branch=main", "--template="]);
    fs::write(path.join("public.txt"), "public fixture source\n").unwrap();
    fixture_git(&["add", "public.txt"]);
    fixture_git(&["commit", "-m", "fixture"]);
    let commit = fixture_git(&["rev-parse", "HEAD"]);
    let expected_tree = fixture_git(&["rev-parse", "HEAD^{tree}"]);
    let mut session = Session {
        directory: f.root.clone(),
        deadline: Instant::now() + Duration::from_secs(30),
        step: 0,
    };
    assert_eq!(prove(&mut session, &path, &commit).unwrap(), expected_tree);
    assert!(prove(&mut session, &path, &"0".repeat(40)).is_err());
    fs::write(path.join("public.txt"), "changed source\n").unwrap();
    assert!(prove(&mut session, &path, &commit).is_err());
    fs::write(path.join("public.txt"), "public fixture source\n").unwrap();
    fs::create_dir_all(path.join(".git/info")).unwrap();
    fs::write(path.join(".git/info/grafts"), "").unwrap();
    assert!(prove(&mut session, &path, &commit).is_err());
    fs::remove_file(path.join(".git/info/grafts")).unwrap();
    fixture_git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{commit},unacquired"),
    ]);
    fixture_git(&["commit", "-m", "unacquired gitlink"]);
    let gitlink_commit = fixture_git(&["rev-parse", "HEAD"]);
    let error = prove(&mut session, &path, &gitlink_commit).unwrap_err();
    assert!(error.to_string().contains("unacquired submodules"));
}
