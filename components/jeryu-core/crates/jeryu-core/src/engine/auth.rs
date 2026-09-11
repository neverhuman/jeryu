//! Account credentials, sessions, PATs, and repository grants.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{ForgeCore, require_name};
use crate::errors::{ForgeError, Result};
use crate::model::*;

const SESSION_TTL_SECS: i64 = 60 * 60 * 24 * 14;
const PAT_DEFAULT_TTL_DAYS: i64 = 90;
const PAT_MAX_TTL_DAYS: i64 = 365;
const INVITATION_MAX_TTL_HOURS: i64 = 24;
const ACTIVATION_CHALLENGE_TTL_MINUTES: i64 = 10;
const ACTIVATION_MAX_ATTEMPTS: u32 = 5;
const ACTIVATION_ERROR: &str = "activation could not be completed";

mod crypto;
use self::crypto::*;

impl ForgeCore {
    pub fn generate_one_time_password(&self) -> Result<String> {
        let secret = random_secret()?;
        Ok(format!("jeryu-{}", &secret[..32]))
    }

    pub fn create_account(
        &self,
        login: &str,
        password: &str,
        role: UserRole,
    ) -> Result<AccountSummary> {
        require_login(login)?;
        require_password(password)?;
        let mut state = self.runtime.state.write();
        if state.accounts.contains_key(login) {
            return Err(ForgeError::Conflict(format!("account {login}")));
        }
        let previous = state.clone();
        let now = Utc::now();
        let account = UserAccount {
            canonical_login: login.to_string(),
            display_name: login.to_string(),
            password_hash: hash_password(password)?,
            role,
            status: AccountStatus::Active,
            auth_epoch: 0,
            must_change_password: false,
            created_at: now,
            updated_at: now,
        };
        state
            .users
            .entry(login.to_string())
            .or_insert_with(|| User {
                id: Uuid::new_v4(),
                login: login.to_string(),
                name: None,
                email: None,
                created_at: now,
            });
        state.accounts.insert(login.to_string(), account.clone());
        self.persist_after_mutation(&mut state, previous)?;
        Ok(account.into())
    }

    pub fn create_temporary_account(
        &self,
        login: &str,
        password: &str,
        role: UserRole,
    ) -> Result<AccountSummary> {
        let account = self.create_account(login, password, role)?;
        self.force_password_change(login, true)?;
        Ok(self.get_account(login).unwrap_or(account))
    }

    pub fn list_accounts(&self) -> Vec<AccountSummary> {
        let mut accounts: Vec<_> = self
            .runtime
            .state
            .read()
            .accounts
            .values()
            .cloned()
            .map(AccountSummary::from)
            .collect();
        accounts.sort_by(|a, b| a.login.cmp(&b.login));
        accounts
    }

    pub fn get_account(&self, login: &str) -> Result<AccountSummary> {
        self.runtime
            .state
            .read()
            .accounts
            .get(login)
            .cloned()
            .map(AccountSummary::from)
            .ok_or_else(|| ForgeError::NotFound(format!("account {login}")))
    }

    pub fn authenticate_password(&self, login: &str, password: &str) -> Result<AccountSummary> {
        let account = self
            .runtime
            .state
            .read()
            .accounts
            .get(login)
            .cloned()
            .ok_or_else(|| ForgeError::Validation("invalid login or password".to_string()))?;
        if !account.status.permits_authentication() {
            return Err(ForgeError::Validation(
                "invalid login or password".to_string(),
            ));
        }
        verify_password(password, &account.password_hash)?;
        Ok(account.into())
    }

    pub fn reset_account_password(
        &self,
        login: &str,
        new_password: &str,
    ) -> Result<AccountSummary> {
        require_password(new_password)?;
        let mut state = self.runtime.state.write();
        if !state.accounts.contains_key(login) {
            return Err(ForgeError::NotFound(format!("account {login}")));
        }
        let previous = state.clone();
        let account = state
            .accounts
            .get_mut(login)
            .expect("presence checked above");
        account.password_hash = hash_password(new_password)?;
        account.must_change_password = true;
        account.auth_epoch = next_auth_epoch(account.auth_epoch)?;
        account.updated_at = Utc::now();
        let updated = account.clone();
        state.sessions.retain(|_, session| session.login != login);
        state
            .personal_tokens
            .retain(|_, token| token.login != login);
        self.persist_after_mutation(&mut state, previous)?;
        Ok(updated.into())
    }

    pub fn change_account_password(
        &self,
        login: &str,
        current_password: &str,
        new_password: &str,
    ) -> Result<AccountSummary> {
        require_password(new_password)?;
        let account = self
            .runtime
            .state
            .read()
            .accounts
            .get(login)
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("account {login}")))?;
        if !account.status.permits_authentication() {
            return Err(ForgeError::Validation(
                "invalid login or password".to_string(),
            ));
        }
        verify_password(current_password, &account.password_hash)?;
        let mut state = self.runtime.state.write();
        let previous = state.clone();
        let account = state
            .accounts
            .get_mut(login)
            .expect("account was read before write lock");
        account.password_hash = hash_password(new_password)?;
        account.must_change_password = false;
        account.auth_epoch = next_auth_epoch(account.auth_epoch)?;
        account.updated_at = Utc::now();
        let updated = account.clone();
        state.sessions.retain(|_, session| session.login != login);
        state
            .personal_tokens
            .retain(|_, token| token.login != login);
        self.persist_after_mutation(&mut state, previous)?;
        Ok(updated.into())
    }

    pub fn force_password_change(&self, login: &str, forced: bool) -> Result<AccountSummary> {
        let mut state = self.runtime.state.write();
        if !state.accounts.contains_key(login) {
            return Err(ForgeError::NotFound(format!("account {login}")));
        }
        let previous = state.clone();
        let account = state
            .accounts
            .get_mut(login)
            .expect("presence checked above");
        account.must_change_password = forced;
        if forced {
            account.auth_epoch = next_auth_epoch(account.auth_epoch)?;
        }
        account.updated_at = Utc::now();
        let updated = account.clone();
        if forced {
            state.sessions.retain(|_, session| session.login != login);
            state
                .personal_tokens
                .retain(|_, token| token.login != login);
        }
        self.persist_after_mutation(&mut state, previous)?;
        Ok(updated.into())
    }

    pub fn create_session(&self, login: &str) -> Result<SessionReceipt> {
        self.create_session_with_ttl(login, chrono::Duration::seconds(SESSION_TTL_SECS))
    }

    pub fn create_session_with_ttl(
        &self,
        login: &str,
        ttl: chrono::Duration,
    ) -> Result<SessionReceipt> {
        if ttl <= chrono::Duration::zero() {
            return Err(ForgeError::Validation(
                "session ttl must be positive".to_string(),
            ));
        }
        let account = self.require_active_account(login)?;
        let token = random_secret()?;
        let csrf_token = random_secret()?;
        let created_at = Utc::now();
        let expires_at = created_at
            .checked_add_signed(ttl)
            .ok_or_else(|| ForgeError::Validation("session ttl is out of range".to_string()))?;
        let session = WebSession {
            id: Uuid::new_v4(),
            login: login.to_string(),
            auth_epoch: account.auth_epoch,
            token_hash: token_hash(&token),
            csrf_token,
            created_at,
            expires_at,
        };
        let mut state = self.runtime.state.write();
        let previous = state.clone();
        state
            .sessions
            .insert(session.token_hash.clone(), session.clone());
        self.persist_after_mutation(&mut state, previous)?;
        Ok(SessionReceipt { session, token })
    }

    pub fn authenticate_session(&self, token: &str) -> Option<AccountSummary> {
        self.session_for_token(token).map(|(account, _)| account)
    }

    pub fn session_for_token(&self, token: &str) -> Option<(AccountSummary, WebSession)> {
        let hash = token_hash(token);
        let state = self.runtime.state.read();
        let session = state.sessions.get(&hash)?;
        if session.expires_at <= Utc::now() {
            return None;
        }
        let account = state.accounts.get(&session.login).cloned()?;
        if !account.status.permits_authentication() || account.auth_epoch != session.auth_epoch {
            return None;
        }
        let account = AccountSummary::from(account);
        Some((account, session.clone()))
    }

    pub fn session_csrf_matches(&self, token: &str, csrf_token: &str) -> bool {
        self.session_for_token(token).is_some_and(|(_, session)| {
            constant_time_eq(session.csrf_token.as_bytes(), csrf_token.as_bytes())
        })
    }

    pub fn revoke_session(&self, token: &str) -> Result<()> {
        let hash = token_hash(token);
        let mut state = self.runtime.state.write();
        let previous = state.clone();
        state.sessions.remove(&hash);
        self.persist_after_mutation(&mut state, previous)
    }

    pub fn create_personal_access_token(
        &self,
        login: &str,
        name: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<PersonalAccessTokenReceipt> {
        let account = self.require_active_account(login)?;
        require_name("token name", name)?;
        let expires_at = validate_pat_expiry(expires_at)?;
        let secret = format!("jpat_{}", random_secret()?);
        let token = PersonalAccessToken {
            id: Uuid::new_v4(),
            login: login.to_string(),
            auth_epoch: account.auth_epoch,
            name: name.trim().to_string(),
            token_hash: token_hash(&secret),
            created_at: Utc::now(),
            expires_at,
        };
        let mut state = self.runtime.state.write();
        let previous = state.clone();
        state.personal_tokens.insert(token.id, token.clone());
        self.persist_after_mutation(&mut state, previous)?;
        Ok(PersonalAccessTokenReceipt { token, secret })
    }

    pub fn list_personal_access_tokens(
        &self,
        login: &str,
    ) -> Result<Vec<PersonalAccessTokenSummary>> {
        self.get_account(login)?;
        let mut tokens: Vec<_> = self
            .runtime
            .state
            .read()
            .personal_tokens
            .values()
            .filter(|token| token.login == login)
            .cloned()
            .map(PersonalAccessTokenSummary::from)
            .collect();
        tokens.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(tokens)
    }

    pub fn revoke_personal_access_token(&self, login: &str, id: Uuid) -> Result<bool> {
        self.get_account(login)?;
        let mut state = self.runtime.state.write();
        let previous = state.clone();
        let removed = state
            .personal_tokens
            .get(&id)
            .is_some_and(|token| token.login == login);
        if removed {
            state.personal_tokens.remove(&id);
        }
        self.persist_after_mutation(&mut state, previous)?;
        Ok(removed)
    }

    pub fn authenticate_personal_access_token(&self, token: &str) -> Option<AccountSummary> {
        let hash = token_hash(token);
        let now = Utc::now();
        let state = self.runtime.state.read();
        let token = state.personal_tokens.values().find(|record| {
            record.token_hash == hash && record.expires_at.is_none_or(|expires| expires > now)
        })?;
        let account = state.accounts.get(&token.login)?.clone();
        if !account.status.permits_authentication() || account.auth_epoch != token.auth_epoch {
            return None;
        }
        Some(AccountSummary::from(account))
    }

    /// Reserve a canonical login with a single-use, hash-only activation secret.
    pub fn create_account_invitation(
        &self,
        issuer_principal: &str,
        canonical_login: &str,
        display_name: &str,
        intended_bindings: InvitationBindings,
        expires_at: DateTime<Utc>,
    ) -> Result<AccountInvitationReceipt> {
        self.create_account_invitation_inner(
            issuer_principal,
            canonical_login,
            display_name,
            intended_bindings,
            expires_at,
            false,
        )
    }

    /// Create the only permitted first-owner bootstrap invitation.
    pub fn create_bootstrap_owner_invitation(
        &self,
        canonical_login: &str,
        display_name: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<AccountInvitationReceipt> {
        self.create_account_invitation_inner(
            "local-operator",
            canonical_login,
            display_name,
            InvitationBindings {
                role: UserRole::Admin,
                teams: Vec::new(),
            },
            expires_at,
            true,
        )
    }

    fn create_account_invitation_inner(
        &self,
        issuer_principal: &str,
        canonical_login: &str,
        display_name: &str,
        mut intended_bindings: InvitationBindings,
        expires_at: DateTime<Utc>,
        bootstrap_owner: bool,
    ) -> Result<AccountInvitationReceipt> {
        require_login(canonical_login)?;
        require_name("display name", display_name)?;
        require_name("issuer principal", issuer_principal)?;
        intended_bindings.teams = normalize_team_bindings(&intended_bindings.teams)?;
        let now = Utc::now();
        if expires_at <= now {
            return Err(ForgeError::Validation(
                "invitation expiry must be in the future".to_string(),
            ));
        }
        if expires_at > now + chrono::Duration::hours(INVITATION_MAX_TTL_HOURS) {
            return Err(ForgeError::Validation(format!(
                "invitation expiry may not exceed {INVITATION_MAX_TTL_HOURS} hours"
            )));
        }
        let activation_secret = random_secret()?;
        let mut state = self.runtime.state.write();
        if state.accounts.contains_key(canonical_login) {
            return Err(ForgeError::Conflict(format!("account {canonical_login}")));
        }
        if bootstrap_owner
            && (state.bootstrap_owner_consumed
                || state
                    .accounts
                    .values()
                    .any(|account| account.role == UserRole::Admin))
        {
            return Err(ForgeError::Conflict(
                "owner bootstrap has already been consumed".to_string(),
            ));
        }
        for team_binding in &intended_bindings.teams {
            let (organization, team) = split_team_binding(team_binding)?;
            if !state
                .teams
                .contains_key(&(organization.to_string(), team.to_string()))
            {
                return Err(ForgeError::NotFound(format!("team {team_binding}")));
            }
        }
        let expired_ids: BTreeSet<_> = state
            .invitations
            .values()
            .filter(|invitation| {
                invitation.consumed_at.is_none()
                    && invitation.revoked_at.is_none()
                    && invitation.expires_at <= now
            })
            .map(|invitation| invitation.id)
            .collect();
        if state.invitations.values().any(|invitation| {
            invitation.canonical_login == canonical_login
                && invitation.consumed_at.is_none()
                && invitation.revoked_at.is_none()
                && !expired_ids.contains(&invitation.id)
        }) {
            return Err(ForgeError::Conflict(format!(
                "active invitation for {canonical_login}"
            )));
        }
        if bootstrap_owner
            && state.invitations.values().any(|invitation| {
                invitation.bootstrap_owner
                    && invitation.consumed_at.is_none()
                    && invitation.revoked_at.is_none()
                    && !expired_ids.contains(&invitation.id)
            })
        {
            return Err(ForgeError::Conflict(
                "active owner bootstrap invitation".to_string(),
            ));
        }
        let previous = state.clone();
        for id in expired_ids {
            state
                .invitations
                .get_mut(&id)
                .expect("expired invitation id came from the same map")
                .revoked_at = Some(now);
        }
        let invitation = AccountInvitation {
            id: Uuid::new_v4(),
            canonical_login: canonical_login.to_string(),
            display_name: display_name.trim().to_string(),
            activation_secret_hash: token_hash(&activation_secret),
            issuer_principal: issuer_principal.trim().to_string(),
            intended_bindings,
            created_at: now,
            expires_at,
            consumed_at: None,
            revoked_at: None,
            attempt_count: 0,
            bootstrap_owner,
        };
        state.invitations.insert(invitation.id, invitation.clone());
        self.persist_after_mutation(&mut state, previous)?;
        Ok(AccountInvitationReceipt {
            invitation: invitation.into(),
            activation_secret,
        })
    }

    pub fn list_account_invitations(&self) -> Vec<AccountInvitationSummary> {
        let mut invitations: Vec<_> = self
            .runtime
            .state
            .read()
            .invitations
            .values()
            .cloned()
            .map(AccountInvitationSummary::from)
            .collect();
        invitations.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| left.canonical_login.cmp(&right.canonical_login))
        });
        invitations
    }

    pub fn revoke_account_invitation(&self, id: Uuid) -> Result<bool> {
        let mut state = self.runtime.state.write();
        let previous = state.clone();
        let revoked = if let Some(invitation) = state.invitations.get_mut(&id) {
            if invitation.consumed_at.is_none() && invitation.revoked_at.is_none() {
                invitation.revoked_at = Some(Utc::now());
                true
            } else {
                false
            }
        } else {
            false
        };
        self.persist_after_mutation(&mut state, previous)?;
        Ok(revoked)
    }

    /// Exchange a valid invitation secret for a short-lived one-time challenge.
    pub fn start_account_activation(
        &self,
        canonical_login: &str,
        activation_secret: &str,
    ) -> Result<ActivationStartReceipt> {
        if require_login(canonical_login).is_err() {
            return Err(activation_error());
        }
        let challenge = random_secret()?;
        let now = Utc::now();
        let mut state = self.runtime.state.write();
        let invitation_id = state
            .invitations
            .values()
            .find(|invitation| {
                invitation.canonical_login == canonical_login
                    && invitation.consumed_at.is_none()
                    && invitation.revoked_at.is_none()
                    && invitation.expires_at > now
            })
            .map(|invitation| invitation.id)
            .ok_or_else(activation_error)?;
        let previous = state.clone();
        let invitation = state
            .invitations
            .get_mut(&invitation_id)
            .expect("invitation id was selected from the same map");
        if invitation.attempt_count >= ACTIVATION_MAX_ATTEMPTS {
            return Err(activation_error());
        }
        invitation.attempt_count += 1;
        let supplied_hash = token_hash(activation_secret);
        if !constant_time_eq(
            supplied_hash.as_bytes(),
            invitation.activation_secret_hash.as_bytes(),
        ) {
            self.persist_after_mutation(&mut state, previous)?;
            return Err(activation_error());
        }
        let expires_at = now + chrono::Duration::minutes(ACTIVATION_CHALLENGE_TTL_MINUTES);
        let record = ActivationChallenge {
            id: Uuid::new_v4(),
            invitation_id,
            challenge_hash: token_hash(&challenge),
            created_at: now,
            expires_at,
            consumed_at: None,
        };
        state
            .activation_challenges
            .insert(record.challenge_hash.clone(), record);
        self.persist_after_mutation(&mut state, previous)?;
        Ok(ActivationStartReceipt {
            challenge,
            expires_at,
        })
    }

    /// Complete activation exactly once and leave the new account MFA-pending.
    pub fn complete_account_activation(
        &self,
        challenge: &str,
        password: &str,
    ) -> Result<AccountSummary> {
        require_password(password)?;
        let password_hash = hash_password(password)?;
        let challenge_hash = token_hash(challenge);
        let now = Utc::now();
        let mut state = self.runtime.state.write();
        let challenge_record = state
            .activation_challenges
            .get(&challenge_hash)
            .filter(|record| record.consumed_at.is_none() && record.expires_at > now)
            .cloned()
            .ok_or_else(activation_error)?;
        let invitation = state
            .invitations
            .get(&challenge_record.invitation_id)
            .filter(|invitation| {
                invitation.consumed_at.is_none()
                    && invitation.revoked_at.is_none()
                    && invitation.expires_at > now
                    && invitation.attempt_count <= ACTIVATION_MAX_ATTEMPTS
            })
            .cloned()
            .ok_or_else(activation_error)?;
        if state.accounts.contains_key(&invitation.canonical_login) {
            return Err(activation_error());
        }
        for team_binding in &invitation.intended_bindings.teams {
            let (organization, team) = split_team_binding(team_binding)?;
            if !state
                .teams
                .contains_key(&(organization.to_string(), team.to_string()))
            {
                return Err(activation_error());
            }
        }
        if invitation.bootstrap_owner
            && (state.bootstrap_owner_consumed
                || state
                    .accounts
                    .values()
                    .any(|account| account.role == UserRole::Admin))
        {
            return Err(activation_error());
        }
        let previous = state.clone();
        let account = UserAccount {
            canonical_login: invitation.canonical_login.clone(),
            display_name: invitation.display_name.clone(),
            password_hash,
            role: invitation.intended_bindings.role.clone(),
            status: AccountStatus::PendingMfa,
            auth_epoch: 0,
            must_change_password: false,
            created_at: now,
            updated_at: now,
        };
        state
            .activation_challenges
            .get_mut(&challenge_hash)
            .expect("challenge was validated under the same lock")
            .consumed_at = Some(now);
        state
            .invitations
            .get_mut(&invitation.id)
            .expect("invitation was validated under the same lock")
            .consumed_at = Some(now);
        state
            .users
            .entry(account.canonical_login.clone())
            .or_insert_with(|| User {
                id: Uuid::new_v4(),
                login: account.canonical_login.clone(),
                name: Some(account.display_name.clone()),
                email: None,
                created_at: now,
            });
        for team_binding in &invitation.intended_bindings.teams {
            let (organization, team) = split_team_binding(team_binding)?;
            let members = &mut state
                .teams
                .get_mut(&(organization.to_string(), team.to_string()))
                .expect("team binding was validated under the same lock")
                .members;
            if !members.contains(&account.canonical_login) {
                members.push(account.canonical_login.clone());
                members.sort();
            }
        }
        if invitation.bootstrap_owner {
            state.bootstrap_owner_consumed = true;
        }
        state
            .accounts
            .insert(account.canonical_login.clone(), account.clone());
        self.persist_after_mutation(&mut state, previous)?;
        Ok(account.into())
    }

    pub fn disable_account(&self, login: &str) -> Result<AccountSummary> {
        self.transition_account_status(login, AccountStatus::Disabled, None)
    }

    pub fn lock_account(&self, login: &str) -> Result<AccountSummary> {
        self.transition_account_status(login, AccountStatus::Locked, None)
    }

    /// Reactivation deliberately returns to MFA enrollment, never directly active.
    pub fn reactivate_account(&self, login: &str) -> Result<AccountSummary> {
        self.transition_account_status(
            login,
            AccountStatus::PendingMfa,
            Some(&[AccountStatus::Disabled, AccountStatus::Locked]),
        )
    }

    /// H3 calls this only after a successful first MFA enrollment.
    pub fn mark_account_mfa_active(&self, login: &str) -> Result<AccountSummary> {
        self.transition_account_status(
            login,
            AccountStatus::Active,
            Some(&[AccountStatus::PendingMfa]),
        )
    }

    #[must_use]
    pub fn bootstrap_owner_consumed(&self) -> bool {
        self.runtime.state.read().bootstrap_owner_consumed
    }

    fn transition_account_status(
        &self,
        login: &str,
        status: AccountStatus,
        allowed_current: Option<&[AccountStatus]>,
    ) -> Result<AccountSummary> {
        let mut state = self.runtime.state.write();
        if !state.accounts.contains_key(login) {
            return Err(ForgeError::NotFound(format!("account {login}")));
        }
        let previous = state.clone();
        let account = state
            .accounts
            .get_mut(login)
            .expect("presence checked above");
        if let Some(allowed) = allowed_current
            && !allowed.contains(&account.status)
        {
            return Err(ForgeError::Validation(match status {
                AccountStatus::PendingMfa => {
                    "only disabled or locked accounts may be reactivated".to_string()
                }
                AccountStatus::Active => "account is not pending MFA enrollment".to_string(),
                _ => "account status transition is not allowed".to_string(),
            }));
        }
        if account.status == status {
            return Ok(account.clone().into());
        }
        account.status = status;
        account.auth_epoch = next_auth_epoch(account.auth_epoch)?;
        account.updated_at = Utc::now();
        let updated = account.clone();
        state.sessions.retain(|_, session| session.login != login);
        state
            .personal_tokens
            .retain(|_, token| token.login != login);
        self.persist_after_mutation(&mut state, previous)?;
        Ok(updated.into())
    }

    fn require_active_account(&self, login: &str) -> Result<AccountSummary> {
        let account = self.get_account(login)?;
        if !account.status.permits_authentication() {
            return Err(ForgeError::Validation("account is not active".to_string()));
        }
        Ok(account)
    }

    pub fn grant_repo_access(
        &self,
        actor: &str,
        login: &str,
        owner: &str,
        repo: &str,
        access: RepoAccessLevel,
    ) -> Result<RepoAccessGrant> {
        self.get_account(login)?;
        self.get_repository(owner, repo)?;
        let mut state = self.runtime.state.write();
        let previous = state.clone();
        let grant = RepoAccessGrant {
            login: login.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            access,
            granted_by: actor.to_string(),
            granted_at: Utc::now(),
        };
        state.repo_grants.insert(
            (login.to_string(), owner.to_string(), repo.to_string()),
            grant.clone(),
        );
        self.persist_after_mutation(&mut state, previous)?;
        Ok(grant)
    }

    pub fn grant_repo_access_checked(
        &self,
        actor: &str,
        login: &str,
        owner: &str,
        repo: &str,
        access: RepoAccessLevel,
    ) -> Result<RepoAccessGrant> {
        self.get_repository(owner, repo)?;
        if !self.user_can_admin_repo(actor, owner, repo) {
            return Err(ForgeError::BranchProtection(
                "repo admin access required".to_string(),
            ));
        }
        self.grant_repo_access(actor, login, owner, repo, access)
    }

    pub fn revoke_repo_access(&self, login: &str, owner: &str, repo: &str) -> Result<bool> {
        let mut state = self.runtime.state.write();
        let previous = state.clone();
        let removed = state
            .repo_grants
            .remove(&(login.to_string(), owner.to_string(), repo.to_string()))
            .is_some();
        self.persist_after_mutation(&mut state, previous)?;
        Ok(removed)
    }

    pub fn revoke_repo_access_checked(
        &self,
        actor: &str,
        login: &str,
        owner: &str,
        repo: &str,
    ) -> Result<bool> {
        self.get_repository(owner, repo)?;
        if !self.user_can_admin_repo(actor, owner, repo) {
            return Err(ForgeError::BranchProtection(
                "repo admin access required".to_string(),
            ));
        }
        self.revoke_repo_access(login, owner, repo)
    }

    pub fn list_repo_access(&self, owner: &str, repo: &str) -> Vec<RepoAccessGrant> {
        let mut grants: Vec<_> = self
            .runtime
            .state
            .read()
            .repo_grants
            .values()
            .filter(|grant| grant.owner == owner && grant.repo == repo)
            .cloned()
            .collect();
        grants.sort_by(|a, b| a.login.cmp(&b.login));
        grants
    }

    pub fn list_repo_access_checked(
        &self,
        actor: &str,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<RepoAccessGrant>> {
        self.get_repository(owner, repo)?;
        if !self.user_can_admin_repo(actor, owner, repo) {
            return Err(ForgeError::BranchProtection(
                "repo admin access required".to_string(),
            ));
        }
        Ok(self.list_repo_access(owner, repo))
    }

    pub fn repo_access_for(&self, login: &str, owner: &str, repo: &str) -> Option<RepoAccessLevel> {
        let state = self.runtime.state.read();
        let account = state.accounts.get(login)?;
        if !account.status.permits_authentication() {
            return None;
        }
        if account.role == UserRole::Admin {
            return Some(RepoAccessLevel::Admin);
        }
        state
            .repo_grants
            .get(&(login.to_string(), owner.to_string(), repo.to_string()))
            .map(|grant| grant.access)
    }

    pub fn user_can_read_repo(&self, login: &str, owner: &str, repo: &str) -> bool {
        self.repo_access_for(login, owner, repo)
            .is_some_and(RepoAccessLevel::allows_read)
    }

    pub fn user_can_write_repo(&self, login: &str, owner: &str, repo: &str) -> bool {
        self.repo_access_for(login, owner, repo)
            .is_some_and(RepoAccessLevel::allows_write)
    }

    pub fn user_can_admin_repo(&self, login: &str, owner: &str, repo: &str) -> bool {
        self.repo_access_for(login, owner, repo)
            .is_some_and(RepoAccessLevel::allows_admin)
    }
}

pub(super) fn require_login(login: &str) -> Result<()> {
    require_name("login", login)?;
    if login.trim() != login
        || !login.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        return Err(ForgeError::Validation(
            "login must be canonical lowercase ASCII using letters, numbers, hyphen, or underscore"
                .to_string(),
        ));
    }
    Ok(())
}
