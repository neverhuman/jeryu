//! Durable account, session, token, and repository-grant models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    User,
}

/// Lifecycle state enforced at every credential boundary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    PendingActivation,
    PendingMfa,
    Active,
    Disabled,
    Locked,
}

impl AccountStatus {
    #[must_use]
    pub fn permits_authentication(self) -> bool {
        self == Self::Active
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserAccount {
    pub canonical_login: String,
    pub display_name: String,
    pub password_hash: String,
    pub role: UserRole,
    pub status: AccountStatus,
    pub auth_epoch: u64,
    #[serde(default)]
    pub must_change_password: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountSummary {
    pub login: String,
    pub display_name: String,
    pub role: UserRole,
    pub status: AccountStatus,
    pub auth_epoch: u64,
    pub must_change_password: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<UserAccount> for AccountSummary {
    fn from(account: UserAccount) -> Self {
        Self {
            login: account.canonical_login,
            display_name: account.display_name,
            role: account.role,
            status: account.status,
            auth_epoch: account.auth_epoch,
            must_change_password: account.must_change_password,
            created_at: account.created_at,
            updated_at: account.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebSession {
    pub id: Uuid,
    pub login: String,
    pub auth_epoch: u64,
    pub token_hash: String,
    pub csrf_token: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReceipt {
    pub session: WebSession,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersonalAccessToken {
    pub id: Uuid,
    pub login: String,
    pub auth_epoch: u64,
    pub name: String,
    pub token_hash: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersonalAccessTokenSummary {
    pub id: Uuid,
    pub login: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl From<PersonalAccessToken> for PersonalAccessTokenSummary {
    fn from(token: PersonalAccessToken) -> Self {
        Self {
            id: token.id,
            login: token.login,
            name: token.name,
            created_at: token.created_at,
            expires_at: token.expires_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonalAccessTokenReceipt {
    pub token: PersonalAccessToken,
    pub secret: String,
}

/// Requested bindings carried by an invitation and applied after activation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvitationBindings {
    pub role: UserRole,
    #[serde(default)]
    pub teams: Vec<String>,
}

/// Non-secret invitation data safe to return from administrative listings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountInvitationSummary {
    pub id: Uuid,
    pub canonical_login: String,
    pub display_name: String,
    pub issuer_principal: String,
    pub intended_bindings: InvitationBindings,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub attempt_count: u32,
}

/// One-time creation receipt. The activation secret is never stored verbatim.
#[derive(Clone, PartialEq, Eq)]
pub struct AccountInvitationReceipt {
    pub invitation: AccountInvitationSummary,
    pub activation_secret: String,
}

impl std::fmt::Debug for AccountInvitationReceipt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AccountInvitationReceipt")
            .field("invitation", &self.invitation)
            .field("activation_secret", &"[REDACTED]")
            .finish()
    }
}

/// One-time activation-start receipt. The challenge is stored only by hash.
#[derive(Clone, PartialEq, Eq)]
pub struct ActivationStartReceipt {
    pub challenge: String,
    pub expires_at: DateTime<Utc>,
}

impl std::fmt::Debug for ActivationStartReceipt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ActivationStartReceipt")
            .field("challenge", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AccountInvitation {
    pub id: Uuid,
    pub canonical_login: String,
    pub display_name: String,
    pub activation_secret_hash: String,
    pub issuer_principal: String,
    pub intended_bindings: InvitationBindings,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub attempt_count: u32,
    pub bootstrap_owner: bool,
}

impl From<AccountInvitation> for AccountInvitationSummary {
    fn from(invitation: AccountInvitation) -> Self {
        Self {
            id: invitation.id,
            canonical_login: invitation.canonical_login,
            display_name: invitation.display_name,
            issuer_principal: invitation.issuer_principal,
            intended_bindings: invitation.intended_bindings,
            created_at: invitation.created_at,
            expires_at: invitation.expires_at,
            consumed_at: invitation.consumed_at,
            revoked_at: invitation.revoked_at,
            attempt_count: invitation.attempt_count,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ActivationChallenge {
    pub id: Uuid,
    pub invitation_id: Uuid,
    pub challenge_hash: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum RepoAccessLevel {
    Read,
    Write,
    Admin,
}

impl RepoAccessLevel {
    #[must_use]
    pub fn allows_read(self) -> bool {
        matches!(self, Self::Read | Self::Write | Self::Admin)
    }

    #[must_use]
    pub fn allows_write(self) -> bool {
        matches!(self, Self::Write | Self::Admin)
    }

    #[must_use]
    pub fn allows_admin(self) -> bool {
        self == Self::Admin
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoAccessGrant {
    pub login: String,
    pub owner: String,
    pub repo: String,
    pub access: RepoAccessLevel,
    pub granted_by: String,
    pub granted_at: DateTime<Utc>,
}
