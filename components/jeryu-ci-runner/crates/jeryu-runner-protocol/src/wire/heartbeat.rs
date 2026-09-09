use super::context::{ExecutionContext, RunnerContext};
use super::validation::{
    MAX_HEARTBEAT_MILLIS, MAX_MESSAGE_TEXT_BYTES, ValidateWire, WireError, WireErrorCode,
    validate_header, validate_id, validate_text, validate_timestamp,
};
use super::{HEARTBEAT_ACK, HEARTBEAT_REQUEST};
use crate::{Heartbeat, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};

/// Runner heartbeat. An active lease echoes its complete immutable context.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatRequest {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub runner: RunnerContext,
    pub lease: Option<ExecutionContext>,
    pub monotonic_millis: u64,
    pub message: String,
    pub sent_at_unix_millis: u64,
}

impl HeartbeatRequest {
    pub fn from_heartbeat(
        request_id: impl Into<String>,
        sent_at_unix_millis: u64,
        lease: Option<ExecutionContext>,
        heartbeat: &Heartbeat,
    ) -> Result<Self, WireError> {
        let request = Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            message_type: HEARTBEAT_REQUEST.to_string(),
            request_id: request_id.into(),
            runner: RunnerContext {
                runner_id: heartbeat.runner_id.clone(),
                runner_epoch: heartbeat.runner_epoch,
            },
            lease,
            monotonic_millis: heartbeat.monotonic_millis,
            message: heartbeat.message.clone(),
            sent_at_unix_millis,
        };
        request.validate_wire()?;
        match &request.lease {
            Some(context)
                if context.run_id == heartbeat.run_id
                    && context.lease_id == heartbeat.lease_id
                    && context.job_id == heartbeat.job_id => {}
            None if heartbeat.run_id.is_empty()
                && heartbeat.lease_id.is_empty()
                && heartbeat.job_id.is_empty() => {}
            _ => {
                return Err(WireError::new(WireErrorCode::ContextMismatch, "heartbeat"));
            }
        }
        Ok(request)
    }

    pub fn to_heartbeat(&self) -> Result<Heartbeat, WireError> {
        self.validate_wire()?;
        let (run_id, lease_id, job_id) = self.lease.as_ref().map_or_else(
            || (String::new(), String::new(), String::new()),
            |context| {
                (
                    context.run_id.clone(),
                    context.lease_id.clone(),
                    context.job_id.clone(),
                )
            },
        );
        Ok(Heartbeat {
            runner_id: self.runner.runner_id.clone(),
            runner_epoch: self.runner.runner_epoch,
            run_id,
            lease_id,
            job_id,
            monotonic_millis: self.monotonic_millis,
            message: self.message.clone(),
        })
    }
}

impl ValidateWire for HeartbeatRequest {
    fn validate_wire(&self) -> Result<(), WireError> {
        validate_header(
            &self.protocol_version,
            &self.message_type,
            HEARTBEAT_REQUEST,
        )?;
        validate_id("request_id", &self.request_id)?;
        self.runner.validate_wire()?;
        if let Some(context) = &self.lease {
            context.validate_wire()?;
            if context.runner() != self.runner {
                return Err(WireError::new(
                    WireErrorCode::ContextMismatch,
                    "heartbeat.lease",
                ));
            }
        }
        validate_text("message", &self.message, MAX_MESSAGE_TEXT_BYTES)?;
        validate_timestamp("sent_at_unix_millis", self.sent_at_unix_millis)
    }
}

/// Heartbeat acknowledgement, including an exact echo of any lease context.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatAck {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub runner: RunnerContext,
    pub lease: Option<ExecutionContext>,
    pub still_owner: bool,
    pub drain: bool,
    pub next_heartbeat_millis: u64,
    pub server_time_unix_millis: u64,
}

impl HeartbeatAck {
    pub fn new(
        request: &HeartbeatRequest,
        still_owner: bool,
        drain: bool,
        next_heartbeat_millis: u64,
        server_time_unix_millis: u64,
    ) -> Result<Self, WireError> {
        let ack = Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            message_type: HEARTBEAT_ACK.to_string(),
            request_id: request.request_id.clone(),
            runner: request.runner.clone(),
            lease: request.lease.clone(),
            still_owner,
            drain,
            next_heartbeat_millis,
            server_time_unix_millis,
        };
        ack.validate_for(request)?;
        Ok(ack)
    }

    pub fn validate_for(&self, request: &HeartbeatRequest) -> Result<(), WireError> {
        self.validate_wire()?;
        request.validate_wire()?;
        if self.request_id != request.request_id
            || self.runner != request.runner
            || self.lease != request.lease
        {
            return Err(WireError::new(
                WireErrorCode::ContextMismatch,
                "heartbeat_ack",
            ));
        }
        Ok(())
    }
}

impl ValidateWire for HeartbeatAck {
    fn validate_wire(&self) -> Result<(), WireError> {
        validate_header(&self.protocol_version, &self.message_type, HEARTBEAT_ACK)?;
        validate_id("request_id", &self.request_id)?;
        self.runner.validate_wire()?;
        if let Some(context) = &self.lease {
            context.validate_wire()?;
            if context.runner() != self.runner {
                return Err(WireError::new(
                    WireErrorCode::ContextMismatch,
                    "heartbeat_ack.lease",
                ));
            }
        }
        if self.still_owner {
            if !(1..=MAX_HEARTBEAT_MILLIS).contains(&self.next_heartbeat_millis) {
                return Err(WireError::new(
                    WireErrorCode::InvalidField,
                    "next_heartbeat_millis",
                ));
            }
        } else if self.drain || self.next_heartbeat_millis != 0 {
            return Err(WireError::new(WireErrorCode::InvalidField, "still_owner"));
        }
        validate_timestamp("server_time_unix_millis", self.server_time_unix_millis)
    }
}

impl_message_io!(HeartbeatRequest);
impl_message_io!(HeartbeatAck);
impl_redacted_debug!(HeartbeatRequest, HeartbeatAck);
