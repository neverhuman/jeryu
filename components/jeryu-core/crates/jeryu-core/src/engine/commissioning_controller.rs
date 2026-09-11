//! One Core controller for a future Deploy transport. No production signing,
//! enrollment, HTTP route or filesystem dispatcher is installed by this module.

use chrono::{DateTime, Utc};
use std::sync::Arc;
use uuid::Uuid;

use super::commissioning_effects::*;
use super::{AuthenticatedActor, ForgeCore, RequiredPublisherCustody, ReviewActorBinding};
use crate::{ForgeError, Result};

#[cfg(test)]
mod tests;

/// Every value is derived by the trusted installed verifier, including P/B/C,
/// accepted-ref observations, signed contract/role epochs, target ABI, all
/// executable/config/assets/schema/recovery identities and actual pair custody.
/// The verifier must not call public Core methods while its caller holds guards.
pub trait CommissioningAuthority: std::fmt::Debug + Send + Sync {
    fn custody(&self) -> &RequiredPublisherCustody;

    fn reserve(
        &self,
        actual: &RequiredPublisherCustody,
        repository_id: Uuid,
        contract_id: Uuid,
        actor: &ReviewActorBinding,
        request: &CommissioningRestoreRequest,
        now: DateTime<Utc>,
    ) -> Result<CommissioningRestoreScope>;

    /// Readback and evidence recording have separately scoped current authority.
    /// Evidence-only recovery may be authorized after the original operator is
    /// revoked/expired, using another current authenticated recovery recorder.
    /// A revoked credential itself can never authenticate or submit a record.
    fn authorize(
        &self,
        actual: &RequiredPublisherCustody,
        operation: &CommissioningRestoreOperation,
        actor: &ReviewActorBinding,
        action: CommissioningAction<'_>,
        now: DateTime<Utc>,
    ) -> Result<CommissioningRecordingAuthority>;
}

#[derive(Debug, Clone, Copy)]
pub enum CommissioningAction<'a> {
    Readback,
    AdmitStep(&'a CommissioningStepRequest),
    RecordOutcome(&'a CommissioningCompletionRequest),
    Close {
        expected: &'a CommissioningRevision,
        receiving_evidence: &'a [u8],
    },
}

impl ForgeCore {
    /// Trusted process installation hook. There is no request-body constructor
    /// and no permissive default implementation. Attachment measures the actual
    /// database/root and both live flock descriptors before accepting the hook.
    pub fn with_commissioning_authority(
        self,
        authority: Arc<dyn CommissioningAuthority>,
    ) -> Result<Self> {
        self.with_installation_custody(|| {
            self.check_commissioning_custody(authority.as_ref())?;
            let mut slot = self.runtime.commissioning_authority.write();
            if slot.is_some() {
                return Err(conflict("commissioning authority is already attached"));
            }
            *slot = Some(authority);
            Ok(())
        })?;
        Ok(self)
    }

    pub fn reserve_commissioning_operation(
        &self,
        actor: &AuthenticatedActor,
        repository_id: Uuid,
        contract_id: Uuid,
        request: CommissioningRestoreRequest,
    ) -> Result<CommissioningRestoreOperation> {
        request.validate()?;
        if repository_id.is_nil() || contract_id.is_nil() {
            return Err(invalid("nil commissioning scope"));
        }
        self.validate_mutation_process()?;
        self.runtime.coordinator.with_authority(&[repository_id], || {
            let (_, binding) = self.required_actor_context(actor, repository_id, true)?;
            let authority = self.commissioning_authority()?;
            let before = self.check_commissioning_custody(authority.as_ref())?;
            let scope = authority.reserve(&before, repository_id, contract_id, &binding, &request, Utc::now())?;
            scope.validate()?;
            if scope.repository_id != repository_id || scope.contract_id != contract_id
                || scope.operator != binding || scope.request != request {
                return Err(conflict("verified commissioning scope differs from the actual requested actor and input"));
            }
            self.required_actor_context(actor, repository_id, true)?;
            if self.check_commissioning_custody(authority.as_ref())? != before {
                return Err(unavailable());
            }
            self.runtime.storage.as_ref().ok_or_else(unavailable)?
                .reserve_commissioning_restore(&scope, Utc::now())
        })
    }

    pub fn commissioning_operation(
        &self,
        actor: &AuthenticatedActor,
        address: CommissioningOperationAddress,
    ) -> Result<CommissioningRestoreOperation> {
        self.with_commissioning_operation(
            actor,
            address,
            false,
            |authority, actual, operation, binding| {
                authority.authorize(
                    actual,
                    &operation,
                    binding,
                    CommissioningAction::Readback,
                    Utc::now(),
                )?;
                Ok(operation)
            },
        )
    }

    /// The returned durable step is not an external filesystem capability by
    /// itself. The future installed dispatcher must keep local authority for
    /// the complete effect and enforce the durable operation-wide barrier.
    pub fn admit_commissioning_step(
        &self,
        actor: &AuthenticatedActor,
        address: CommissioningOperationAddress,
        request: CommissioningStepRequest,
    ) -> Result<CommissioningRestoreOperation> {
        self.with_commissioning_operation(
            actor,
            address,
            true,
            |authority, actual, operation, binding| {
                let now = Utc::now();
                let role = authority.authorize(
                    actual,
                    &operation,
                    binding,
                    CommissioningAction::AdmitStep(&request),
                    now,
                )?;
                if role != CommissioningRecordingAuthority::CurrentOperator
                    || *binding != operation.scope.operator
                {
                    return Err(ForgeError::Forbidden(
                        "a recovery recorder cannot admit filesystem effects".into(),
                    ));
                }
                operation.scope.require_window(Utc::now())?;
                self.recheck_commissioning_actor_custody(
                    actor,
                    address.repository_id,
                    binding,
                    authority,
                    actual,
                )?;
                self.runtime
                    .storage
                    .as_ref()
                    .ok_or_else(unavailable)?
                    .admit_commissioning_step(address.operation_id, &request, binding, Utc::now())
            },
        )
    }

    pub fn complete_commissioning_step(
        &self,
        actor: &AuthenticatedActor,
        address: CommissioningOperationAddress,
        request: CommissioningCompletionRequest,
    ) -> Result<CommissioningRestoreOperation> {
        self.with_commissioning_operation(
            actor,
            address,
            true,
            |authority, actual, operation, binding| {
                // Deliberately no original-window precondition here. The trusted
                // verifier still authenticates current evidence-recording scope;
                // the reducer classifies late outcomes as non-authorizing.
                let role = authority.authorize(
                    actual,
                    &operation,
                    binding,
                    CommissioningAction::RecordOutcome(&request),
                    Utc::now(),
                )?;
                if role == CommissioningRecordingAuthority::CurrentOperator
                    && *binding != operation.scope.operator
                {
                    return Err(ForgeError::Forbidden(
                        "current operator recording scope differs from original enrollment".into(),
                    ));
                }
                self.recheck_commissioning_actor_custody(
                    actor,
                    address.repository_id,
                    binding,
                    authority,
                    actual,
                )?;
                self.runtime
                    .storage
                    .as_ref()
                    .ok_or_else(unavailable)?
                    .complete_commissioning_step(
                        address.operation_id,
                        &request,
                        binding,
                        role,
                        Utc::now(),
                    )
            },
        )
    }

    pub fn close_commissioning_operation(
        &self,
        actor: &AuthenticatedActor,
        address: CommissioningOperationAddress,
        expected: CommissioningRevision,
        receiving_evidence: Vec<u8>,
    ) -> Result<CommissioningRestoreOperation> {
        evidence_shape(&receiving_evidence)?;
        self.with_commissioning_operation(
            actor,
            address,
            true,
            |authority, actual, operation, binding| {
                let role = authority.authorize(
                    actual,
                    &operation,
                    binding,
                    CommissioningAction::Close {
                        expected: &expected,
                        receiving_evidence: &receiving_evidence,
                    },
                    Utc::now(),
                )?;
                if role != CommissioningRecordingAuthority::CurrentOperator
                    || *binding != operation.scope.operator
                {
                    return Err(ForgeError::Forbidden(
                        "recording authority cannot clear the restoration barrier".into(),
                    ));
                }
                self.recheck_commissioning_actor_custody(
                    actor,
                    address.repository_id,
                    binding,
                    authority,
                    actual,
                )?;
                self.runtime
                    .storage
                    .as_ref()
                    .ok_or_else(unavailable)?
                    .close_commissioning_operation(
                        address.operation_id,
                        &expected,
                        binding,
                        &receiving_evidence,
                        Utc::now(),
                    )
            },
        )
    }

    fn with_commissioning_operation<T>(
        &self,
        actor: &AuthenticatedActor,
        address: CommissioningOperationAddress,
        mutation: bool,
        execute: impl FnOnce(
            &dyn CommissioningAuthority,
            &RequiredPublisherCustody,
            CommissioningRestoreOperation,
            &ReviewActorBinding,
        ) -> Result<T>,
    ) -> Result<T> {
        let CommissioningOperationAddress {
            repository_id,
            contract_id,
            operation_id,
        } = address;
        if repository_id.is_nil() || contract_id.is_nil() || operation_id.is_nil() {
            return Err(invalid("nil commissioning readback or operation scope"));
        }
        self.validate_mutation_process()?;
        self.runtime
            .coordinator
            .with_authority(&[repository_id], || {
                let (_, binding) = self.required_actor_context(actor, repository_id, mutation)?;
                let authority = self.commissioning_authority()?;
                let before = self.check_commissioning_custody(authority.as_ref())?;
                let operation = self
                    .runtime
                    .storage
                    .as_ref()
                    .ok_or_else(unavailable)?
                    .commissioning_operation(operation_id)?;
                if operation.scope.repository_id != repository_id
                    || operation.scope.contract_id != contract_id
                {
                    return Err(ForgeError::NotFound(
                        "commissioning operation in requested scope".into(),
                    ));
                }
                let result = execute(authority.as_ref(), &before, operation, &binding)?;
                self.required_actor_context(actor, repository_id, mutation)?;
                if self.check_commissioning_custody(authority.as_ref())? != before {
                    return Err(unavailable());
                }
                Ok(result)
            })
    }

    fn commissioning_authority(&self) -> Result<Arc<dyn CommissioningAuthority>> {
        self.runtime
            .commissioning_authority
            .read()
            .clone()
            .ok_or_else(unavailable)
    }

    fn recheck_commissioning_actor_custody(
        &self,
        actor: &AuthenticatedActor,
        repository_id: Uuid,
        expected_actor: &ReviewActorBinding,
        authority: &dyn CommissioningAuthority,
        expected_custody: &RequiredPublisherCustody,
    ) -> Result<()> {
        let (_, current) = self.required_actor_context(actor, repository_id, true)?;
        if &current != expected_actor {
            return Err(ForgeError::Forbidden(
                "commissioning actor changed during authority verification".into(),
            ));
        }
        if &self.check_commissioning_custody(authority)? != expected_custody {
            return Err(unavailable());
        }
        Ok(())
    }

    fn check_commissioning_custody(
        &self,
        authority: &dyn CommissioningAuthority,
    ) -> Result<RequiredPublisherCustody> {
        // Private measurement within the already-held coordinator. Calling the
        // public custody wrapper here would recursively acquire authority-write.
        let actual = self
            .runtime
            .storage
            .as_ref()
            .ok_or_else(unavailable)?
            .required_publisher_custody()?;
        if &actual != authority.custody() {
            return Err(unavailable());
        }
        Ok(actual)
    }
}

fn unavailable() -> ForgeError {
    ForgeError::WriterUnavailable(
        "commissioning trust, current custody or recovery service is unavailable".into(),
    )
}
