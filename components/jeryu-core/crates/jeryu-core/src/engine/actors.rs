//! Opaque credential proof, revalidated under the operation's authority guard.

use std::sync::{Arc, Weak};

use chrono::Utc;

use super::bound_reviews::{ReviewActorBinding, ReviewCredentialKind};
use super::runtime::SharedRuntime;
use super::{ForgeCore, State};
use crate::{ForgeError, Result};

/// Borrowed secret input. Deliberately has no Debug or serialization implementation.
pub enum ActorCredential<'a> {
    PersonalAccessToken(&'a str),
    Session {
        token: &'a str,
        csrf_token: &'a str,
    },
    /// Read-only cookie proof. It cannot authorize a mutation or token issuance.
    SessionReadOnly(&'a str),
}

/// Minted only by Core from an actual credential. A public account summary or
/// deserialized request cannot construct this handle. Administrators in the
/// owning process remain trusted; this is not a sandbox for arbitrary Rust code.
#[derive(Clone)]
pub struct AuthenticatedActor {
    runtime: Weak<SharedRuntime>,
    binding: ReviewActorBinding,
    mutation_authorized: bool,
}

impl std::fmt::Debug for AuthenticatedActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthenticatedActor")
            .field("login", &self.binding.login)
            .field("credential_id", &self.binding.credential_id)
            .field("mutation_authorized", &self.mutation_authorized)
            .finish_non_exhaustive()
    }
}

impl ForgeCore {
    pub fn authenticate_actor(
        &self,
        credential: ActorCredential<'_>,
    ) -> Result<AuthenticatedActor> {
        self.validate_mutation_process()?;
        self.runtime.coordinator.with_repositories(&[], || {
            let state = self.runtime.state.read();
            let (login, kind, credential_id, epoch, mutation_authorized) = match credential {
                ActorCredential::PersonalAccessToken(secret) => {
                    let hash = super::auth::token_hash(secret);
                    let token = state
                        .personal_tokens
                        .values()
                        .find(|token| {
                            super::auth::constant_time_eq(
                                token.token_hash.as_bytes(),
                                hash.as_bytes(),
                            ) && token.expires_at.is_none_or(|expiry| expiry > Utc::now())
                        })
                        .ok_or_else(invalid_credential)?;
                    (
                        token.login.as_str(),
                        ReviewCredentialKind::PersonalAccessToken,
                        token.id,
                        token.auth_epoch,
                        true,
                    )
                }
                ActorCredential::Session { token, csrf_token } => {
                    let session = state
                        .sessions
                        .get(&super::auth::token_hash(token))
                        .filter(|session| session.expires_at > Utc::now())
                        .ok_or_else(invalid_credential)?;
                    if !super::auth::constant_time_eq(
                        session.csrf_token.as_bytes(),
                        csrf_token.as_bytes(),
                    ) {
                        return Err(ForgeError::Forbidden(
                            "session CSRF proof is invalid".into(),
                        ));
                    }
                    (
                        session.login.as_str(),
                        ReviewCredentialKind::Session,
                        session.id,
                        session.auth_epoch,
                        true,
                    )
                }
                ActorCredential::SessionReadOnly(token) => {
                    let session = state
                        .sessions
                        .get(&super::auth::token_hash(token))
                        .filter(|session| session.expires_at > Utc::now())
                        .ok_or_else(invalid_credential)?;
                    (
                        session.login.as_str(),
                        ReviewCredentialKind::Session,
                        session.id,
                        session.auth_epoch,
                        false,
                    )
                }
            };
            let account = state.accounts.get(login).ok_or_else(invalid_credential)?;
            let profile = state.users.get(login).ok_or_else(invalid_credential)?;
            let actor = AuthenticatedActor {
                runtime: Arc::downgrade(&self.runtime),
                binding: ReviewActorBinding {
                    login: login.to_string(),
                    profile_id: profile.id,
                    account_created_at: account.created_at,
                    auth_epoch: epoch,
                    credential_kind: kind,
                    credential_id,
                },
                mutation_authorized,
            };
            self.validate_actor_locked(&state, &actor, false)?;
            Ok(actor)
        })
    }

    /// Caller holds the authority gate, and fences the process before State.
    pub(super) fn validate_actor_locked(
        &self,
        state: &State,
        actor: &AuthenticatedActor,
        mutation: bool,
    ) -> Result<ReviewActorBinding> {
        let runtime = actor.runtime.upgrade().ok_or_else(invalid_credential)?;
        if !Arc::ptr_eq(&runtime, &self.runtime) {
            return Err(invalid_credential());
        }
        if mutation && !actor.mutation_authorized {
            return Err(ForgeError::Forbidden(
                "read-only session proof cannot authorize mutations".into(),
            ));
        }
        let binding = &actor.binding;
        let account = state
            .accounts
            .get(&binding.login)
            .ok_or_else(invalid_credential)?;
        if !account.status.permits_authentication()
            || account.must_change_password
            || account.auth_epoch != binding.auth_epoch
            || account.created_at != binding.account_created_at
            || !state
                .users
                .get(&binding.login)
                .is_some_and(|profile| profile.id == binding.profile_id)
        {
            return Err(invalid_credential());
        }
        let live = match binding.credential_kind {
            ReviewCredentialKind::PersonalAccessToken => state
                .personal_tokens
                .get(&binding.credential_id)
                .is_some_and(|token| {
                    token.login == binding.login
                        && token.auth_epoch == binding.auth_epoch
                        && token.expires_at.is_none_or(|expiry| expiry > Utc::now())
                }),
            ReviewCredentialKind::Session => state.sessions.values().any(|session| {
                session.id == binding.credential_id
                    && session.login == binding.login
                    && session.auth_epoch == binding.auth_epoch
                    && session.expires_at > Utc::now()
            }),
        };
        if !live {
            return Err(invalid_credential());
        }
        Ok(binding.clone())
    }
}

fn invalid_credential() -> ForgeError {
    ForgeError::Unauthenticated(
        "credential is absent, expired, revoked or no longer eligible".into(),
    )
}
