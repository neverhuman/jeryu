use super::*;
use std::cell::Cell;

/// Scripted [`DeviceFlowHttp`] used in place of the network. Each refresh
/// returns a distinct access token so freshness is observable.
struct FakeDeviceFlow {
    poll_access: String,
    poll_refresh: Option<String>,
    poll_expiry: u64,
    refreshed_expiry: u64,
    refresh_calls: Cell<u64>,
}

impl FakeDeviceFlow {
    fn new(refresh: Option<&str>) -> Self {
        Self {
            poll_access: "access-initial".to_string(),
            poll_refresh: refresh.map(str::to_string),
            poll_expiry: 1_000,
            refreshed_expiry: 1_000_000,
            refresh_calls: Cell::new(0),
        }
    }

    fn refresh_calls(&self) -> u64 {
        self.refresh_calls.get()
    }
}

impl DeviceFlowHttp for FakeDeviceFlow {
    fn start(&self, _tool: AgentToolKind) -> Result<DeviceFlowStart, AgentAuthError> {
        Ok(DeviceFlowStart {
            verification_uri: "https://example.test/device".to_string(),
            user_code: "WXYZ-1234".to_string(),
            device_code: "device-code".to_string(),
            interval_secs: 5,
            expires_in_secs: 900,
        })
    }

    fn poll(
        &self,
        _tool: AgentToolKind,
        _device_code: &str,
    ) -> Result<DeviceCredential, AgentAuthError> {
        Ok(DeviceCredential {
            access_token: self.poll_access.clone(),
            refresh_token: self.poll_refresh.clone(),
            expires_at_unix: self.poll_expiry,
            token_type: "Bearer".to_string(),
            scopes: vec!["user:inference".to_string()],
        })
    }

    fn refresh(
        &self,
        _tool: AgentToolKind,
        _refresh_token: &str,
    ) -> Result<DeviceCredential, AgentAuthError> {
        let call = self.refresh_calls.get() + 1;
        self.refresh_calls.set(call);
        Ok(DeviceCredential {
            access_token: format!("access-refreshed-{call}"),
            // The upstream omits a rotated refresh token; the host store must
            // preserve its own copy.
            refresh_token: None,
            expires_at_unix: self.refreshed_expiry,
            token_type: "Bearer".to_string(),
            scopes: vec!["user:inference".to_string()],
        })
    }
}

fn credential(access: &str, refresh: Option<&str>, expires_at_unix: u64) -> DeviceCredential {
    DeviceCredential {
        access_token: access.to_string(),
        refresh_token: refresh.map(str::to_string),
        expires_at_unix,
        token_type: "Bearer".to_string(),
        scopes: vec!["user:inference".to_string()],
    }
}

/// import_from_device persists credential.json at 0600 and the receipt
/// carries no secret value.
#[test]
fn import_from_device_persists_private_credential_without_secret_in_receipt() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data = temp.path().join("data");
    let cred = credential("access-topsecret", Some("refresh-topsecret"), 1_000);

    let receipt = import_from_device(&data, AgentToolKind::Claude, &cred).expect("import succeeds");

    assert_eq!(receipt.tool, AgentToolKind::Claude);
    assert_eq!(receipt.files.len(), 1);
    assert_eq!(receipt.files[0].mode, "0600");
    assert!(receipt.files[0].digest.starts_with("sha256:"));
    assert!(data.join("agent-auth/claude/credential.json").is_file());
    let rendered = serde_json::to_string(&receipt).expect("receipt json");
    assert!(!rendered.contains("topsecret"));
}

/// load_credential round-trips the persisted credential exactly.
#[test]
fn load_credential_round_trips_persisted_credential() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data = temp.path().join("data");
    let cred = credential("access-a", Some("refresh-a"), 4_242);

    import_from_device(&data, AgentToolKind::Codex, &cred).expect("import");
    let loaded = load_credential(&data, AgentToolKind::Codex).expect("load");

    assert_eq!(loaded, cred);
}

/// is_expiring is true at or past expiry once skew is applied, false before.
#[test]
fn is_expiring_is_true_past_expiry_and_skew_false_otherwise() {
    let cred = credential("access", None, 1_000);
    assert!(is_expiring(&cred, 1_000, 0));
    assert!(is_expiring(&cred, 1_001, 0));
    assert!(is_expiring(&cred, 950, 100));
    assert!(!is_expiring(&cred, 900, 0));
    assert!(!is_expiring(&cred, 800, 100));
}

/// refresh_if_expiring refreshes when expiring with a refresh token, rewrites
/// the store with a different access token, and preserves the refresh token.
#[test]
fn refresh_if_expiring_rewrites_store_when_expiring_with_refresh_token() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data = temp.path().join("data");
    let http = FakeDeviceFlow::new(Some("refresh-keep"));
    import_from_device(
        &data,
        AgentToolKind::Claude,
        &credential("access-initial", Some("refresh-keep"), 1_000),
    )
    .expect("import");

    let refreshed =
        refresh_if_expiring(&data, AgentToolKind::Claude, &http, 1_000).expect("refresh succeeds");

    assert_eq!(http.refresh_calls(), 1);
    assert_ne!(refreshed.access_token, "access-initial");
    assert_eq!(refreshed.refresh_token.as_deref(), Some("refresh-keep"));
    let stored = load_credential(&data, AgentToolKind::Claude).expect("reload");
    assert_eq!(stored, refreshed);
}

/// refresh_if_expiring is a no-op when the credential is not expiring.
#[test]
fn refresh_if_expiring_is_noop_when_not_expiring() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data = temp.path().join("data");
    let http = FakeDeviceFlow::new(Some("refresh-keep"));
    import_from_device(
        &data,
        AgentToolKind::Claude,
        &credential("access-initial", Some("refresh-keep"), 1_000_000),
    )
    .expect("import");

    let result =
        refresh_if_expiring(&data, AgentToolKind::Claude, &http, 1_000).expect("no refresh needed");

    assert_eq!(http.refresh_calls(), 0);
    assert_eq!(result.access_token, "access-initial");
}

/// refresh_if_expiring cannot refresh without a refresh token and returns the
/// stored credential unchanged.
#[test]
fn refresh_if_expiring_is_noop_without_refresh_token() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data = temp.path().join("data");
    let http = FakeDeviceFlow::new(None);
    import_from_device(
        &data,
        AgentToolKind::Claude,
        &credential("access-initial", None, 1_000),
    )
    .expect("import");

    let result = refresh_if_expiring(&data, AgentToolKind::Claude, &http, 1_000)
        .expect("nothing to refresh");

    assert_eq!(http.refresh_calls(), 0);
    assert_eq!(result.access_token, "access-initial");
}

/// mint_run_credential returns a credential with a fresh access token and no
/// refresh token.
#[test]
fn mint_run_credential_strips_refresh_token_and_mints_fresh_access() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data = temp.path().join("data");
    let http = FakeDeviceFlow::new(Some("refresh-keep"));
    import_from_device(
        &data,
        AgentToolKind::Codex,
        &credential("access-initial", Some("refresh-keep"), 1_000_000),
    )
    .expect("import");

    let minted = mint_run_credential(&data, AgentToolKind::Codex, &http, 1_000).expect("mint");

    assert_eq!(minted.refresh_token, None);
    assert_ne!(minted.access_token, "access-initial");
    assert!(minted.access_token.starts_with("access-refreshed-"));
}

/// The core security property: the run-home credential carries the access
/// token but never the refresh token.
#[test]
fn materialize_run_credential_writes_access_without_refresh_token() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data = temp.path().join("data");
    let run_home = temp.path().join("run-home");
    let http = FakeDeviceFlow::new(Some("refresh-SUPERSECRET-zzz"));
    import_from_device(
        &data,
        AgentToolKind::Claude,
        &credential("access-initial", Some("refresh-SUPERSECRET-zzz"), 1_000_000),
    )
    .expect("import");

    let receipt = materialize_run_credential(&data, AgentToolKind::Claude, &run_home, &http, 1_000)
        .expect("materialize");

    assert_eq!(receipt.files.len(), 1);
    assert_eq!(receipt.files[0].mode, "0600");
    let run_path = run_home.join(".claude/credential.json");
    let raw = std::fs::read_to_string(&run_path).expect("run credential");
    assert!(
        !raw.contains("refresh-SUPERSECRET-zzz"),
        "refresh token leaked into the run home"
    );
    let parsed: DeviceCredential = serde_json::from_str(&raw).expect("parse run credential");
    assert_eq!(parsed.refresh_token, None);
    assert!(parsed.access_token.starts_with("access-refreshed-"));
    // The host store still holds the refresh token.
    let stored = load_credential(&data, AgentToolKind::Claude).expect("reload");
    assert_eq!(
        stored.refresh_token.as_deref(),
        Some("refresh-SUPERSECRET-zzz")
    );
}

/// A keychain-only/host-bound credential surfaces the typed repair.
#[test]
fn host_bound_credential_surfaces_typed_repair() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data = temp.path().join("data");
    let run_home = temp.path().join("run-home");
    let http = FakeDeviceFlow::new(None);
    import_from_device(
        &data,
        AgentToolKind::Claude,
        &credential("", None, 1_000_000),
    )
    .expect("import");

    let error = materialize_run_credential(&data, AgentToolKind::Claude, &run_home, &http, 1_000)
        .expect_err("host-bound credential denied");

    assert_eq!(error.code, "agent_auth_host_bound");
    assert!(!error.repair.common_fixes.is_empty());
    assert!(!error.repair.repair_hint.is_empty());
}
