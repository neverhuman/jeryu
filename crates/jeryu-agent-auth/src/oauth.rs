//! Device-authorization inheritance for agent-edit native CLIs.
//!
//! The host runs the Codex/Claude device login once, persists the resulting
//! credential into Jeryu-owned storage, and then mints a short-lived,
//! access-token-only credential into each per-run home. The refresh token never
//! leaves the host store and never enters a run home, so a sandboxed agent can
//! call the upstream API without holding the long-lived secret.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{fs_error, missing_auth};
use crate::fs_receipt::{
    auth_dir, create_private_dir, write_private_file, write_private_file_atomic,
};
use crate::{AgentAuthError, AgentToolKind, AuthImportReceipt, RunAuthReceipt};

/// Credential file name written into the Jeryu store and into a run home.
const CREDENTIAL_FILE: &str = "credential.json";

/// Refresh the host credential this many seconds before it actually expires so
/// a run never inherits a token that lapses mid-task.
const REFRESH_SKEW_SECS: u64 = 60;

/// Public device-flow client id for the Anthropic Claude CLI. Hosts may build a
/// custom [`DeviceFlowEndpoints`] to override it.
const ANTHROPIC_DEVICE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

/// Public device-flow client id for the OpenAI Codex CLI. Hosts may build a
/// custom [`DeviceFlowEndpoints`] to override it.
const OPENAI_DEVICE_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

/// Server response that opens an OAuth 2.0 device authorization grant
/// (RFC 8628). The operator visits `verification_uri` and enters `user_code`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceFlowStart {
    /// URL the operator opens to approve the device.
    pub verification_uri: String,
    /// Short code the operator types at `verification_uri`.
    pub user_code: String,
    /// Opaque code the host polls the token endpoint with.
    pub device_code: String,
    /// Minimum seconds to wait between token polls.
    pub interval_secs: u64,
    /// Seconds until the device code expires.
    pub expires_in_secs: u64,
}

/// A persisted device credential. Stored host-side with the refresh token; the
/// run-home copy is minted without one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCredential {
    /// Bearer access token presented to the upstream API.
    pub access_token: String,
    /// Long-lived refresh token. Present in the host store, never in a run home.
    pub refresh_token: Option<String>,
    /// Unix time (seconds) when `access_token` expires.
    pub expires_at_unix: u64,
    /// OAuth token type, usually `Bearer`.
    pub token_type: String,
    /// Granted OAuth scopes.
    pub scopes: Vec<String>,
}

/// Network seam for the device-authorization grant. Implementations perform the
/// upstream HTTP exchange; all host-side credential logic is written against
/// this trait so it can be exercised without a network.
pub trait DeviceFlowHttp {
    /// Open a device authorization request and return the operator prompt.
    fn start(&self, tool: AgentToolKind) -> Result<DeviceFlowStart, AgentAuthError>;
    /// Poll the token endpoint with `device_code` once the operator approves.
    fn poll(
        &self,
        tool: AgentToolKind,
        device_code: &str,
    ) -> Result<DeviceCredential, AgentAuthError>;
    /// Exchange a refresh token for a credential with a fresh access token.
    fn refresh(
        &self,
        tool: AgentToolKind,
        refresh_token: &str,
    ) -> Result<DeviceCredential, AgentAuthError>;
}

/// Raw HTTP transport owned by the host process. [`UpstreamDeviceFlow`] owns the
/// OAuth encoding and decoding; the host owns the socket and its client.
pub trait HttpTransport {
    /// POST `form` as `application/x-www-form-urlencoded` to `url` and return the
    /// JSON response body as text.
    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<String, AgentAuthError>;
}

/// Provider endpoints for the device authorization grant. `for_tool` returns the
/// documented Anthropic and OpenAI defaults; hosts may construct their own.
#[derive(Debug, Clone)]
pub struct DeviceFlowEndpoints {
    /// Device authorization endpoint that opens the flow.
    pub device_code_url: String,
    /// Token endpoint polled for the credential and used for refreshes.
    pub token_url: String,
    /// OAuth client id presented to the provider.
    pub client_id: String,
    /// Scopes requested at device authorization time.
    pub scopes: Vec<String>,
}

impl DeviceFlowEndpoints {
    /// Documented device-flow endpoints for `tool`.
    pub fn for_tool(tool: AgentToolKind) -> Result<Self, AgentAuthError> {
        match tool {
            AgentToolKind::Claude => Ok(Self {
                device_code_url: "https://console.anthropic.com/v1/oauth/device/code".to_string(),
                token_url: "https://console.anthropic.com/v1/oauth/token".to_string(),
                client_id: ANTHROPIC_DEVICE_CLIENT_ID.to_string(),
                scopes: vec![
                    "org:create_api_key".to_string(),
                    "user:inference".to_string(),
                ],
            }),
            AgentToolKind::Codex => Ok(Self {
                device_code_url: "https://auth.openai.com/oauth/device/code".to_string(),
                token_url: "https://auth.openai.com/oauth/token".to_string(),
                client_id: OPENAI_DEVICE_CLIENT_ID.to_string(),
                scopes: vec!["openid".to_string(), "offline_access".to_string()],
            }),
            AgentToolKind::Jekko => Err(device_unsupported(tool)),
        }
    }
}

/// [`DeviceFlowHttp`] implementation that speaks the real Anthropic and OpenAI
/// device-grant protocol over a host-supplied [`HttpTransport`].
pub struct UpstreamDeviceFlow<T: HttpTransport> {
    transport: T,
}

impl<T: HttpTransport> UpstreamDeviceFlow<T> {
    /// Build the upstream flow over `transport`.
    pub fn new(transport: T) -> Self {
        Self { transport }
    }
}

impl<T: HttpTransport> DeviceFlowHttp for UpstreamDeviceFlow<T> {
    fn start(&self, tool: AgentToolKind) -> Result<DeviceFlowStart, AgentAuthError> {
        let endpoints = DeviceFlowEndpoints::for_tool(tool)?;
        let scope = endpoints.scopes.join(" ");
        let body = self.transport.post_form(
            &endpoints.device_code_url,
            &[("client_id", &endpoints.client_id), ("scope", &scope)],
        )?;
        parse_device_start(&body)
    }

    fn poll(
        &self,
        tool: AgentToolKind,
        device_code: &str,
    ) -> Result<DeviceCredential, AgentAuthError> {
        let endpoints = DeviceFlowEndpoints::for_tool(tool)?;
        let body = self.transport.post_form(
            &endpoints.token_url,
            &[
                ("client_id", &endpoints.client_id),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ],
        )?;
        parse_token(&body)
    }

    fn refresh(
        &self,
        tool: AgentToolKind,
        refresh_token: &str,
    ) -> Result<DeviceCredential, AgentAuthError> {
        let endpoints = DeviceFlowEndpoints::for_tool(tool)?;
        let body = self.transport.post_form(
            &endpoints.token_url,
            &[
                ("client_id", &endpoints.client_id),
                ("refresh_token", refresh_token),
                ("grant_type", "refresh_token"),
            ],
        )?;
        parse_token(&body)
    }
}

/// Persist a device credential into `data_home/agent-auth/<tool>/credential.json`
/// at mode 0600, mirroring [`crate::import_from_host`]. The receipt records the
/// path, mode, and digest only.
pub fn import_from_device(
    data_home: &Path,
    tool: AgentToolKind,
    cred: &DeviceCredential,
) -> Result<AuthImportReceipt, AgentAuthError> {
    persist_credential(data_home, tool, cred)
}

/// Load the persisted device credential for `tool`.
pub fn load_credential(
    data_home: &Path,
    tool: AgentToolKind,
) -> Result<DeviceCredential, AgentAuthError> {
    let path = credential_store_path(data_home, tool);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(missing_auth(tool));
        }
        Err(error) => return Err(fs_error(error)),
    };
    deserialize_credential(&bytes)
}

/// Whether `cred` is at or past expiry once `skew_secs` of headroom is applied.
pub fn is_expiring(cred: &DeviceCredential, now_unix: u64, skew_secs: u64) -> bool {
    cred.expires_at_unix <= now_unix.saturating_add(skew_secs)
}

/// Refresh the host credential when it is expiring and a refresh token exists,
/// rewriting the store atomically. The refresh token stays in the host store.
pub fn refresh_if_expiring(
    data_home: &Path,
    tool: AgentToolKind,
    http: &dyn DeviceFlowHttp,
    now_unix: u64,
) -> Result<DeviceCredential, AgentAuthError> {
    let current = load_credential(data_home, tool)?;
    if !is_expiring(&current, now_unix, REFRESH_SKEW_SECS) {
        return Ok(current);
    }
    let Some(refresh_token) = current.refresh_token.as_deref() else {
        return Ok(current);
    };
    let mut refreshed = http.refresh(tool, refresh_token)?;
    if refreshed.refresh_token.is_none() {
        refreshed.refresh_token = current.refresh_token.clone();
    }
    persist_credential(data_home, tool, &refreshed)?;
    Ok(refreshed)
}

/// Mint a run credential: a fresh access token with `refresh_token: None`. The
/// host store keeps the refresh token; the returned value carries none.
pub fn mint_run_credential(
    data_home: &Path,
    tool: AgentToolKind,
    http: &dyn DeviceFlowHttp,
    now_unix: u64,
) -> Result<DeviceCredential, AgentAuthError> {
    let host = refresh_if_expiring(data_home, tool, http, now_unix)?;
    ensure_portable(&host, tool)?;
    let mut run_cred = match host.refresh_token.as_deref() {
        Some(refresh_token) => http.refresh(tool, refresh_token)?,
        None => host,
    };
    run_cred.refresh_token = None;
    Ok(run_cred)
}

/// Refresh-if-needed, mint a no-refresh credential, and write it into the run
/// home's CLI credential path at mode 0600. The receipt records paths, modes,
/// and digests only.
pub fn materialize_run_credential(
    data_home: &Path,
    tool: AgentToolKind,
    run_home: &Path,
    http: &dyn DeviceFlowHttp,
    now_unix: u64,
) -> Result<RunAuthReceipt, AgentAuthError> {
    let run_cred = mint_run_credential(data_home, tool, http, now_unix)?;
    create_private_dir(run_home)?;
    let target = run_credential_path(tool, run_home)?;
    let bytes = serialize_credential(&run_cred)?;
    let receipt = write_private_file(&target, &bytes)?;
    Ok(RunAuthReceipt {
        tool,
        run_home: run_home.to_path_buf(),
        files: vec![receipt],
    })
}

fn persist_credential(
    data_home: &Path,
    tool: AgentToolKind,
    cred: &DeviceCredential,
) -> Result<AuthImportReceipt, AgentAuthError> {
    let dir = auth_dir(data_home, tool);
    create_private_dir(&dir)?;
    let bytes = serialize_credential(cred)?;
    let receipt = write_private_file_atomic(&dir.join(CREDENTIAL_FILE), &bytes)?;
    Ok(AuthImportReceipt {
        tool,
        auth_dir: dir,
        files: vec![receipt],
    })
}

fn credential_store_path(data_home: &Path, tool: AgentToolKind) -> PathBuf {
    auth_dir(data_home, tool).join(CREDENTIAL_FILE)
}

fn run_credential_path(tool: AgentToolKind, run_home: &Path) -> Result<PathBuf, AgentAuthError> {
    let dir = match tool {
        AgentToolKind::Codex => run_home.join("codex"),
        AgentToolKind::Claude => run_home.join(".claude"),
        AgentToolKind::Jekko => return Err(device_unsupported(tool)),
    };
    Ok(dir.join(CREDENTIAL_FILE))
}

fn ensure_portable(cred: &DeviceCredential, tool: AgentToolKind) -> Result<(), AgentAuthError> {
    if cred.access_token.is_empty() && cred.refresh_token.is_none() {
        return Err(host_bound_credential(tool));
    }
    Ok(())
}

fn serialize_credential(cred: &DeviceCredential) -> Result<Vec<u8>, AgentAuthError> {
    let mut bytes = serde_json::to_vec_pretty(cred)
        .map_err(|error| credential_codec_error(error.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn deserialize_credential(bytes: &[u8]) -> Result<DeviceCredential, AgentAuthError> {
    serde_json::from_slice(bytes).map_err(|error| credential_codec_error(error.to_string()))
}

fn parse_device_start(body: &str) -> Result<DeviceFlowStart, AgentAuthError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|error| credential_codec_error(error.to_string()))?;
    Ok(DeviceFlowStart {
        verification_uri: required_str(&value, "verification_uri")?,
        user_code: required_str(&value, "user_code")?,
        device_code: required_str(&value, "device_code")?,
        interval_secs: value
            .get("interval")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(5),
        expires_in_secs: value
            .get("expires_in")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    })
}

fn parse_token(body: &str) -> Result<DeviceCredential, AgentAuthError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|error| credential_codec_error(error.to_string()))?;
    let expires_in = value
        .get("expires_in")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let scopes = value
        .get("scope")
        .and_then(serde_json::Value::as_str)
        .map(|scope| scope.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    Ok(DeviceCredential {
        access_token: required_str(&value, "access_token")?,
        refresh_token: value
            .get("refresh_token")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        expires_at_unix: unix_now().saturating_add(expires_in),
        token_type: value
            .get("token_type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Bearer")
            .to_string(),
        scopes,
    })
}

fn required_str(value: &serde_json::Value, field: &str) -> Result<String, AgentAuthError> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| credential_codec_error(format!("device response is missing '{field}'")))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

fn host_bound_credential(tool: AgentToolKind) -> AgentAuthError {
    AgentAuthError::new(
        "agent_auth_host_bound",
        "mint a portable run credential",
        format!(
            "the stored {tool} credential is host-bound with no portable access or refresh token"
        ),
        &[
            "rerun the device login so the provider returns an access or refresh token",
            "ensure the login did not produce a keychain-only reference",
        ],
        "docs/testing.md#workcells",
        "rerun cargo test -p jeryu-agent-auth --jobs 40",
    )
}

fn device_unsupported(tool: AgentToolKind) -> AgentAuthError {
    AgentAuthError::new(
        "agent_auth_device_unsupported",
        "run the device-authorization flow",
        format!("{tool} does not use the OAuth device-authorization flow"),
        &[
            "import portable auth with jeryu agent auth import --from-host",
            "use codex or claude for device login",
        ],
        "docs/testing.md#workcells",
        "rerun cargo test -p jeryu-agent-auth --jobs 40",
    )
}

fn credential_codec_error(detail: String) -> AgentAuthError {
    AgentAuthError::new(
        "agent_auth_credential_codec",
        "encode or decode a device credential",
        detail,
        &[
            "rerun the device login to regenerate credential.json",
            "remove the corrupt credential.json and re-import",
        ],
        "docs/testing.md#workcells",
        "rerun cargo test -p jeryu-agent-auth --jobs 40",
    )
}

#[cfg(test)]
mod tests {
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

        let receipt =
            import_from_device(&data, AgentToolKind::Claude, &cred).expect("import succeeds");

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

        let refreshed = refresh_if_expiring(&data, AgentToolKind::Claude, &http, 1_000)
            .expect("refresh succeeds");

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

        let result = refresh_if_expiring(&data, AgentToolKind::Claude, &http, 1_000)
            .expect("no refresh needed");

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

        let receipt =
            materialize_run_credential(&data, AgentToolKind::Claude, &run_home, &http, 1_000)
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

        let error =
            materialize_run_credential(&data, AgentToolKind::Claude, &run_home, &http, 1_000)
                .expect_err("host-bound credential denied");

        assert_eq!(error.code, "agent_auth_host_bound");
        assert!(!error.repair.common_fixes.is_empty());
        assert!(!error.repair.repair_hint.is_empty());
    }
}
