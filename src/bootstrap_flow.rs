use anyhow::Result;
use std::fs;

use crate::config;
use crate::docker::DockerCtl;
use crate::gitlab_client::GitlabClient;
use crate::secrets;
use crate::state::Db;

use super::generate_env_file;

#[path = "bootstrap_flow_steps.rs"]
mod steps;
use steps::{
    create_authenticated_client, create_runner_pools, install_gcd_service, print_bootstrap_summary,
    print_webhook_notes, run_smoke_test, wait_for_gitlab_ready,
};

pub async fn run_bootstrap() -> Result<()> {
    println!("🚀 jeryu bootstrap — headless GitLab setup\n");

    // Step 1: Generate secrets
    println!("  [1/8] Generating secrets...");
    let (root_password, _webhook_secret) = generate_env_file()?;

    // Step 2: Write docker-compose.yml
    println!("  [2/8] Writing docker-compose.yml...");
    let compose_content = config::render_compose(&root_password);
    let compose_path = config::compose_file();
    if let Some(parent) = compose_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&compose_path, &compose_content)?;

    // Ensure GitLab volume directories exist
    fs::create_dir_all(config::gitlab_config_dir())?;
    fs::create_dir_all(config::gitlab_logs_dir())?;
    fs::create_dir_all(config::gitlab_data_dir())?;
    fs::create_dir_all(config::runners_dir())?;
    fs::create_dir_all(config::vault_config_dir())?;
    fs::create_dir_all(config::vault_storage_dir())?;

    // Step 3: Start GitLab
    println!("  [3/8] Starting GitLab container (this may take a minute)...");
    let docker = DockerCtl::connect()?;
    docker.compose_up().await?;

    println!("  [3b/8] Provisioning local Vault...");
    let vault_status = secrets::run_secrets_provision(None).await?;
    println!(
        "    ✅ Vault ready at {} (mount={}, prefix={})",
        vault_status.addr, vault_status.mount, vault_status.prefix
    );

    println!("  [4/8] Waiting for GitLab to become ready...");
    let gitlab_url = format!("http://localhost:{}", config::GITLAB_HTTP_PORT);
    let client = GitlabClient::new(&gitlab_url, None);
    wait_for_gitlab_ready(&client).await?;

    // Step 5: Create root PAT programmatically (bypassing OAuth ROPC)
    println!("  [5/8] Creating root PAT via gitlab-rails...");
    let client = create_authenticated_client(&gitlab_url).await?;

    // Step 6: Create runner pools
    println!("  [6/8] Creating runner pools...");
    let db = Db::open().await?;
    create_runner_pools(&client, &db).await?;

    // Step 7: Register webhook
    print_webhook_notes();

    // Step 8: Install always-on disk daemon (jeryu-gcd.service).
    // Maintains df >= 80 GiB free; the rest of bootstrap depends on this.
    // Skipped when JERYU_BOOTSTRAP_SKIP_GCD=1 (CI/containers) or when
    // systemctl is unavailable.
    install_gcd_service().await?;

    // Step 9: Validation (The Smoke Test)
    println!("  [9/9] Running smoke test (validating end-to-end CI)...");
    run_smoke_test(&client).await?;

    // Summary
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    print_bootstrap_summary(&gitlab_url, &db).await?;

    Ok(())
}
