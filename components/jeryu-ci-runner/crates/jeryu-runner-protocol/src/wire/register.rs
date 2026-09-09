use super::context::{RunnerContext, WireRunnerClass};
use super::validation::{
    MAX_CLASSES, MAX_HEARTBEAT_MILLIS, MAX_LEASE_MILLIS, ValidateWire, WireError, WireErrorCode,
    validate_header, validate_id, validate_label, validate_timestamp, validate_unique,
};
use super::{MAX_CAPACITY, MAX_LABELS, REGISTER_ACK, REGISTER_REQUEST};
use crate::{PROTOCOL_VERSION, RunnerHello};
use serde::{Deserialize, Serialize};

/// Runner registration request derived from [`RunnerHello`].
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterRequest {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub runner_id: String,
    pub supported_classes: Vec<WireRunnerClass>,
    pub labels: Vec<String>,
    pub capacity: u32,
    pub sent_at_unix_millis: u64,
}

impl RegisterRequest {
    pub fn from_hello(
        request_id: impl Into<String>,
        sent_at_unix_millis: u64,
        hello: &RunnerHello,
    ) -> Result<Self, WireError> {
        if hello.protocol_version != PROTOCOL_VERSION {
            return Err(WireError::new(
                WireErrorCode::InvalidProtocol,
                "protocol_version",
            ));
        }
        let request = Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            message_type: REGISTER_REQUEST.to_string(),
            request_id: request_id.into(),
            runner_id: hello.runner_id.clone(),
            supported_classes: hello
                .supported_classes
                .iter()
                .map(WireRunnerClass::from_runner_class)
                .collect::<Result<Vec<_>, _>>()?,
            labels: hello.labels.clone(),
            capacity: hello.capacity,
            sent_at_unix_millis,
        };
        request.validate_wire()?;
        Ok(request)
    }

    pub fn to_hello(&self) -> Result<RunnerHello, WireError> {
        self.validate_wire()?;
        Ok(RunnerHello {
            runner_id: self.runner_id.clone(),
            protocol_version: self.protocol_version.clone(),
            supported_classes: self
                .supported_classes
                .iter()
                .map(WireRunnerClass::to_runner_class)
                .collect(),
            labels: self.labels.clone(),
            capacity: self.capacity,
        })
    }
}

impl ValidateWire for RegisterRequest {
    fn validate_wire(&self) -> Result<(), WireError> {
        validate_header(&self.protocol_version, &self.message_type, REGISTER_REQUEST)?;
        validate_id("request_id", &self.request_id)?;
        validate_id("runner_id", &self.runner_id)?;
        if self.supported_classes.is_empty() || self.supported_classes.len() > MAX_CLASSES {
            return Err(WireError::new(
                WireErrorCode::InvalidCollection,
                "supported_classes",
            ));
        }
        validate_unique(
            "supported_classes",
            self.supported_classes.iter().map(WireRunnerClass::as_str),
        )?;
        if self.labels.len() > MAX_LABELS {
            return Err(WireError::new(WireErrorCode::InvalidCollection, "labels"));
        }
        for label in &self.labels {
            validate_label("labels", label)?;
        }
        validate_unique("labels", self.labels.iter().map(String::as_str))?;
        if !(1..=MAX_CAPACITY).contains(&self.capacity) {
            return Err(WireError::new(WireErrorCode::InvalidField, "capacity"));
        }
        validate_timestamp("sent_at_unix_millis", self.sent_at_unix_millis)
    }
}

/// Registration acknowledgement. The epoch is a fencing token, not a secret.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterAck {
    pub protocol_version: String,
    pub message_type: String,
    pub request_id: String,
    pub runner_id: String,
    pub runner_epoch: u64,
    pub heartbeat_interval_millis: u64,
    pub lease_ttl_millis: u64,
    pub server_time_unix_millis: u64,
}

impl RegisterAck {
    pub fn new(
        request: &RegisterRequest,
        runner_epoch: u64,
        heartbeat_interval_millis: u64,
        lease_ttl_millis: u64,
        server_time_unix_millis: u64,
    ) -> Result<Self, WireError> {
        let ack = Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            message_type: REGISTER_ACK.to_string(),
            request_id: request.request_id.clone(),
            runner_id: request.runner_id.clone(),
            runner_epoch,
            heartbeat_interval_millis,
            lease_ttl_millis,
            server_time_unix_millis,
        };
        ack.validate_for(request)?;
        Ok(ack)
    }

    pub fn validate_for(&self, request: &RegisterRequest) -> Result<(), WireError> {
        self.validate_wire()?;
        request.validate_wire()?;
        if self.request_id != request.request_id || self.runner_id != request.runner_id {
            return Err(WireError::new(
                WireErrorCode::ContextMismatch,
                "register_ack",
            ));
        }
        Ok(())
    }
}

impl ValidateWire for RegisterAck {
    fn validate_wire(&self) -> Result<(), WireError> {
        validate_header(&self.protocol_version, &self.message_type, REGISTER_ACK)?;
        validate_id("request_id", &self.request_id)?;
        RunnerContext {
            runner_id: self.runner_id.clone(),
            runner_epoch: self.runner_epoch,
        }
        .validate_wire()?;
        if !(1..=MAX_HEARTBEAT_MILLIS).contains(&self.heartbeat_interval_millis)
            || !(self.heartbeat_interval_millis..=MAX_LEASE_MILLIS).contains(&self.lease_ttl_millis)
        {
            return Err(WireError::new(
                WireErrorCode::InvalidField,
                "heartbeat_interval_millis",
            ));
        }
        validate_timestamp("server_time_unix_millis", self.server_time_unix_millis)
    }
}

impl_message_io!(RegisterRequest);
impl_message_io!(RegisterAck);
impl_redacted_debug!(RegisterRequest, RegisterAck);
