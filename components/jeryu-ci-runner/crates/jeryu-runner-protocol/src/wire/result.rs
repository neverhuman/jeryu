use super::context::ExecutionContext;
use super::job::WireJobResult;
use super::lease::LeaseGrant;
use super::validation::{
    CanonicalSha256, ValidateWire, WireError, WireErrorCode, validate_header, validate_id,
    validate_timestamp,
};
use super::{RESULT_ACK, RESULT_REQUEST};
use crate::PROTOCOL_VERSION;
use serde::{Deserialize, Serialize};

/// Result submission carrying a deterministic, full-context idempotency key.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultRequest {
    pub protocol_version: String,
    pub message_type: String,
    pub context: ExecutionContext,
    pub result: WireJobResult,
    pub receipt_id: String,
    pub submitted_at_unix_millis: u64,
}

impl ResultRequest {
    pub fn new(
        context: ExecutionContext,
        result: WireJobResult,
        submitted_at_unix_millis: u64,
    ) -> Result<Self, WireError> {
        context.validate_wire()?;
        result.validate_wire()?;
        context.validate_result(&result)?;
        let receipt_id = result_receipt_id(&context, &result);
        let request = Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            message_type: RESULT_REQUEST.to_string(),
            context,
            result,
            receipt_id,
            submitted_at_unix_millis,
        };
        request.validate_wire()?;
        Ok(request)
    }

    #[must_use]
    pub fn expected_receipt_id(&self) -> String {
        result_receipt_id(&self.context, &self.result)
    }

    pub fn validate_for(&self, lease: &LeaseGrant) -> Result<(), WireError> {
        self.validate_wire()?;
        lease.validate_wire()?;
        if self.context != lease.context {
            return Err(WireError::new(
                WireErrorCode::ContextMismatch,
                "result_request",
            ));
        }
        Ok(())
    }
}

impl ValidateWire for ResultRequest {
    fn validate_wire(&self) -> Result<(), WireError> {
        validate_header(&self.protocol_version, &self.message_type, RESULT_REQUEST)?;
        self.context.validate_wire()?;
        self.result.validate_wire()?;
        self.context.validate_result(&self.result)?;
        validate_id("receipt_id", &self.receipt_id)?;
        if self.receipt_id != result_receipt_id(&self.context, &self.result) {
            return Err(WireError::new(WireErrorCode::ReceiptMismatch, "receipt_id"));
        }
        validate_timestamp("submitted_at_unix_millis", self.submitted_at_unix_millis)?;
        if self.submitted_at_unix_millis < self.result.finished_at_unix_millis {
            return Err(WireError::new(
                WireErrorCode::InvalidField,
                "submitted_at_unix_millis",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResultDecision {
    Accepted,
    Duplicate,
    Fenced,
}

/// Result acknowledgement echoing the full context and idempotency key.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultAck {
    pub protocol_version: String,
    pub message_type: String,
    pub context: ExecutionContext,
    pub receipt_id: String,
    pub decision: ResultDecision,
    pub server_time_unix_millis: u64,
}

impl ResultAck {
    pub fn new(
        request: &ResultRequest,
        decision: ResultDecision,
        server_time_unix_millis: u64,
    ) -> Result<Self, WireError> {
        let ack = Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            message_type: RESULT_ACK.to_string(),
            context: request.context.clone(),
            receipt_id: request.receipt_id.clone(),
            decision,
            server_time_unix_millis,
        };
        ack.validate_for(request)?;
        Ok(ack)
    }

    pub fn validate_for(&self, request: &ResultRequest) -> Result<(), WireError> {
        self.validate_wire()?;
        request.validate_wire()?;
        if self.context != request.context || self.receipt_id != request.receipt_id {
            return Err(WireError::new(WireErrorCode::ContextMismatch, "result_ack"));
        }
        Ok(())
    }
}

impl ValidateWire for ResultAck {
    fn validate_wire(&self) -> Result<(), WireError> {
        validate_header(&self.protocol_version, &self.message_type, RESULT_ACK)?;
        self.context.validate_wire()?;
        validate_id("receipt_id", &self.receipt_id)?;
        validate_timestamp("server_time_unix_millis", self.server_time_unix_millis)
    }
}

fn result_receipt_id(context: &ExecutionContext, result: &WireJobResult) -> String {
    let mut digest = CanonicalSha256::new("jeryu.runner.result-receipt.v1");
    digest.field("protocol", PROTOCOL_VERSION);
    digest.field("runner_id", &context.runner_id);
    digest.field("runner_epoch", &context.runner_epoch.to_string());
    for (name, value) in [
        ("run_id", context.run_id.as_str()),
        ("lease_id", context.lease_id.as_str()),
        ("job_id", context.job_id.as_str()),
        ("repository", context.repository.as_str()),
        ("head_sha", context.head_sha.as_str()),
        ("required_check", context.required_check.as_str()),
        ("job_digest", context.job_digest.as_str()),
        (
            "protected_policy_sha",
            context.protected_policy_sha.as_str(),
        ),
        ("toolchain_digest", context.toolchain_digest.as_str()),
        (
            "runner_class_policy_id",
            context.runner_class_policy_id.as_str(),
        ),
        (
            "runner_class_policy_digest",
            context.runner_class_policy_digest.as_str(),
        ),
        ("image_digest", context.image_digest.as_str()),
        ("rootfs_digest", context.rootfs_digest.as_str()),
        ("outcome", result.outcome.as_str()),
    ] {
        digest.field(name, value);
    }
    digest.field(
        "exit_code",
        &result
            .exit_code
            .map_or_else(|| "none".to_string(), |code| code.to_string()),
    );
    digest.field(
        "started_at_unix_millis",
        &result.started_at_unix_millis.to_string(),
    );
    digest.field(
        "finished_at_unix_millis",
        &result.finished_at_unix_millis.to_string(),
    );
    for artifact_digest in &result.artifact_digests {
        digest.field("artifact_digest", artifact_digest);
    }
    for receipt in &result.cache_receipts {
        digest.field("cache_receipt", receipt);
    }
    digest.field("log_digest", &result.log_digest);
    digest.finish()
}

impl_message_io!(ResultRequest);
impl_message_io!(ResultAck);
impl_redacted_debug!(ResultRequest, ResultAck);
