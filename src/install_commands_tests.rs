use super::*;
use tempfile::tempdir;

#[test]
fn tilde_expansion_targets_home() {
    let prefix = expand_tilde("~/.jeryu/bin");
    assert!(prefix.ends_with(".jeryu/bin"));
}

#[test]
fn install_plan_stays_user_space() {
    let plan = build_plan(
        "local",
        &InstallOptions {
            prefix: "/tmp/jeryu".into(),
            dry_run: true,
            json: true,
            yes: true,
            color: ColorMode::Never,
            interactive: InteractiveMode::Never,
            path_mode: PathMode::Advise,
            verbose: false,
            install_deps: false,
            allow_sudo: false,
        },
    );
    let rendered = serde_json::to_value(&plan).unwrap();
    let steps = rendered["steps"].as_array().unwrap();
    assert_eq!(rendered["mode"], "local");
    assert!(!rendered["install_deps"].as_bool().unwrap());
    assert!(!rendered["allow_sudo"].as_bool().unwrap());
    assert!(steps.iter().all(|step| {
        let label = step["label"].as_str().unwrap();
        let command = step["command"].as_str().unwrap();
        !label.contains("sudo")
            && !label.contains("python")
            && !label.contains("pip")
            && !command.contains("sudo")
            && !command.contains("python")
            && !command.contains("pip")
    }));
}

#[test]
fn guided_plan_includes_direct_repo_and_policy_steps() {
    let mut plan = build_plan(
        "guided",
        &InstallOptions {
            prefix: "/tmp/jeryu".into(),
            dry_run: true,
            json: true,
            yes: true,
            color: ColorMode::Never,
            interactive: InteractiveMode::Never,
            path_mode: PathMode::Advise,
            verbose: false,
            install_deps: false,
            allow_sudo: false,
        },
    );
    plan.steps.push(PlanStep {
        id: "repo-direct-setup".into(),
        label: "optionally create or adopt a direct repository".into(),
        detail: "new repos use origin for local JeRyu/GitLab; existing repos preserve origin and add jeryu unless --replace-origin is selected".into(),
        command: Some("jeryu repo adopt . --direct --namespace <namespace> --name <repo> --hooks off".into()),
        requires_sudo: false,
        estimated_seconds: Some(5),
    });
    plan.steps.push(PlanStep {
        id: "server-policy".into(),
        label: "apply server-side repository policy".into(),
        detail: "protect main as merge-request only and install `jeryu server-hook pre-receive` for protected-ref admission".into(),
        command: Some("jeryu policy audit --target local-gitlab".into()),
        requires_sudo: false,
        estimated_seconds: Some(10),
    });
    let rendered = serde_json::to_value(&plan).unwrap();
    let steps = rendered["steps"].as_array().unwrap();
    assert!(
        steps
            .iter()
            .any(|step| step["id"].as_str() == Some("repo-direct-setup"))
    );
    assert!(
        steps
            .iter()
            .any(|step| step["id"].as_str() == Some("server-policy"))
    );
}

#[test]
fn path_snippets_are_shell_specific() {
    assert!(path_snippet(Path::new("/tmp/bin"), Some("/bin/bash")).contains("export PATH"));
    assert!(path_snippet(Path::new("/tmp/bin"), Some("/usr/bin/fish")).contains("set -gx PATH"));
    assert!(path_snippet(Path::new("/tmp/bin"), Some("/bin/zsh")).contains(JERYU_PATH_START));
    assert!(path_snippet(Path::new("/tmp/bin"), Some("/bin/zsh")).contains(JERYU_PATH_END));
}

#[test]
fn strip_path_block_preserves_profile_content() {
    let text = concat!(
        "export BEFORE=1\n",
        "# >>> jeryu path >>>\n",
        "export PATH=\"/tmp/jeryu:$PATH\"\n",
        "# <<< jeryu path <<<\n",
        "alias gs='git status'\n",
    );
    let (updated, removed) = install_runtime::strip_jeryu_path_block(text);
    assert!(removed);
    assert_eq!(updated, "export BEFORE=1\nalias gs='git status'\n");
}

#[test]
fn strip_path_block_ignores_partial_marker() {
    let text = concat!(
        "export BEFORE=1\n",
        "# >>> jeryu path >>>\n",
        "export PATH=\"/tmp/jeryu:$PATH\"\n",
        "alias gs='git status'\n",
    );
    let (updated, removed) = install_runtime::strip_jeryu_path_block(text);
    assert!(!removed);
    assert_eq!(updated, text);
}

#[test]
fn remove_path_block_from_file_backs_up_profile() {
    let dir = tempdir().unwrap();
    let rc = dir.path().join("profile");
    fs::write(
        &rc,
        path_snippet(Path::new("/tmp/jeryu"), Some("/bin/bash")),
    )
    .unwrap();

    assert!(install_runtime::remove_path_block_from_file(&rc).unwrap());
    assert!(!install_runtime::has_jeryu_path_block(
        &fs::read_to_string(&rc).unwrap()
    ));
    assert!(rc.with_extension("jeryu-uninstall.bak").exists());
}

#[test]
fn plan_tracks_path_advice_for_unknown_prefix() {
    let plan = build_plan(
        "local",
        &InstallOptions {
            prefix: tempdir().unwrap().path().join("jeryu-bin"),
            dry_run: true,
            json: false,
            yes: true,
            color: ColorMode::Auto,
            interactive: InteractiveMode::Auto,
            path_mode: PathMode::Advise,
            verbose: false,
            install_deps: false,
            allow_sudo: false,
        },
    );
    assert!(plan.path_advice.is_some());
    assert!(plan.steps.iter().any(|step| step.id == "verify"));
}

#[test]
fn color_mode_respects_never_and_always() {
    assert!(!should_colorize(ColorMode::Never, false));
    assert!(should_colorize(ColorMode::Always, false));
}
