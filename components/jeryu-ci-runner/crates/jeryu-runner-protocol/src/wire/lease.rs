use super::context::{ExecutionContext, RunnerContext};
use super::job::WireJobRequest;
use super::validation::{
    MAX_LEASE_MILLIS, ValidateWire, WireError, WireErrorCode, validate_header, validate_id,
    validate_timestamp,
};
use super::{LEASE_ACK, LEASE_REQUEST};
use crate::PROTOCOL_VERSION;
use serde::{Deserialize, Serialize};

/// Request for at most one lease for the current runner epoch.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseRequest {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub runner: RunnerContext,
    pub requested_at_unix_millis: u64,
}

impl LeaseRequest {
    pub fn new(
        request_id: impl Into<String>,
        runner: RunnerContext,
        requested_at_unix_millis: u64,
    ) -> Result<Self, WireError> {
        let request = Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            message_type: LEASE_REQUEST.to_string(),
            request_id: request_id.into(),
            runner,
            requested_at_unix_millis,
        };
        request.validate_wire()?;
        Ok(request)
    }
}

impl ValidateWire for LeaseRequest {
    fn validate_wire(&self) -> Result<(), WireError> {
        validate_header(&self.protocol_version, &self.message_type, LEASE_REQUEST)?;
        validate_id("request_id", &self.request_id)?;
        self.runner.validate_wire()?;
        validate_timestamp("requested_at_unix_millis", self.requested_at_unix_millis)
    }
}

/// A context-bound job granted to a runner.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseGrant {
    pub context: ExecutionContext,
    pub leased_at_unix_millis: u64,
    pub expires_at_unix_millis: u64,
    pub job: WireJobRequest,
}

impl LeaseGrant {
    pub fn new(
        context: ExecutionContext,
        leased_at_unix_millis: u64,
        expires_at_unix_millis: u64,
        job: WireJobRequest,
    ) -> Result<Self, WireError> {
        let grant = Self {
            context,
            leased_at_unix_millis,
            expires_at_unix_millis,
            job,
        };
        grant.validate_wire()?;
        Ok(grant)
    }

    pub fn validate(&self) -> Result<(), WireError> {
        self.validate_wire()
    }
}

impl ValidateWire for LeaseGrant {
    fn validate_wire(&self) -> Result<(), WireError> {
        self.context.validate_wire()?;
        self.job.validate_wire()?;
        self.context.validate_job(&self.job)?;
        validate_timestamp("leased_at_unix_millis", self.leased_at_unix_millis)?;
        validate_timestamp("expires_at_unix_millis", self.expires_at_unix_millis)?;
        if self.expires_at_unix_millis <= self.leased_at_unix_millis
            || self
                .expires_at_unix_millis
                .saturating_sub(self.leased_at_unix_millis)
                > MAX_LEASE_MILLIS
        {
            return Err(WireError::new(
                WireErrorCode::InvalidField,
                "expires_at_unix_millis",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LeaseDecision {
    Assigned,
    NoWork,
    Drain,
    Fenced,
}

/// Lease acknowledgement. `lease` exists only for `assigned`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseAck {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub runner: RunnerContext,
    pub decision: LeaseDecision,
    pub lease: Option<LeaseGrant>,
    pub server_time_unix_millis: u64,
}

impl LeaseAck {
    pub fn assigned(
        request: &LeaseRequest,
        lease: LeaseGrant,
        server_time_unix_millis: u64,
    ) -> Result<Self, WireError> {
        let ack = Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            message_type: LEASE_ACK.to_string(),
            request_id: request.request_id.clone(),
            runner: request.runner.clone(),
            decision: LeaseDecision::Assigned,
            lease: Some(lease),
            server_time_unix_millis,
        };
        ack.validate_for(request)?;
        Ok(ack)
    }

    pub fn without_work(
        request: &LeaseRequest,
        decision: LeaseDecision,
        server_time_unix_millis: u64,
    ) -> Result<Self, WireError> {
        if decision == LeaseDecision::Assigned {
            return Err(WireError::new(
                WireErrorCode::InvalidField,
                "lease_ack.decision",
            ));
        }
        let ack = Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            message_type: LEASE_ACK.to_string(),
            request_id: request.request_id.clone(),
            runner: request.runner.clone(),
            decision,
            lease: None,
            server_time_unix_millis,
        };
        ack.validate_for(request)?;
        Ok(ack)
    }

    pub fn validate_for(&self, request: &LeaseRequest) -> Result<(), WireError> {
        self.validate_wire()?;
        request.validate_wire()?;
        if self.request_id != request.request_id || self.runner != request.runner {
            return Err(WireError::new(WireErrorCode::ContextMismatch, "lease_ack"));
        }
        Ok(())
    }
}

impl ValidateWire for LeaseAck {
    fn validate_wire(&self) -> Result<(), WireError> {
        validate_header(&self.protocol_version, &self.message_type, LEASE_ACK)?;
        validate_id("request_id", &self.request_id)?;
        self.runner.validate_wire()?;
        validate_timestamp("server_time_unix_millis", self.server_time_unix_millis)?;
        match (&self.decision, &self.lease) {
            (LeaseDecision::Assigned, Some(lease)) => {
                lease.validate_wire()?;
                if lease.context.runner() != self.runner {
                    return Err(WireError::new(
                        WireErrorCode::ContextMismatch,
                        "lease_ack.lease",
                    ));
                }
                if self.server_time_unix_millis < lease.leased_at_unix_millis
                    || self.server_time_unix_millis >= lease.expires_at_unix_millis
                {
                    return Err(WireError::new(
                        WireErrorCode::InvalidField,
                        "server_time_unix_millis",
                    ));
                }
            }
            (LeaseDecision::Assigned, None) | (_, Some(_)) => {
                return Err(WireError::new(
                    WireErrorCode::InvalidField,
                    "lease_ack.decision",
                ));
            }
            (_, None) => {}
        }
        Ok(())
    }
}

impl_message_io!(LeaseRequest);
impl_message_io!(LeaseAck);
impl_redacted_debug!(LeaseRequest, LeaseGrant, LeaseAck);
