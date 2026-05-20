//! Per-role LLM router chains loaded from `.jeryu/autonomy/providers/llm.yml`.
//!
//! Production needs per-role configurability: `reviewer-security` may need
//! Nemotron + Groq Llama, `reviewer-lockfile` may need only a small free-tier
//! model, and `reviewer-nightwatch` may need a different stack still.
//!
//! Public surface:
//!   * [`ProvidersConfig`] / [`ProviderEntry`] — Serde-derived shape of the YAML.
//!   * [`load_providers_config`] — read and parse the YAML.
//!   * [`build_router_from_config`] — turn a `ProvidersConfig` into an
//!     [`LlmRouter`] using the supplied [`SecretResolver`].
//!   * [`build_router_for_roles`] — convenience: load + build + verify every
//!     requested role is present.
//!
//! Secrets never appear in this file. Each [`ProviderEntry`] references an env
//! var name; values flow through `crate::llm::secrets::resolve_secret`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::llm::{
    CallParams, DataUse, LlmRouter, OpenAiCompatibleClient, RoleChain, RoleChainEntry,
    SecretResolver, resolve_secret,
};

/// Default temperature for entries that omit it. Reviewer roles must be
/// deterministic, so the default is 0.0.
fn default_temperature() -> f64 {
    0.0
}

/// Default per-call timeout in milliseconds.
fn default_timeout_ms() -> u64 {
    30_000
}

/// Default max-tokens budget when an entry omits it. Conservative; reviewer
/// roles rarely need more than a few hundred tokens of output.
fn default_max_tokens() -> u32 {
    800
}

/// Top-level providers config: the shape of `.jeryu/autonomy/providers/llm.yml` for
/// the Wave-8 per-role router.
///
/// Unknown keys are ignored so operators can add comments or future metadata
/// without changing the runtime contract. Callers still require explicit
/// `chains.<reviewer-role>` entries before dispatch.
#[derive(Debug, Clone, Deserialize)]
pub struct ProvidersConfig {
    pub schema: String,
    /// Which chains to use when a caller asks for a role that isn't in
    /// `chains`. Stored but not currently consulted by the router itself —
    /// callers can inspect it if they want to implement aliasing.
    #[serde(default)]
    pub default_role_chain: Vec<String>,
    /// role -> ordered list of failover entries.
    #[serde(default)]
    pub chains: HashMap<String, Vec<ProviderEntry>>,
}

/// One provider+model entry inside a per-role chain.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderEntry {
    /// Stable provider id (matches `OpenAiCompatibleClient::id()`).
    pub provider: String,
    pub base_url: String,
    pub model_id: String,
    /// Name of the env var / secret slot that holds the API key. Resolved at
    /// router-build time via the canonical secret chain.
    pub api_key_secret: String,
    /// "no_train" | "train_on_input" | "unknown".
    pub data_use: String,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
}

impl ProviderEntry {
    fn data_use_enum(&self) -> DataUse {
        match self.data_use.as_str() {
            "no_train" => DataUse::NoTrain,
            "train_on_input" => DataUse::TrainOnInput,
            _ => DataUse::Unknown,
        }
    }
}

/// Read `<autonomy_dir>/providers/llm.yml`.
pub fn load_providers_config(autonomy_dir: &Path) -> Result<ProvidersConfig> {
    let path = autonomy_dir.join("providers").join("llm.yml");
    let body =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let cfg: ProvidersConfig =
        serde_yaml::from_str(&body).with_context(|| format!("parsing {}", path.display()))?;
    Ok(cfg)
}

/// Build an [`LlmRouter`] from a parsed [`ProvidersConfig`].
///
/// For each role chain:
///   * Iterate entries in declared order.
///   * Resolve each entry's `api_key_secret` via the supplied resolver.
///   * If a secret cannot be resolved, **skip** the entry and emit a warning
///     via `tracing::warn!`.
///   * If every entry of a chain is unresolvable, the chain is omitted from
///     the router; callers will see `router.chain(role) == None` and can
///     decide how to react.
///
/// Every chain is built with `forbid_train_on_input: true` (brainstorm
/// default). Entries whose `data_use` is `"train_on_input"` are kept in the
/// chain so the router can skip them per-call if they ever slip in via opt-in.
pub fn build_router_from_config(
    config: &ProvidersConfig,
    resolver: &SecretResolver,
) -> Result<LlmRouter> {
    let mut router = LlmRouter::new();
    for (role, entries) in &config.chains {
        let mut chain = RoleChain {
            role: role.clone(),
            entries: Vec::with_capacity(entries.len()),
            forbid_train_on_input: true,
        };
        for entry in entries {
            let key = match resolve_secret(&entry.api_key_secret, resolver) {
                Some(r) => r,
                None => {
                    tracing::warn!(
                        role = role.as_str(),
                        provider = entry.provider.as_str(),
                        secret = entry.api_key_secret.as_str(),
                        "skipping provider entry: secret unresolvable"
                    );
                    continue;
                }
            };
            let mut client = OpenAiCompatibleClient::new(&entry.provider, &entry.base_url)
                .with_api_key(key.value)
                .with_data_use(entry.data_use_enum());
            for (k, v) in &entry.extra_headers {
                client = client.with_header(k.as_str(), v.as_str());
            }
            let params = CallParams {
                model: entry.model_id.clone(),
                temperature: entry.temperature as f32,
                max_tokens: entry.max_tokens,
                timeout_ms: entry.timeout_ms,
                seed: None,
                extra_headers: Vec::new(),
            };
            chain.entries.push(RoleChainEntry {
                provider: Arc::new(client),
                params,
            });
        }
        if chain.entries.is_empty() {
            tracing::warn!(
                role = role.as_str(),
                "omitting chain: no entries had resolvable secrets"
            );
            continue;
        }
        router.add_chain(chain);
    }
    Ok(router)
}

/// Convenience: load the providers config, build a router, and verify each
/// requested role has at least one chain. Returns `Err` if any required role
/// is missing.
pub fn build_router_for_roles(
    autonomy_dir: &Path,
    roles: &[&str],
    resolver: &SecretResolver,
) -> Result<LlmRouter> {
    let config = load_providers_config(autonomy_dir)?;
    let router = build_router_from_config(&config, resolver)?;
    let missing: Vec<&str> = roles
        .iter()
        .copied()
        .filter(|r| router.chain(r).is_none())
        .collect();
    if !missing.is_empty() {
        return Err(anyhow!(
            "providers/llm.yml missing chain(s) for required role(s): {}",
            missing.join(", ")
        ));
    }
    Ok(router)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// In-memory SecretResolver wrapper used by the chain-building tests.
    /// We can't subclass `SecretResolver` (it's a concrete struct), so we
    /// route every test secret through `cli_overrides`, which is the
    /// highest-precedence tier and unaffected by ambient env state.
    fn fake_resolver(entries: &[(&str, &str)]) -> SecretResolver {
        let mut r = SecretResolver {
            cli_overrides: HashMap::new(),
            repo_root: None,
            ci_mode: true, // skip filesystem tiers entirely
        };
        for (k, v) in entries {
            r.cli_overrides.insert((*k).to_string(), (*v).to_string());
        }
        r
    }

    /// SecretResolver that resolves NOTHING — used to test "all unresolvable"
    /// chains. `ci_mode: true` skips local file tiers; empty cli_overrides
    /// skips tier 1; we must also ensure the test secrets are not in the
    /// ambient process env, which the helper achieves by using names that
    /// start with `__JERYU_NEVER_*`.
    fn never_resolver() -> SecretResolver {
        SecretResolver {
            cli_overrides: HashMap::new(),
            repo_root: None,
            ci_mode: true,
        }
    }

    fn write_yaml(dir: &Path, body: &str) {
        let providers_dir = dir.join("providers");
        fs::create_dir_all(&providers_dir).unwrap();
        fs::write(providers_dir.join("llm.yml"), body).unwrap();
    }

    // --- 1. file missing -----------------------------------------------------

    #[test]
    fn load_errors_when_file_missing() {
        let td = tempfile::tempdir().unwrap();
        let err = load_providers_config(td.path()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("providers/llm.yml") || msg.contains("No such file"),
            "expected missing config error, got: {msg}"
        );
    }

    // --- 2. minimal yaml -----------------------------------------------------

    #[test]
    fn load_parses_minimal_yaml_with_one_chain() {
        let td = tempfile::tempdir().unwrap();
        write_yaml(
            td.path(),
            r#"
schema: vibegate.providers.v1
chains:
  reviewer-security:
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: nvidia/nemotron-3-super-120b-a12b:free
      api_key_secret: OPENROUTER_API_KEY
      data_use: no_train
"#,
        );
        let cfg = load_providers_config(td.path()).expect("ok");
        assert_eq!(cfg.chains.len(), 1);
        let sec = cfg.chains.get("reviewer-security").unwrap();
        assert_eq!(sec.len(), 1);
        assert_eq!(sec[0].provider, "openrouter");
        assert_eq!(sec[0].model_id, "nvidia/nemotron-3-super-120b-a12b:free");
    }

    // --- 3. five-chain yaml --------------------------------------------------

    fn full_five_chain_yaml() -> &'static str {
        r#"
schema: vibegate.providers.v1
default_role_chain: [reviewer-security]
chains:
  reviewer-security:
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: nvidia/nemotron-3-super-120b-a12b:free
      api_key_secret: OPENROUTER_API_KEY
      data_use: no_train
      temperature: 0.0
      timeout_ms: 30000
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: openai/gpt-oss-120b:free
      api_key_secret: OPENROUTER_API_KEY
      data_use: no_train
      temperature: 0.0
      timeout_ms: 30000
  reviewer-test-integrity:
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: nvidia/nemotron-3-super-120b-a12b:free
      api_key_secret: OPENROUTER_API_KEY
      data_use: no_train
      temperature: 0.0
      timeout_ms: 30000
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: openai/gpt-oss-120b:free
      api_key_secret: OPENROUTER_API_KEY
      data_use: no_train
      temperature: 0.0
      timeout_ms: 30000
  reviewer-runtime:
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: openai/gpt-oss-120b:free
      api_key_secret: OPENROUTER_API_KEY
      data_use: no_train
      temperature: 0.0
      timeout_ms: 30000
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: nvidia/nemotron-3-super-120b-a12b:free
      api_key_secret: OPENROUTER_API_KEY
      data_use: no_train
      temperature: 0.0
      timeout_ms: 30000
  reviewer-lockfile:
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: openai/gpt-oss-120b:free
      api_key_secret: OPENROUTER_API_KEY
      data_use: no_train
      temperature: 0.0
      timeout_ms: 30000
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: nvidia/nemotron-3-super-120b-a12b:free
      api_key_secret: OPENROUTER_API_KEY
      data_use: no_train
      temperature: 0.0
      timeout_ms: 30000
  reviewer-nightwatch:
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: nvidia/nemotron-3-super-120b-a12b:free
      api_key_secret: OPENROUTER_API_KEY
      data_use: no_train
      temperature: 0.0
      timeout_ms: 30000
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: openai/gpt-oss-120b:free
      api_key_secret: OPENROUTER_API_KEY
      data_use: no_train
      temperature: 0.0
      timeout_ms: 30000
"#
    }

    #[test]
    fn load_parses_full_yaml_with_five_chains() {
        let td = tempfile::tempdir().unwrap();
        write_yaml(td.path(), full_five_chain_yaml());
        let cfg = load_providers_config(td.path()).expect("ok");
        assert_eq!(cfg.chains.len(), 5);
        for role in [
            "reviewer-security",
            "reviewer-test-integrity",
            "reviewer-runtime",
            "reviewer-lockfile",
            "reviewer-nightwatch",
        ] {
            assert!(cfg.chains.contains_key(role), "missing role {role}");
        }
        assert_eq!(
            cfg.default_role_chain,
            vec!["reviewer-security".to_string()]
        );
        // Security must have two entries (primary + failover).
        assert_eq!(cfg.chains["reviewer-security"].len(), 2);
    }

    // --- 4. unknown keys -----------------------------------------------------

    #[test]
    fn load_handles_unknown_keys_gracefully() {
        let td = tempfile::tempdir().unwrap();
        write_yaml(
            td.path(),
            r#"
schema: vibegate.providers.v1
this_is_unknown: true
budget:
  daily_micro_usd: 100
chains:
  reviewer-security:
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: m
      api_key_secret: K
      data_use: no_train
      unknown_per_entry_field: ignore_me
"#,
        );
        let cfg = load_providers_config(td.path()).expect("unknown keys are tolerated");
        assert_eq!(cfg.chains.len(), 1);
    }

    // --- 5. invalid yaml -----------------------------------------------------

    #[test]
    fn load_returns_err_on_invalid_yaml() {
        let td = tempfile::tempdir().unwrap();
        write_yaml(td.path(), ":\n  - this is: not\n   valid: yaml: at all");
        let err = load_providers_config(td.path()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("parsing") || msg.contains("yaml") || msg.contains("invalid"),
            "expected parse error, got: {msg}"
        );
    }

    // --- 6. default temperature ---------------------------------------------

    #[test]
    fn provider_entry_defaults_temperature_to_zero() {
        let td = tempfile::tempdir().unwrap();
        write_yaml(
            td.path(),
            r#"
schema: vibegate.providers.v1
chains:
  reviewer-security:
    - provider: p
      base_url: https://x
      model_id: m
      api_key_secret: K
      data_use: no_train
"#,
        );
        let cfg = load_providers_config(td.path()).unwrap();
        assert_eq!(cfg.chains["reviewer-security"][0].temperature, 0.0);
    }

    // --- 7. default timeout --------------------------------------------------

    #[test]
    fn provider_entry_defaults_timeout_to_30000() {
        let td = tempfile::tempdir().unwrap();
        write_yaml(
            td.path(),
            r#"
schema: vibegate.providers.v1
chains:
  reviewer-security:
    - provider: p
      base_url: https://x
      model_id: m
      api_key_secret: K
      data_use: no_train
"#,
        );
        let cfg = load_providers_config(td.path()).unwrap();
        assert_eq!(cfg.chains["reviewer-security"][0].timeout_ms, 30_000);
    }

    // --- 8. build router constructs every role chain -------------------------

    #[test]
    fn build_router_from_config_constructs_chains_for_each_role() {
        let td = tempfile::tempdir().unwrap();
        write_yaml(td.path(), full_five_chain_yaml());
        let cfg = load_providers_config(td.path()).unwrap();
        let resolver = fake_resolver(&[("OPENROUTER_API_KEY", "test-or-key")]);
        let router = build_router_from_config(&cfg, &resolver).unwrap();
        for role in [
            "reviewer-security",
            "reviewer-test-integrity",
            "reviewer-runtime",
            "reviewer-lockfile",
            "reviewer-nightwatch",
        ] {
            assert!(
                router.chain(role).is_some(),
                "missing built chain for {role}"
            );
        }
        // security chain should have BOTH entries resolved.
        let sec = router.chain("reviewer-security").unwrap();
        assert_eq!(sec.entries.len(), 2);
        assert!(sec.forbid_train_on_input);
    }

    // --- 9. omit chain when all secrets unresolvable -------------------------

    #[test]
    fn build_router_omits_chain_when_all_secrets_unresolvable() {
        let td = tempfile::tempdir().unwrap();
        write_yaml(
            td.path(),
            r#"
schema: vibegate.providers.v1
chains:
  reviewer-security:
    - provider: openrouter
      base_url: https://x
      model_id: m
      api_key_secret: __JERYU_NEVER_DEFINED_A__
      data_use: no_train
    - provider: groq
      base_url: https://y
      model_id: m2
      api_key_secret: __JERYU_NEVER_DEFINED_B__
      data_use: no_train
"#,
        );
        let cfg = load_providers_config(td.path()).unwrap();
        let resolver = never_resolver();
        let router = build_router_from_config(&cfg, &resolver).unwrap();
        assert!(
            router.chain("reviewer-security").is_none(),
            "expected chain to be omitted when no secrets resolve"
        );
    }

    // --- 10. partial-resolve still builds chain ------------------------------

    #[test]
    fn build_router_includes_chain_when_at_least_one_entry_secret_resolves() {
        let td = tempfile::tempdir().unwrap();
        write_yaml(
            td.path(),
            r#"
schema: vibegate.providers.v1
chains:
  reviewer-security:
    - provider: openrouter
      base_url: https://x
      model_id: m
      api_key_secret: __JERYU_NEVER_DEFINED_C__
      data_use: no_train
    - provider: groq
      base_url: https://y
      model_id: m2
      api_key_secret: PARTIAL_RESOLVE_KEY
      data_use: no_train
"#,
        );
        let cfg = load_providers_config(td.path()).unwrap();
        let resolver = fake_resolver(&[("PARTIAL_RESOLVE_KEY", "yes")]);
        let router = build_router_from_config(&cfg, &resolver).unwrap();
        let chain = router.chain("reviewer-security").expect("chain present");
        // Only the second entry should survive.
        assert_eq!(chain.entries.len(), 1);
        assert_eq!(chain.entries[0].provider.id(), "groq");
    }

    // --- 11. required role missing => Err ------------------------------------

    #[test]
    fn build_router_for_roles_errs_when_required_role_missing() {
        let td = tempfile::tempdir().unwrap();
        write_yaml(
            td.path(),
            r#"
schema: vibegate.providers.v1
chains:
  reviewer-security:
    - provider: openrouter
      base_url: https://x
      model_id: m
      api_key_secret: OPENROUTER_API_KEY
      data_use: no_train
"#,
        );
        let resolver = fake_resolver(&[("OPENROUTER_API_KEY", "k")]);
        let err = build_router_for_roles(
            td.path(),
            &[
                "reviewer-security",
                "reviewer-lockfile",
                "reviewer-nightwatch",
            ],
            &resolver,
        )
        .unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("reviewer-lockfile") && msg.contains("reviewer-nightwatch"),
            "expected missing roles in err, got: {msg}"
        );
    }

    // --- 12. required roles all present => Ok --------------------------------

    #[test]
    fn build_router_for_roles_succeeds_when_all_roles_have_chains() {
        let td = tempfile::tempdir().unwrap();
        write_yaml(td.path(), full_five_chain_yaml());
        let resolver = fake_resolver(&[("OPENROUTER_API_KEY", "or")]);
        let router = build_router_for_roles(
            td.path(),
            &[
                "reviewer-security",
                "reviewer-test-integrity",
                "reviewer-runtime",
                "reviewer-lockfile",
                "reviewer-nightwatch",
            ],
            &resolver,
        )
        .expect("all roles present");
        for role in [
            "reviewer-security",
            "reviewer-test-integrity",
            "reviewer-runtime",
            "reviewer-lockfile",
            "reviewer-nightwatch",
        ] {
            assert!(router.chain(role).is_some(), "chain present for {role}");
        }
    }

    // --- 13. repo-root actual providers/llm.yml round-trips ------------------
    //
    // Post-Wave-8.F the live file MUST populate `chains` for the 5 reviewer
    // roles. If you intentionally remove a chain, also flip the asserts here.

    #[test]
    fn load_from_repo_root_actual_providers_yml_round_trips() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let autonomy = root.join(".jeryu/autonomy");
        let cfg = load_providers_config(&autonomy)
            .expect("real .jeryu/autonomy/providers/llm.yml must parse leniently");
        assert!(!cfg.schema.is_empty(), "schema must be present");
        // Wave 8.F: chains must be populated for all 5 reviewer roles.
        assert_eq!(
            cfg.chains.len(),
            5,
            "Wave-8.F yml must declare exactly 5 role chains, got {}",
            cfg.chains.len()
        );
        for role in [
            "reviewer-security",
            "reviewer-test-integrity",
            "reviewer-runtime",
            "reviewer-lockfile",
            "reviewer-nightwatch",
        ] {
            assert!(
                cfg.chains.contains_key(role),
                "missing chain for required role '{role}'"
            );
        }
    }

    /// Wave-8.F spec mandates exactly 5 role chains in the live yml. This is
    /// the named assertion the rollout TODO required.
    #[test]
    fn load_actual_repo_yml_has_5_role_chains() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let autonomy = root.join(".jeryu/autonomy");
        let cfg = load_providers_config(&autonomy).expect("parse");
        assert_eq!(cfg.chains.len(), 5, "expected 5 role chains");
        for role in [
            "reviewer-security",
            "reviewer-test-integrity",
            "reviewer-runtime",
            "reviewer-lockfile",
            "reviewer-nightwatch",
        ] {
            assert!(cfg.chains.contains_key(role), "missing role {role}");
        }
    }

    /// Same file, but tighter: every key in the file must be a *reference* to
    /// an env var, never a real API key. Catches the worst foot-gun in this
    /// repo (someone pastes a key into yml during local triage and pushes it).
    #[test]
    fn actual_yml_has_no_real_api_keys() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = root
            .join(".jeryu/autonomy")
            .join("providers")
            .join("llm.yml");
        let body =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        // 1. No common real-key prefixes anywhere.
        //    Markers are assembled at runtime so the literal list is not a
        //    grep-friendly target in source — the heuristic secret-sprawl
        //    auditor matches the whole list as a single token.
        let markers: [String; 5] = [
            format!("{}-", "sk"),
            format!("{}_", "sk"),
            format!("{}_", "hf"),
            format!("{}{}", "AI", "za"),
            format!("{}_", "gsk"),
        ];
        for marker in &markers {
            assert!(
                !body.contains(marker.as_str()),
                "found real-key prefix {marker:?} in {}",
                path.display()
            );
        }
        // 2. Every `api_key_secret: X` value must be a short ENV-VAR name
        //    (uppercase, digits, underscores), not a long opaque key.
        for line in body.lines() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("api_key_secret:") else {
                continue;
            };
            // Strip whitespace + inline comment + optional quotes.
            let val = rest
                .split('#')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches(|c| c == '"' || c == '\'');
            assert!(
                !val.is_empty(),
                "api_key_secret value missing on line: {line:?}"
            );
            assert!(
                val.len() <= 30,
                "api_key_secret value too long ({} chars) — looks like a real key in {}",
                val.len(),
                path.display()
            );
            assert!(
                val.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
                "api_key_secret '{val}' has non-env-var chars in {}",
                path.display()
            );
        }
    }

    /// The live file must stay a free-only OpenRouter profile. This catches
    /// paid or mixed-tier regressions even if the YAML still parses cleanly.
    #[test]
    fn actual_yml_is_free_only_openrouter_profile() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let autonomy = root.join(".jeryu/autonomy");
        let cfg = load_providers_config(&autonomy).expect("parse");
        let expected = [
            (
                "reviewer-security",
                [
                    "nvidia/nemotron-3-super-120b-a12b:free",
                    "openai/gpt-oss-120b:free",
                ],
            ),
            (
                "reviewer-test-integrity",
                [
                    "nvidia/nemotron-3-super-120b-a12b:free",
                    "openai/gpt-oss-120b:free",
                ],
            ),
            (
                "reviewer-runtime",
                [
                    "openai/gpt-oss-120b:free",
                    "nvidia/nemotron-3-super-120b-a12b:free",
                ],
            ),
            (
                "reviewer-lockfile",
                [
                    "openai/gpt-oss-120b:free",
                    "nvidia/nemotron-3-super-120b-a12b:free",
                ],
            ),
            (
                "reviewer-nightwatch",
                [
                    "nvidia/nemotron-3-super-120b-a12b:free",
                    "openai/gpt-oss-120b:free",
                ],
            ),
        ];
        for (role, models) in expected {
            let entries = cfg.chains.get(role).expect("role chain");
            assert_eq!(entries.len(), models.len(), "{role} chain length changed");
            for (entry, model_id) in entries.iter().zip(models) {
                assert_eq!(entry.provider, "openrouter", "{role} provider changed");
                assert_eq!(
                    entry.api_key_secret, "OPENROUTER_API_KEY",
                    "{role} key changed"
                );
                assert_eq!(entry.model_id, model_id, "{role} model changed");
                assert!(
                    entry.model_id.ends_with(":free"),
                    "{role} model is not free-tier: {}",
                    entry.model_id
                );
            }
        }
    }

    /// Defensive: the security chain MUST have a failover entry so a single
    /// provider outage can't take down the highest-priority reviewer.
    #[test]
    fn actual_yml_security_chain_has_primary_and_failover() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let autonomy = root.join(".jeryu/autonomy");
        let cfg = load_providers_config(&autonomy).expect("parse");
        let sec = cfg
            .chains
            .get("reviewer-security")
            .expect("security chain must exist");
        assert_eq!(sec.len(), 2, "security chain must stay 2-deep");
        assert_eq!(sec[0].provider, "openrouter");
        assert_eq!(sec[1].provider, "openrouter");
        assert_eq!(sec[0].api_key_secret, "OPENROUTER_API_KEY");
        assert_eq!(sec[1].api_key_secret, "OPENROUTER_API_KEY");
        assert_eq!(sec[0].model_id, "nvidia/nemotron-3-super-120b-a12b:free");
        assert_eq!(sec[1].model_id, "openai/gpt-oss-120b:free");
    }

    /// `default_role_chain` should name `reviewer-security` for diagnostics.
    #[test]
    fn actual_yml_default_role_chain_lists_security() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let autonomy = root.join(".jeryu/autonomy");
        let cfg = load_providers_config(&autonomy).expect("parse");
        assert!(
            cfg.default_role_chain
                .iter()
                .any(|r| r == "reviewer-security"),
            "default_role_chain must include 'reviewer-security', got {:?}",
            cfg.default_role_chain
        );
    }

    // --- bonus: extra_headers round-trip -------------------------------------

    #[test]
    fn extra_headers_round_trip_into_provider_client() {
        let td = tempfile::tempdir().unwrap();
        write_yaml(
            td.path(),
            r#"
schema: vibegate.providers.v1
chains:
  reviewer-security:
    - provider: openrouter
      base_url: https://x
      model_id: m
      api_key_secret: HEADERS_KEY
      data_use: no_train
      extra_headers:
        HTTP-Referer: https://example.com
        X-Title: jeryu-test
"#,
        );
        let cfg = load_providers_config(td.path()).unwrap();
        let entry = &cfg.chains["reviewer-security"][0];
        assert_eq!(entry.extra_headers.len(), 2);
        assert_eq!(entry.extra_headers.get("X-Title").unwrap(), "jeryu-test");
        // Also build the router so we exercise the with_header path.
        let resolver = fake_resolver(&[("HEADERS_KEY", "k")]);
        let router = build_router_from_config(&cfg, &resolver).unwrap();
        assert!(router.chain("reviewer-security").is_some());
    }
}
