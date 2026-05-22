use anyhow::{Context, Result};
use std::time::{Duration, Instant};

use crate::config;
use crate::gitlab_client::GitlabClient;
use crate::state::{Db, Pool};

use super::super::{append_env, generate_password};

pub(crate) async fn wait_for_gitlab_ready(client: &GitlabClient) -> Result<()> {
    let max_wait = Duration::from_secs(300); // 5 minutes
    let poll_interval = Duration::from_secs(5);
    let start = Instant::now();

    loop {
        if client.is_ready().await {
            println!("    ✅ GitLab is ready!");
            break;
        }
        if start.elapsed() > max_wait {
            anyhow::bail!(
                "GitLab did not become ready within {} seconds. \
                 Check `docker logs jeryu-gitlab` for details.",
                max_wait.as_secs()
            );
        }
        print!(
            "    ⏳ waiting ({:.0}s)...\r",
            start.elapsed().as_secs_f64()
        );
        tokio::time::sleep(poll_interval).await;
    }

    Ok(())
}

pub(crate) async fn create_authenticated_client(gitlab_url: &str) -> Result<GitlabClient> {
    let pat = format!("jeryu-pat-{}", generate_password(20));

    let rails_script = format!(
        "u = User.find_by_username('root');\
         t = u.personal_access_tokens.create!(scopes: ['api', 'create_runner', 'manage_runner', 'read_repository', 'write_repository'], name: 'jeryu-control-plane', expires_at: 365.days.from_now);\
         t.set_token('{}');\
         t.save!",
        pat
    );

    let output = tokio::process::Command::new("docker")
        .args([
            "exec",
            "jeryu-gitlab",
            "gitlab-rails",
            "runner",
            &rails_script,
        ])
        .output()
        .await
        .context("running gitlab-rails runner to create PAT")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("failed to create PAT via gitlab-rails: {}", stderr);
    }

    append_env("GITLAB_PAT", &pat)?;
    println!("    ✅ PAT stored in jeryu.env");
    Ok(GitlabClient::new(gitlab_url, Some(pat)))
}

pub(crate) async fn create_runner_pools(client: &GitlabClient, db: &Db) -> Result<()> {
    for pool_def in config::DEFAULT_POOLS {
        let tags: Vec<&str> = pool_def.tags.split(',').collect();
        let runner = client
            .create_runner(
                &format!("jeryu-{}", pool_def.name),
                &tags,
                pool_def.name == "default", // only default runs untagged
                "instance_type",
            )
            .await
            .with_context(|| format!("creating runner pool: {}", pool_def.name))?;

        let env_key = format!("RUNNER_TOKEN_{}", pool_def.name.to_uppercase());
        append_env(&env_key, &runner.token)?;

        let pool = Pool {
            name: pool_def.name.to_string(),
            gitlab_runner_id: runner.id,
            auth_token: runner.token.clone(),
            tags: pool_def.tags.to_string(),
            executor: pool_def.executor.to_string(),
            min_warm: pool_def.min_warm,
            max_managers: pool_def.max_managers,
            concurrent: pool_def.concurrent,
            request_concurrency: pool_def.request_concurrency,
            paused: false,
            trust_tier: pool_def.trust_tier.to_string(),
        };
        db.insert_pool(&pool).await?;

        println!(
            "    ✅ Pool '{}' created (runner_id={})",
            pool_def.name, runner.id
        );
    }

    Ok(())
}

pub(crate) fn print_webhook_notes() {
    println!("  [7/8] Webhook registration...");
    println!("    ℹ️  Webhook will be registered when the first project/group is created.");
    println!("    ℹ️  Webhook secret stored in jeryu.env for later use.");
}

pub(crate) async fn install_gcd_service() -> Result<()> {
    if std::env::var("JERYU_BOOTSTRAP_SKIP_GCD").as_deref() == Ok("1") {
        println!("  [8/9] jeryu-gcd install... ⊘ skipped (JERYU_BOOTSTRAP_SKIP_GCD=1)");
        return Ok(());
    }

    println!("  [8/9] Installing jeryu-gcd.service (always-on disk daemon)...");
    match crate::host::install_gcd_service(true).await {
        Ok(_) => println!("    ✅ jeryu-gcd.service installed and enabled."),
        Err(err) => {
            eprintln!(
                "    ⚠️  jeryu-gcd install failed (continuing bootstrap): {err}\n       Run `jeryu host install-gcd-service --allow-sudo` manually."
            );
        }
    }

    Ok(())
}

pub(crate) async fn run_smoke_test(client: &GitlabClient) -> Result<()> {
    let test_project = client.create_project("jeryu-smoke-test").await?;

    let ci_yaml = r#"
smoke_test:
  tags: [rust]
  script:
    - echo "━━━ jeryu smoke test ━━━"
    - echo "God Mode: Enabled"
    - hostname
    - uname -a
"#;
    client
        .create_file(
            test_project.id,
            "main",
            ".gitlab-ci.yml",
            ci_yaml,
            "Init smoke test",
        )
        .await?;
    println!("    ✅ Smoke test project created and pipeline triggered.");
    println!("    ℹ️  Run `jeryu up` to start the engine and process this job.");

    Ok(())
}

pub(crate) async fn print_bootstrap_summary(gitlab_url: &str, db: &Db) -> Result<()> {
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  ✅ jeryu bootstrap complete!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("  GitLab URL:     {}", gitlab_url);
    println!(
        "  SSH:            ssh://git@localhost:{}",
        config::GITLAB_SSH_PORT
    );
    println!("  Root user:      root");
    println!("  Secrets file:   {}", config::env_file().display());
    println!(
        "  Database:       {}",
        crate::db::config::state_path().display()
    );
    println!("  Runner configs: {}", config::runners_dir().display());
    println!();
    println!("  Pools:");
    let pools = match db.list_pools().await {
        Ok(p) => p,
        Err(_) => Vec::new(),
    };
    for p in &pools {
        let active = db.count_active_managers(&p.name).await.unwrap_or(0);
        println!(
            "    {:<15} runners={:<3} managers={}/{}",
            p.name, p.gitlab_runner_id, active, p.max_managers
        );
    }
    println!();
    println!("  Next steps:");
    println!("    jeryu status          — check system health");
    println!("    jeryu pool list       — show pools and managers");
    println!("    jeryu agent spawn ... — launch an autonomous agent");
    println!();

    Ok(())
}
