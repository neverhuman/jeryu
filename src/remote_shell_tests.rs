use super::*;

fn sample_remote_config(alias: &str) -> RemoteConfig {
    let alias = alias.to_string();
    RemoteConfig {
        connection: build_remote_connection(alias.clone(), alias, 22, None),
        created_at_utc: "2026-05-04T00:00:00Z".into(),
        service_mode: ServiceMode::Auto,
    }
}

#[test]
fn default_alias_is_target_tail() {
    assert_eq!(default_alias("deploy@10.0.0.20"), "10.0.0.20");
    assert_eq!(default_alias("xbabe1"), "xbabe1");
}

#[test]
fn config_round_trip_contains_expected_paths() {
    let cfg = sample_remote_config("xbabe1");
    let text = toml::to_string_pretty(&cfg).unwrap();
    assert!(text.contains("remote_bin"));
    assert!(text.contains("~/.jeryu/bin/jeryu"));
    assert!(text.contains("service_mode"));
}

#[test]
fn remote_install_plan_includes_service_mode_and_steps() {
    let cfg = sample_remote_config("xbabe1");
    let plan = build_remote_plan(
        &cfg,
        true,
        &RemoteCommonOptions {
            dry_run: true,
            json: true,
            yes: true,
            color: ColorMode::Never,
            interactive: InteractiveMode::Never,
            service_mode: ServiceMode::Manual,
            verbose: false,
        },
    );
    let rendered = serde_json::to_value(&plan).unwrap();
    assert_eq!(rendered["service_mode"], "Manual");
    assert_eq!(rendered["setup_key"], true);
    assert!(
        rendered["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| { step["id"].as_str().unwrap() == "verify" })
    );
}

#[test]
fn remote_plan_is_json_serializable_without_network() {
    // Hold PATH_ENV_LOCK: build_remote_plan reads PATH via command_exists()
    // to detect ssh/ssh-keygen. Other tests mutate PATH in parallel which
    // causes flake (ssh_keygen present but ssh missing during the racy window).
    let _guard = crate::test_sync::PATH_ENV_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());

    let cfg = sample_remote_config("xbabe1");
    let plan = build_remote_plan(
        &cfg,
        false,
        &RemoteCommonOptions {
            dry_run: true,
            json: false,
            yes: true,
            color: ColorMode::Auto,
            interactive: InteractiveMode::Auto,
            service_mode: ServiceMode::Auto,
            verbose: false,
        },
    );
    assert_eq!(plan.action, "remote-install");
    assert!(!plan.preflight.local_ssh_keygen || plan.preflight.local_ssh);
}

#[test]
fn effective_service_mode_resolves_auto_by_preflight() {
    assert_eq!(
        effective_service_mode(ServiceMode::Auto, Some(true)),
        ServiceMode::User
    );
    assert_eq!(
        effective_service_mode(ServiceMode::Auto, Some(false)),
        ServiceMode::Manual
    );
    assert_eq!(
        effective_service_mode(ServiceMode::User, Some(false)),
        ServiceMode::User
    );
    assert_eq!(
        effective_service_mode(ServiceMode::Manual, Some(true)),
        ServiceMode::Manual
    );
}

#[test]
fn remote_config_defaults_service_mode_when_missing() {
    let text = r#"
alias = "xbabe1"
target = "xbabe1"
ssh_port = 22
remote_prefix = "~/.jeryu"
remote_bin = "~/.jeryu/bin/jeryu"
local_http_port = 8929
local_ssh_port = 2224
local_vault_port = 18200
local_webhook_port = 9777
created_at_utc = "2026-05-04T00:00:00Z"
"#;
    let cfg: RemoteConfig = toml::from_str(text).unwrap();
    assert_eq!(cfg.service_mode, ServiceMode::Auto);
}
