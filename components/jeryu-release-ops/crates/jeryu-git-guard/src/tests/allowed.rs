use super::*;

#[test]
fn local_edit_and_read_allowed() {
    for command in [
        vec!["status"],
        vec!["status", "--short"],
        vec!["diff"],
        vec!["diff", "--cached"],
        vec!["log", "--oneline", "-10"],
        vec!["show", "HEAD"],
        vec!["add", "-A"],
        vec!["add", "src/lib.rs"],
        vec!["commit", "-m", "work"],
        vec!["commit", "--amend", "--no-edit"],
        vec!["revert", "HEAD"],
        vec!["reset", "--hard", "HEAD~1"],
        vec!["restore", "src/lib.rs"],
        vec!["restore", "--staged", "src/lib.rs"],
        vec!["rm", "old.rs"],
        vec!["mv", "a.rs", "b.rs"],
        vec!["stash"],
        vec!["stash", "pop"],
        vec!["rev-parse", "HEAD"],
        vec!["blame", "src/lib.rs"],
        vec!["grep", "needle"],
    ] {
        assert!(
            decide(&command).is_allowed(),
            "expected allow: git {command:?}"
        );
    }
}

#[test]
fn leading_subcommand_skips_globals() {
    assert_eq!(
        leading_subcommand(&argv(&["commit", "-m", "x"])),
        Some("commit")
    );
    assert_eq!(
        leading_subcommand(&argv(&["-C", "/repo", "commit", "-m", "x"])),
        Some("commit")
    );
    assert_eq!(leading_subcommand(&argv(&["status"])), Some("status"));
    assert_eq!(leading_subcommand(&argv(&["--version"])), None);
}

#[test]
fn branch_listing_allowed_creation_denied() {
    for command in [
        vec!["branch"],
        vec!["branch", "-a"],
        vec!["branch", "-v"],
        vec!["branch", "--list"],
        vec!["branch", "--show-current"],
        vec!["branch", "--contains", "HEAD"],
        vec!["branch", "--merged", "main"],
    ] {
        assert!(
            decide(&command).is_allowed(),
            "expected allow: git {command:?}"
        );
    }
    for command in [
        vec!["branch", "newbranch"],
        vec!["branch", "newbranch", "main"],
        vec!["branch", "-d", "old"],
        vec!["branch", "-D", "old"],
        vec!["branch", "-m", "old", "new"],
        vec!["branch", "-c", "old", "copy"],
        vec!["branch", "-u", "origin/x"],
        vec!["branch", "--set-upstream-to", "origin/x"],
    ] {
        assert_eq!(
            reason(&decide(&command)),
            "git_branch_denied",
            "git {command:?}"
        );
    }
}

#[test]
fn checkout_and_switch_stay_on_the_assigned_branch() {
    assert!(decide(&["checkout", "--", "src/lib.rs"]).is_allowed());
    assert!(decide(&["checkout", "."]).is_allowed());
    assert!(decide(&["checkout", "agents/a/wc/feature"]).is_allowed());
    assert_eq!(
        reason(&decide(&["checkout", "-b", "newbranch"])),
        "git_branch_denied"
    );
    assert_eq!(reason(&decide(&["checkout", "main"])), "git_branch_denied");
    assert_eq!(
        reason(&decide(&["checkout", "--detach", "HEAD"])),
        "git_branch_denied"
    );
    assert!(decide(&["switch", "agents/a/wc/feature"]).is_allowed());
    assert_eq!(
        reason(&decide(&["switch", "-c", "new"])),
        "git_branch_denied"
    );
    assert_eq!(reason(&decide(&["switch", "main"])), "git_branch_denied");
    assert_eq!(reason(&decide(&["switch", "-"])), "git_branch_denied");
}

#[test]
fn config_and_tag_reads_are_allowed_but_writes_are_denied() {
    assert!(decide(&["config", "user.name"]).is_allowed());
    assert!(decide(&["config", "--get", "user.email"]).is_allowed());
    assert!(decide(&["config", "--list"]).is_allowed());
    assert!(decide(&["config", "--get-regexp", "^user"]).is_allowed());
    assert_eq!(
        reason(&decide(&["config", "user.email", "me@x"])),
        "git_config_write_denied"
    );
    assert_eq!(
        reason(&decide(&["config", "--global", "user.name", "X"])),
        "git_config_write_denied"
    );
    assert_eq!(
        reason(&decide(&["config", "alias.x", "!sh"])),
        "git_config_write_denied"
    );
    assert_eq!(
        reason(&decide(&["config", "--unset", "core.x"])),
        "git_config_write_denied"
    );
    assert!(decide(&["tag"]).is_allowed());
    assert!(decide(&["tag", "-l"]).is_allowed());
    assert!(decide(&["tag", "-l", "v*"]).is_allowed());
    assert_eq!(reason(&decide(&["tag", "v1.0"])), "git_branch_denied");
    assert_eq!(
        reason(&decide(&["tag", "-a", "v1", "-m", "x"])),
        "git_branch_denied"
    );
    assert_eq!(reason(&decide(&["tag", "-d", "v1"])), "git_branch_denied");
}

#[test]
fn benign_globals_bare_git_and_reflog_reads_are_allowed() {
    assert!(decide(&["--no-pager", "log"]).is_allowed());
    assert!(decide(&["-C", "subdir", "status"]).is_allowed());
    assert!(decide(&["--paginate", "diff"]).is_allowed());
    assert_eq!(
        reason(&decide(&["--no-pager", "push"])),
        "git_network_denied"
    );
    assert!(git_command_decision(&[], "b").is_allowed());
    assert!(decide(&["--version"]).is_allowed());
    assert!(decide(&["--help"]).is_allowed());
    assert!(decide(&["reflog"]).is_allowed());
    assert!(decide(&["reflog", "show"]).is_allowed());
    assert_eq!(
        reason(&decide(&["reflog", "expire", "--all"])),
        "git_integration_denied"
    );
    assert_eq!(
        reason(&decide(&["reflog", "delete", "HEAD@{0}"])),
        "git_integration_denied"
    );
}
