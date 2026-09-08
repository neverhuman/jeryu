use super::*;

#[test]
fn commit_skipping_the_audit_gate_is_denied() {
    for command in [
        vec!["commit", "--no-verify"],
        vec!["commit", "-n"],
        vec!["commit", "-n", "-m", "msg"],
        vec!["commit", "-am", "msg", "--no-verify"],
        vec!["commit", "-an", "msg"],
    ] {
        let decision = decide(&command);
        assert!(!decision.is_allowed(), "expected deny: git {command:?}");
        assert_eq!(reason(&decision), "git_commit_unverified_denied");
    }
    assert!(decide(&["commit", "-m", "msg", "--", "-n"]).is_allowed());
}

#[test]
fn commit_hookspath_injection_stays_denied() {
    assert_eq!(
        reason(&decide(&["-c", "core.hooksPath=/tmp", "commit", "-m", "x"])),
        "git_global_flag_denied"
    );
    assert_eq!(
        reason(&decide(&["config", "core.hooksPath", "/tmp"])),
        "git_config_write_denied"
    );
}

#[test]
fn network_commands_are_denied() {
    for command in [
        vec!["push"],
        vec!["push", "origin", "HEAD"],
        vec!["push", "--force"],
        vec!["fetch"],
        vec!["fetch", "origin"],
        vec!["pull"],
        vec!["clone", "https://example.com/x.git"],
        vec!["remote", "add", "evil", "https://x"],
        vec!["remote", "-v"],
        vec!["ls-remote"],
        vec!["send-email", "x.patch"],
        vec!["format-patch", "-1"],
    ] {
        assert_eq!(reason(&decide(&command)), "git_network_denied");
    }
}

#[test]
fn worktree_submodule_and_integration_commands_are_denied() {
    let decision = decide(&["worktree", "add", "../wt"]);
    assert_eq!(reason(&decision), "git_worktree_denied");
    if let GitDecision::Deny(error) = decision {
        assert!(error.repair_hint.contains("more runners"));
    }
    assert_eq!(
        reason(&decide(&["submodule", "add", "x"])),
        "git_worktree_denied"
    );
    for command in [
        vec!["merge", "other"],
        vec!["rebase", "main"],
        vec!["cherry-pick", "abc123"],
        vec!["am", "x.patch"],
        vec!["apply", "x.patch"],
        vec!["gc"],
        vec!["update-ref", "refs/heads/x", "HEAD"],
        vec!["filter-branch"],
    ] {
        assert_eq!(reason(&decide(&command)), "git_integration_denied");
    }
}

#[test]
fn config_injection_globals_are_denied() {
    for command in [
        vec!["-c", "alias.x=!sh", "status"],
        vec!["-c", "core.fsmonitor=evil", "status"],
        vec!["--git-dir", "/other/.git", "log"],
        vec!["--git-dir=/other/.git", "log"],
        vec!["--work-tree", "/other", "status"],
        vec!["--exec-path=/evil", "status"],
    ] {
        assert_eq!(reason(&decide(&command)), "git_global_flag_denied");
    }
}

#[test]
fn unknown_subcommands_and_aliases_are_denied() {
    assert_eq!(reason(&decide(&["wtf"])), "git_subcommand_denied");
    assert_eq!(reason(&decide(&["my-alias"])), "git_subcommand_denied");
}

#[test]
fn error_renders_typed_block() {
    if let GitDecision::Deny(error) = decide(&["push", "origin", "HEAD"]) {
        let rendered = error.render();
        assert!(rendered.contains("refused `git push origin HEAD`"));
        assert!(rendered.contains("git_network_denied"));
        assert!(rendered.contains("fixes:"));
        assert!(rendered.contains("docs:"));
        assert_eq!(error.command(), "git push origin HEAD");
    } else {
        panic!("expected deny");
    }
}
