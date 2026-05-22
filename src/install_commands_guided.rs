use super::*;

pub(crate) async fn guided(opts: &InstallOptions) -> Result<i32> {
    let mut plan = build_plan("guided", opts);
    plan.steps.push(PlanStep {
        id: "optional-server".into(),
        label: "optionally install and start the local server".into(),
        detail: "run `jeryu install server`, then verify GitLab HTTP :8929, SSH :2224, webhook :9777, and Vault when enabled".into(),
        command: Some("jeryu install server --yes".into()),
        requires_sudo: false,
        estimated_seconds: Some(120),
    });
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
    plan.steps.push(PlanStep {
        id: "backup".into(),
        label: "optionally configure backup health checks".into(),
        detail: "store backup policy in .jeryu/backup.toml without SSH identities or credentials"
            .into(),
        command: Some("jeryu install doctor".into()),
        requires_sudo: false,
        estimated_seconds: Some(5),
    });

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        render_plan(&plan);
        println!("Final commands:");
        println!("  git status");
        println!("  git push");
        println!("  jeryu git push");
        println!("  jeryu system");
        println!("  jeryu install doctor");
    }
    if opts.dry_run {
        return Ok(0);
    }
    if !prompt_for_confirmation(&plan, opts)? {
        bail!("guided install cancelled");
    }
    install_runtime::install_binary(&opts.prefix).await?;
    println!(
        "Guided install completed binary installation. Run `jeryu install server` to start the local server."
    );
    Ok(0)
}
