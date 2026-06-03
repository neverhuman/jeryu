//! Opt-in egress contract for live agent execution.
//!
//! Deterministic edit-bot tests stay network-denied in `jeryu-agentbridge`.
//! Live agent execution must pass through this contract first: the network
//! policy is egress-only, destinations are allowlisted, secret handling is
//! explicit, and the budget gate can stop a call before launch.

use std::fmt::{Display, Formatter};

use jeryu_runner_core::{NetworkPolicy, SecretPolicy, TokenPolicy};

/// Egress validation result.
pub type EgressResult<T> = Result<T, EgressError>;

/// Typed, repairable egress contract error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EgressError {
    /// Operation purpose.
    pub purpose: &'static str,
    /// Machine-readable reason.
    pub reason: String,
    /// Common local fixes.
    pub common_fixes: Vec<&'static str>,
    /// Owning docs URL.
    pub docs_url: &'static str,
    /// Agent-readable rerun or repair hint.
    pub repair_hint: &'static str,
}

impl EgressError {
    fn new(
        purpose: &'static str,
        reason: impl Into<String>,
        common_fixes: Vec<&'static str>,
        repair_hint: &'static str,
    ) -> Self {
        Self {
            purpose,
            reason: reason.into(),
            common_fixes,
            docs_url: "docs/testing.md#agent-egress",
            repair_hint,
        }
    }
}

impl Display for EgressError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.purpose, self.reason)
    }
}

impl std::error::Error for EgressError {}

/// Why a live agent is allowed to reach a host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EgressPurpose {
    /// Live model provider endpoint.
    LlmProvider,
    /// Package registry needed by a bounded repair.
    PackageRegistry,
    /// Forge Git endpoint for the target repository.
    ForgeGit,
}

impl EgressPurpose {
    /// Stable purpose label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LlmProvider => "llm-provider",
            Self::PackageRegistry => "package-registry",
            Self::ForgeGit => "forge-git",
        }
    }
}

/// One egress allowlist rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EgressRule {
    /// Purpose for this destination.
    pub purpose: EgressPurpose,
    /// Hostname only. Schemes, paths, ports, and wildcards are denied.
    pub host: String,
}

impl EgressRule {
    /// Construct an allowlist rule. Validation happens in
    /// [`EgressAllowlist::try_new`].
    pub fn new(purpose: EgressPurpose, host: impl Into<String>) -> Self {
        Self {
            purpose,
            host: host.into(),
        }
    }
}

/// Explicit egress allowlist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EgressAllowlist {
    rules: Vec<EgressRule>,
}

impl EgressAllowlist {
    /// Build and validate an allowlist.
    pub fn try_new(rules: Vec<EgressRule>) -> EgressResult<Self> {
        let allowlist = Self { rules };
        allowlist.validate()?;
        Ok(allowlist)
    }

    /// Rules in declaration order.
    pub fn rules(&self) -> &[EgressRule] {
        &self.rules
    }

    fn validate(&self) -> EgressResult<()> {
        if self.rules.is_empty() {
            return Err(EgressError::new(
                "validate live agent egress allowlist",
                "allowlist must name at least one destination",
                vec![
                    "add a host for each live egress purpose",
                    "keep deterministic edit-bot tests on network deny",
                ],
                "rerun cargo test -p jeryu-egress --jobs 40",
            ));
        }
        for rule in &self.rules {
            validate_host(&rule.host)?;
        }
        Ok(())
    }
}

/// Explicit secret handling for a live agent launch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecretHandling {
    /// Explicitly launch without secrets.
    None,
    /// Inject only the named environment variables. Values are resolved by the
    /// caller's secret store and must never be stored in this contract.
    ExplicitEnv { names: Vec<String> },
}

impl SecretHandling {
    /// Runner secret policy implied by the explicit handling mode.
    pub fn secret_policy(&self) -> SecretPolicy {
        match self {
            Self::None => SecretPolicy::None,
            Self::ExplicitEnv { .. } => SecretPolicy::Requested,
        }
    }

    fn validate(&self) -> EgressResult<()> {
        match self {
            Self::None => Ok(()),
            Self::ExplicitEnv { names } => {
                if names.is_empty() {
                    return Err(EgressError::new(
                        "validate live agent secret handling",
                        "explicit secret handling must name at least one env var or choose none",
                        vec![
                            "name only the secret environment variables",
                            "use SecretHandling::None for secretless local models",
                        ],
                        "rerun cargo test -p jeryu-egress --jobs 40",
                    ));
                }
                for name in names {
                    validate_env_name(name)?;
                }
                Ok(())
            }
        }
    }
}

/// Budget gate for one live-agent request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetGate {
    /// Operator or automation identity that opted into the budget.
    pub operator: String,
    /// Total request budget units.
    pub request_budget_units: u64,
    /// Units already consumed.
    pub consumed_units: u64,
    /// Stop threshold as a percentage of the request budget.
    pub stop_at_percent: u8,
}

impl BudgetGate {
    /// Create a budget gate with the project default 80 percent stop threshold.
    pub fn new(
        operator: impl Into<String>,
        request_budget_units: u64,
        consumed_units: u64,
    ) -> Self {
        Self {
            operator: operator.into(),
            request_budget_units,
            consumed_units,
            stop_at_percent: 80,
        }
    }

    /// Override the stop threshold.
    pub fn with_stop_at_percent(mut self, stop_at_percent: u8) -> Self {
        self.stop_at_percent = stop_at_percent;
        self
    }

    /// Remaining units before the hard request budget.
    pub fn remaining_units(&self) -> u64 {
        self.request_budget_units
            .saturating_sub(self.consumed_units)
    }

    /// Receipt shape required before a live call launches.
    pub fn receipt(&self) -> BudgetReceipt {
        BudgetReceipt {
            operator: self.operator.clone(),
            request_budget_units: self.request_budget_units,
            consumed_units: self.consumed_units,
            remaining_units: self.remaining_units(),
            stop_at_percent: self.stop_at_percent,
        }
    }

    fn validate(&self) -> EgressResult<()> {
        if self.operator.trim().is_empty() {
            return Err(EgressError::new(
                "validate live agent budget gate",
                "budget gate must name the operator that opted in",
                vec!["record the operator identity before launching a live agent"],
                "rerun cargo test -p jeryu-egress --jobs 40",
            ));
        }
        if self.request_budget_units == 0 {
            return Err(EgressError::new(
                "validate live agent budget gate",
                "request budget must be greater than zero",
                vec!["set an explicit nonzero request budget"],
                "rerun cargo test -p jeryu-egress --jobs 40",
            ));
        }
        if self.consumed_units > self.request_budget_units {
            return Err(EgressError::new(
                "validate live agent budget gate",
                "consumed units exceed the request budget",
                vec!["refresh the budget receipt before launching"],
                "rerun cargo test -p jeryu-egress --jobs 40",
            ));
        }
        if !(1..=100).contains(&self.stop_at_percent) {
            return Err(EgressError::new(
                "validate live agent budget gate",
                "stop threshold must be between 1 and 100 percent",
                vec!["use the default 80 percent stop threshold"],
                "rerun cargo test -p jeryu-egress --jobs 40",
            ));
        }
        Ok(())
    }

    fn ensure_can_spend(&self, estimated_units: u64) -> EgressResult<()> {
        self.validate()?;
        if estimated_units == 0 {
            return Err(EgressError::new(
                "check live agent budget estimate",
                "estimated units must be greater than zero",
                vec!["provide a conservative nonzero estimate before launch"],
                "rerun cargo test -p jeryu-egress --jobs 40",
            ));
        }
        let projected = self
            .consumed_units
            .checked_add(estimated_units)
            .ok_or_else(|| {
                EgressError::new(
                    "check live agent budget estimate",
                    "projected budget usage overflowed",
                    vec!["lower the request estimate and refresh the budget receipt"],
                    "rerun cargo test -p jeryu-egress --jobs 40",
                )
            })?;
        if projected > self.request_budget_units {
            return Err(EgressError::new(
                "check live agent budget estimate",
                "projected usage exceeds the request budget",
                vec!["lower the estimate or opt into a larger explicit budget"],
                "rerun cargo test -p jeryu-egress --jobs 40",
            ));
        }
        if u128::from(projected) * 100
            >= u128::from(self.request_budget_units) * u128::from(self.stop_at_percent)
        {
            return Err(EgressError::new(
                "check live agent budget estimate",
                "projected usage reaches the live-agent stop threshold",
                vec![
                    "stop before launching the live agent",
                    "attach a fresh budget receipt for any later opt-in",
                ],
                "rerun cargo test -p jeryu-egress --jobs 40",
            ));
        }
        Ok(())
    }
}

/// Budget receipt metadata exposed to callers before live execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BudgetReceipt {
    /// Operator or automation identity that opted in.
    pub operator: String,
    /// Total request budget units.
    pub request_budget_units: u64,
    /// Units already consumed.
    pub consumed_units: u64,
    /// Remaining units before the hard request budget.
    pub remaining_units: u64,
    /// Stop threshold as a percentage of the request budget.
    pub stop_at_percent: u8,
}

/// Runtime contract for an opt-in live agent path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveAgentRuntimeContract {
    /// Always `egress-only` for live agent execution.
    pub network_policy: NetworkPolicy,
    /// Never defaults; derived from explicit secret handling.
    pub secret_policy: SecretPolicy,
    /// Live model calls do not need forge write tokens.
    pub token_policy: TokenPolicy,
    /// Destination allowlist.
    pub allowlist: EgressAllowlist,
    /// Explicit secret handling.
    pub secrets: SecretHandling,
    /// Budget gate.
    pub budget: BudgetGate,
}

impl LiveAgentRuntimeContract {
    /// Build the live runtime contract.
    pub fn new(
        allowlist: EgressAllowlist,
        secrets: SecretHandling,
        budget: BudgetGate,
    ) -> EgressResult<Self> {
        allowlist.validate()?;
        secrets.validate()?;
        budget.validate()?;
        Ok(Self {
            network_policy: NetworkPolicy::EgressOnly,
            secret_policy: secrets.secret_policy(),
            token_policy: TokenPolicy::ReadOnly,
            allowlist,
            secrets,
            budget,
        })
    }

    /// Check a conservative launch estimate and return the current budget
    /// receipt when the live call remains under the stop threshold.
    pub fn validate_estimated_call(&self, estimated_units: u64) -> EgressResult<BudgetReceipt> {
        self.allowlist.validate()?;
        self.secrets.validate()?;
        self.budget.ensure_can_spend(estimated_units)?;
        Ok(self.budget.receipt())
    }
}

fn validate_host(host: &str) -> EgressResult<()> {
    let host = host.trim();
    let valid = !host.is_empty()
        && !host.contains('*')
        && !host.contains('/')
        && !host.contains('\\')
        && !host.contains(':')
        && host
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(valid_host_byte));
    if valid {
        return Ok(());
    }
    Err(EgressError::new(
        "validate live agent egress host",
        format!("invalid egress host '{host}'"),
        vec![
            "use hostnames only, without scheme, path, port, or wildcard",
            "keep each live destination purpose-specific",
        ],
        "rerun cargo test -p jeryu-egress --jobs 40",
    ))
}

fn valid_host_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-'
}

fn validate_env_name(name: &str) -> EgressResult<()> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return invalid_env_name(name);
    };
    if !(first == '_' || first.is_ascii_uppercase()) {
        return invalid_env_name(name);
    }
    if chars.all(|c| c == '_' || c.is_ascii_uppercase() || c.is_ascii_digit()) {
        return Ok(());
    }
    invalid_env_name(name)
}

fn invalid_env_name(name: &str) -> EgressResult<()> {
    Err(EgressError::new(
        "validate live agent secret env name",
        format!("invalid secret env name '{name}'"),
        vec![
            "store only env var names, never secret values",
            "use uppercase ASCII env var names such as LLM_API_KEY",
        ],
        "rerun cargo test -p jeryu-egress --jobs 40",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowlist() -> EgressAllowlist {
        EgressAllowlist::try_new(vec![
            EgressRule::new(EgressPurpose::LlmProvider, "llm.internal"),
            EgressRule::new(EgressPurpose::PackageRegistry, "crates.local"),
            EgressRule::new(EgressPurpose::ForgeGit, "forge.local"),
        ])
        .expect("allowlist")
    }

    #[test]
    fn live_contract_is_egress_only_with_explicit_secrets() {
        let contract = LiveAgentRuntimeContract::new(
            allowlist(),
            SecretHandling::ExplicitEnv {
                names: vec!["LLM_API_KEY".to_string()],
            },
            BudgetGate::new("operator", 10_000, 1_000),
        )
        .expect("contract");

        assert_eq!(contract.network_policy, NetworkPolicy::EgressOnly);
        assert_eq!(contract.secret_policy, SecretPolicy::Requested);
        assert_eq!(contract.token_policy, TokenPolicy::ReadOnly);
    }

    #[test]
    fn allowlist_rejects_empty_and_wildcard_hosts() {
        let empty = EgressAllowlist::try_new(Vec::new()).expect_err("empty denied");
        assert_eq!(empty.purpose, "validate live agent egress allowlist");

        let wildcard = EgressAllowlist::try_new(vec![EgressRule::new(
            EgressPurpose::LlmProvider,
            "*.example.com",
        )])
        .expect_err("wildcard denied");
        assert_eq!(wildcard.purpose, "validate live agent egress host");
    }

    #[test]
    fn secret_handling_stores_names_only() {
        let err = LiveAgentRuntimeContract::new(
            allowlist(),
            SecretHandling::ExplicitEnv {
                names: vec!["LLM_API_KEY=secret".to_string()],
            },
            BudgetGate::new("operator", 10_000, 1_000),
        )
        .expect_err("secret values denied");

        assert_eq!(err.purpose, "validate live agent secret env name");
    }

    #[test]
    fn budget_gate_stops_at_80_percent() {
        let contract = LiveAgentRuntimeContract::new(
            allowlist(),
            SecretHandling::None,
            BudgetGate::new("operator", 100, 70),
        )
        .expect("contract");

        let receipt = contract
            .validate_estimated_call(5)
            .expect("under stop threshold");
        assert_eq!(receipt.remaining_units, 30);

        let err = contract
            .validate_estimated_call(10)
            .expect_err("80 percent threshold stops launch");
        assert_eq!(err.purpose, "check live agent budget estimate");
    }
}
