//! Web account/session endpoints and auth middleware.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Json;
use axum::extract::{ConnectInfo, Extension, Path as AxumPath, State};
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response as AxumResponse};
use chrono::{DateTime, Duration, Utc};
use jeryu_core::{
    AccountStatus, AccountSummary, ActorCredential, AuthenticatedActor, ForgeError,
    PersonalAccessTokenSummary, RepoAccessGrant, RepoAccessLevel, UserRole,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::repositories::find_repo;
use super::{WebState, api_error};

const HOST_SESSION_COOKIE: &str = "__Host-jeryu-session";
const LOCAL_SESSION_COOKIE: &str = "jeryu-session";
const CSRF_HEADER: &str = "x-jeryu-csrf";
const AUTH_LIMIT_MAX: u32 = 10;
const AUTH_LIMIT_WINDOW_SECS: i64 = 60;
const REMEMBER_ME_MAX_AGE_SECS: i64 = 60 * 60 * 24 * 30;

#[derive(Debug, Deserialize)]
pub(super) struct SignupRequest {
    login: String,
    password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LoginRequest {
    login: String,
    password: String,
    #[serde(default)]
    remember_me: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateTokenRequest {
    name: Option<String>,
    #[serde(rename = "expiresAt")]
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PasswordChangeRequest {
    current_password: String,
    new_password: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GrantRequest {
    access: RepoAccessLevel,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AuthUserResponse {
    login: String,
    role: UserRole,
    must_change_password: bool,
    csrf_token: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TokenResponse {
    id: String,
    token: String,
    name: String,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TokenSummaryResponse {
    id: String,
    name: String,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub(super) struct PasswordResetResponse {
    login: String,
    password: String,
}

impl From<AccountSummary> for AuthUserResponse {
    fn from(account: AccountSummary) -> Self {
        Self::new(account, None)
    }
}

impl AuthUserResponse {
    fn new(account: AccountSummary, csrf_token: Option<String>) -> Self {
        Self {
            login: account.login,
            role: account.role,
            must_change_password: account.must_change_password,
            csrf_token,
        }
    }
}

impl From<PersonalAccessTokenSummary> for TokenSummaryResponse {
    fn from(token: PersonalAccessTokenSummary) -> Self {
        Self {
            id: token.id.to_string(),
            name: token.name,
            created_at: token.created_at,
            expires_at: token.expires_at,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RateLimitBucket {
    attempts: u32,
    reset_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthSource {
    Bearer,
    Session,
    LocalDev,
}

#[derive(Clone, Debug)]
pub(crate) struct HeaderAuth {
    pub(crate) account: AccountSummary,
    pub(crate) source: AuthSource,
}

pub(super) async fn signup(
    State(state): State<Arc<WebState>>,
    peer: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    Json(request): Json<SignupRequest>,
) -> AxumResponse {
    blocking_auth_response(move || {
        if auth_rate_limit_exceeded(&state, peer.as_ref(), &headers, "signup", &request.login) {
            return rate_limited();
        }
        let account =
            match state
                .core
                .create_account(&request.login, &request.password, UserRole::User)
            {
                Ok(account) => account,
                Err(ForgeError::Conflict(_)) => {
                    return api_error(
                        StatusCode::CONFLICT,
                        "conflict",
                        "an account with that login already exists",
                    );
                }
                Err(ForgeError::Validation(reason)) => {
                    return api_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_input", &reason);
                }
                Err(error) => {
                    return api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "storage_failed",
                        &format!("could not create account: {error}"),
                    );
                }
            };
        let session = match state.core.create_session(&account.login, &request.password) {
            Ok(session) => session,
            Err(error) => return authenticated_actor_error(error),
        };
        let csrf_token = session.session.csrf_token.clone();
        let mut response = Json(AuthUserResponse::new(account, Some(csrf_token))).into_response();
        if let Ok(value) = cookie_header(&state, &session.token, None) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
        response
    })
    .await
}

pub(super) async fn login(
    State(state): State<Arc<WebState>>,
    peer: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> AxumResponse {
    blocking_auth_response(move || {
        if auth_rate_limit_exceeded(&state, peer.as_ref(), &headers, "login", &request.login) {
            return rate_limited();
        }
        let session = match if request.remember_me {
            state.core.create_session_with_ttl(
                &request.login,
                &request.password,
                Duration::seconds(REMEMBER_ME_MAX_AGE_SECS),
            )
        } else {
            state.core.create_session(&request.login, &request.password)
        } {
            Ok(session) => session,
            Err(ForgeError::Unauthenticated(_) | ForgeError::Validation(_)) => {
                return api_error(
                    StatusCode::UNAUTHORIZED,
                    "unauthorized",
                    "invalid login or password",
                );
            }
            Err(error) => return authenticated_actor_error(error),
        };
        let Some(account) = state.core.authenticate_session(&session.token) else {
            return api_error(
                StatusCode::UNAUTHORIZED,
                "session_revoked",
                "the account or session changed during sign in; sign in again",
            );
        };
        let csrf_token = session.session.csrf_token.clone();
        let mut response = Json(AuthUserResponse::new(account, Some(csrf_token))).into_response();
        let max_age = request.remember_me.then_some(REMEMBER_ME_MAX_AGE_SECS);
        if let Ok(value) = cookie_header(&state, &session.token, max_age) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
        response
    })
    .await
}

pub(super) async fn logout(State(state): State<Arc<WebState>>, headers: HeaderMap) -> AxumResponse {
    blocking_auth_response(move || {
        if let Some(token) = session_token_from_headers(&headers)
            && let Err(error) = state.core.revoke_session(&token)
        {
            return authenticated_actor_error(error);
        }
        let mut response = Json(json!({ "ok": true })).into_response();
        if let Ok(value) = expired_cookie_header(&state) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
        response
    })
    .await
}

pub(super) async fn me(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    headers: HeaderMap,
) -> AxumResponse {
    let csrf_token = session_token_from_headers(&headers).and_then(|token| {
        state
            .core
            .session_for_token(&token)
            .map(|(_, session)| session.csrf_token)
    });
    Json(AuthUserResponse::new(account, csrf_token)).into_response()
}

pub(super) async fn change_password(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    Json(request): Json<PasswordChangeRequest>,
) -> AxumResponse {
    blocking_auth_response(move || {
        match state.core.change_account_password(
            &account.login,
            &request.current_password,
            &request.new_password,
        ) {
            Ok(updated) => match state
                .core
                .create_session(&updated.login, &request.new_password)
            {
                Ok(session) => {
                    let csrf_token = session.session.csrf_token.clone();
                    let mut response =
                        Json(AuthUserResponse::new(updated, Some(csrf_token))).into_response();
                    match cookie_header(&state, &session.token, None) {
                        Ok(value) => {
                            response.headers_mut().append(header::SET_COOKIE, value);
                            response
                        }
                        Err(error) => api_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "session_rotation_failed",
                            &format!("could not publish the rotated session: {error}"),
                        ),
                    }
                }
                Err(error) => authenticated_actor_error(error),
            },
            Err(ForgeError::Validation(_)) => api_error(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "invalid current password",
            ),
            Err(error) => api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_failed",
                &format!("could not change password: {error}"),
            ),
        }
    })
    .await
}

pub(super) async fn create_token(
    State(state): State<Arc<WebState>>,
    headers: HeaderMap,
    Json(request): Json<CreateTokenRequest>,
) -> AxumResponse {
    blocking_auth_response(move || {
        let actor = match authenticated_actor(&state, &headers) {
            Ok(actor) => actor,
            Err(error) => return authenticated_actor_error(error),
        };
        let name = request.name.unwrap_or_else(|| "web token".to_string());
        match state
            .core
            .create_personal_access_token(&actor, &name, request.expires_at)
        {
            Ok(receipt) => Json(TokenResponse {
                id: receipt.token.id.to_string(),
                token: receipt.secret,
                name: receipt.token.name,
                expires_at: receipt.token.expires_at,
            })
            .into_response(),
            Err(ForgeError::Validation(reason)) => {
                api_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_input", &reason)
            }
            Err(error) => authenticated_actor_error(error),
        }
    })
    .await
}

/// Credential changes can wait for a review's authority guard or perform
/// password hashing. Keep those complete operations off the async workers.
async fn blocking_auth_response(
    operation: impl FnOnce() -> AxumResponse + Send + 'static,
) -> AxumResponse {
    match tokio::task::spawn_blocking(operation).await {
        Ok(response) => response,
        Err(_) => authenticated_actor_error(ForgeError::WriterUnavailable(
            "credential operation worker is unavailable; refresh account state before retrying"
                .into(),
        )),
    }
}

/// Privileged mutations require actual credential custody, including in local
/// development. Middleware account summaries are display data, not credentials.
pub(super) fn authenticated_actor(
    state: &WebState,
    headers: &HeaderMap,
) -> jeryu_core::Result<AuthenticatedActor> {
    if let Some(authorization) = headers.get(header::AUTHORIZATION) {
        let token = authorization
            .to_str()
            .ok()
            .and_then(jeryu_gitd::auth::extract_bearer_or_basic)
            .ok_or_else(|| ForgeError::Unauthenticated("invalid authorization header".into()))?;
        return state
            .core
            .authenticate_actor(ActorCredential::PersonalAccessToken(&token));
    }
    let token = session_token_from_headers(headers)
        .ok_or_else(|| ForgeError::Unauthenticated("authenticated credential required".into()))?;
    let csrf_token = headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ForgeError::Forbidden("session mutation requires a CSRF token".into()))?;
    state.core.authenticate_actor(ActorCredential::Session {
        token: &token,
        csrf_token,
    })
}

pub(super) fn authenticated_read_actor(
    state: &WebState,
    headers: &HeaderMap,
) -> jeryu_core::Result<AuthenticatedActor> {
    if headers.contains_key(header::AUTHORIZATION) {
        return authenticated_actor(state, headers);
    }
    let token = session_token_from_headers(headers)
        .ok_or_else(|| ForgeError::Unauthenticated("authenticated credential required".into()))?;
    state
        .core
        .authenticate_actor(ActorCredential::SessionReadOnly(&token))
}

pub(super) fn authenticated_actor_error(error: ForgeError) -> AxumResponse {
    let (status, code) = match &error {
        ForgeError::Unauthenticated(_) => (StatusCode::UNAUTHORIZED, "unauthorized"),
        ForgeError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
        ForgeError::WriterUnavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "writer_unavailable"),
        ForgeError::PreconditionRequired(_) => {
            (StatusCode::PRECONDITION_REQUIRED, "precondition_required")
        }
        ForgeError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
        ForgeError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_input"),
        ForgeError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
        ForgeError::BranchProtection(_) => (StatusCode::FORBIDDEN, "branch_protection"),
        ForgeError::Storage(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage_failed"),
    };
    super::repositories::api_error_with_hint(
        status,
        code,
        &error.to_string(),
        super::repositories::ApiErrorHint {
            purpose: "complete authenticated Core operation",
            reason: code,
            common_fixes: &[
                "use a current credential with access to this repository",
                "refresh the requested state and satisfy the reported precondition",
                "if the writer is unavailable, read durable history before retrying",
            ],
            docs_url: "docs/boundaries.md#authenticated-review-boundary",
            repair_hint: "resolve the reported authentication, precondition or writer error before retrying",
        },
    )
}

pub(super) async fn list_tokens(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
) -> AxumResponse {
    match state.core.list_personal_access_tokens(&account.login) {
        Ok(tokens) => Json(
            tokens
                .into_iter()
                .map(TokenSummaryResponse::from)
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_failed",
            &format!("could not list tokens: {error}"),
        ),
    }
}

pub(super) async fn delete_token(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    blocking_auth_response(move || {
        let id = match Uuid::parse_str(&id) {
            Ok(id) => id,
            Err(_) => {
                return api_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_input",
                    "invalid token id",
                );
            }
        };
        match state.core.revoke_personal_access_token(&account.login, id) {
            Ok(true) => (StatusCode::NO_CONTENT, "").into_response(),
            Ok(false) => api_error(StatusCode::NOT_FOUND, "not_found", "token not found"),
            Err(error) => api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_failed",
                &format!("could not delete token: {error}"),
            ),
        }
    })
    .await
}

pub(super) async fn admin_users(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
) -> AxumResponse {
    if account.role != UserRole::Admin {
        return forbidden("admin role required");
    }
    Json(state.core.list_accounts()).into_response()
}

pub(super) async fn admin_reset_password(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    peer: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    AxumPath(login): AxumPath<String>,
) -> AxumResponse {
    blocking_auth_response(move || {
        if account.role != UserRole::Admin {
            return forbidden("admin role required");
        }
        if auth_rate_limit_exceeded(&state, peer.as_ref(), &headers, "reset", &login) {
            return rate_limited();
        }
        let password = match state.core.generate_one_time_password() {
            Ok(password) => password,
            Err(error) => {
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_failed",
                    &format!("could not generate password: {error}"),
                );
            }
        };
        match state.core.reset_account_password(&login, &password) {
            Ok(_) => Json(PasswordResetResponse { login, password }).into_response(),
            Err(ForgeError::NotFound(_)) => {
                api_error(StatusCode::NOT_FOUND, "not_found", "user not found")
            }
            Err(error) => api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_failed",
                &format!("could not reset password: {error}"),
            ),
        }
    })
    .await
}

pub(super) async fn admin_repo_grants(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath((owner, repo)): AxumPath<(String, String)>,
) -> AxumResponse {
    match state
        .core
        .list_repo_access_checked(&account.login, &owner, &repo)
    {
        Ok(grants) => Json(grants).into_response(),
        Err(ForgeError::BranchProtection(_)) => forbidden("repo admin access required"),
        Err(ForgeError::NotFound(_)) => {
            api_error(StatusCode::NOT_FOUND, "not_found", "repository not found")
        }
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_failed",
            &format!("could not list access: {error}"),
        ),
    }
}

pub(super) async fn admin_grant_repo(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath((owner, repo, login)): AxumPath<(String, String, String)>,
    Json(request): Json<GrantRequest>,
) -> AxumResponse {
    blocking_auth_response(move || {
        match state.core.grant_repo_access_checked(
            &account.login,
            &login,
            &owner,
            &repo,
            request.access,
        ) {
            Ok(grant) => Json(grant).into_response(),
            Err(ForgeError::BranchProtection(_)) => forbidden("repo admin access required"),
            Err(ForgeError::NotFound(_)) => api_error(
                StatusCode::NOT_FOUND,
                "not_found",
                "user or repository not found",
            ),
            Err(error) => api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_failed",
                &format!("could not grant access: {error}"),
            ),
        }
    })
    .await
}

pub(super) async fn admin_revoke_repo(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath((owner, repo, login)): AxumPath<(String, String, String)>,
) -> AxumResponse {
    blocking_auth_response(move || {
        match state
            .core
            .revoke_repo_access_checked(&account.login, &login, &owner, &repo)
        {
            Ok(_) => Json(json!({ "ok": true })).into_response(),
            Err(ForgeError::BranchProtection(_)) => forbidden("repo admin access required"),
            Err(error) => api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_failed",
                &format!("could not revoke access: {error}"),
            ),
        }
    })
    .await
}

pub(super) async fn gate(
    State(state): State<Arc<WebState>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> AxumResponse {
    if !auth_applies(request.uri().path()) {
        return next.run(request).await;
    }
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect| connect.0);
    let auth = if !state.auth_required || local_dev_trusted(&state, peer) {
        HeaderAuth {
            account: trusted_local_account(&state),
            source: AuthSource::LocalDev,
        }
    } else {
        match authenticate_headers(&state, request.headers()) {
            Some(auth) => auth,
            None => {
                return api_error(StatusCode::UNAUTHORIZED, "unauthorized", "login required");
            }
        }
    };
    let account = auth.account.clone();

    if account.must_change_password && !password_change_allowed_path(request.uri().path()) {
        return api_error(
            StatusCode::FORBIDDEN,
            "password_change_required",
            "password change required before continuing",
        );
    }

    if unsafe_method(request.method())
        && auth.source == AuthSource::Session
        && !csrf_valid(&state, request.headers())
    {
        return api_error(
            StatusCode::FORBIDDEN,
            "csrf_required",
            "missing or invalid CSRF token",
        );
    }

    if admin_only_path(request.uri().path()) && account.role != UserRole::Admin {
        return forbidden("admin role required");
    }

    if let Some(repo_id) = repo_id_from_path(request.uri().path())
        && let Some(repo) = find_repo(&state, &repo_id)
    {
        let allowed = match *request.method() {
            Method::GET | Method::HEAD => {
                account.role == UserRole::Admin
                    || state
                        .core
                        .user_can_read_repo(&account.login, &repo.owner, &repo.name)
            }
            Method::DELETE => {
                account.role == UserRole::Admin
                    || state
                        .core
                        .user_can_admin_repo(&account.login, &repo.owner, &repo.name)
            }
            _ => {
                account.role == UserRole::Admin
                    || state
                        .core
                        .user_can_write_repo(&account.login, &repo.owner, &repo.name)
            }
        };
        if !allowed {
            return forbidden("repository access denied");
        }
    }

    request.extensions_mut().insert(account);
    next.run(request).await
}

pub(crate) fn authenticate_headers(state: &WebState, headers: &HeaderMap) -> Option<HeaderAuth> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(jeryu_gitd::auth::extract_bearer_or_basic);
    if let Some(bearer) = bearer
        && let Some(account) = state.core.authenticate_personal_access_token(&bearer)
    {
        return Some(HeaderAuth {
            account,
            source: AuthSource::Bearer,
        });
    }
    if let Some(token) = session_token_from_headers(headers)
        && let Some(account) = state.core.authenticate_session(&token)
    {
        return Some(HeaderAuth {
            account,
            source: AuthSource::Session,
        });
    }
    None
}

pub(crate) fn trusted_local_account(state: &WebState) -> AccountSummary {
    match state.core.get_account("jeryu-admin") {
        Ok(account) => account,
        Err(_) => AccountSummary {
            login: "jeryu-admin".to_string(),
            display_name: "Jeryu Admin".to_string(),
            role: UserRole::Admin,
            status: AccountStatus::Active,
            auth_epoch: 0,
            must_change_password: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    }
}

/// External CI-evidence maintenance is an administrative boundary. Native
/// runners report through runner-only APIs; the server publishes their checks
/// internally instead of handing a repository writer a check-minting route.
pub(crate) fn can_publish_external_ci_evidence(account: &AccountSummary) -> bool {
    account.role == UserRole::Admin
}

pub(super) fn forbidden(message: &str) -> AxumResponse {
    api_error(StatusCode::FORBIDDEN, "permission_denied", message)
}

fn auth_applies(path: &str) -> bool {
    path == "/mcp"
        || path.starts_with("/mcp/")
        || (path.starts_with("/api/v1/")
            && !matches!(path, "/api/v1/auth/signup" | "/api/v1/auth/login"))
}

fn password_change_allowed_path(path: &str) -> bool {
    matches!(
        path,
        "/api/v1/auth/me" | "/api/v1/auth/logout" | "/api/v1/auth/password"
    )
}

pub(crate) fn local_dev_trusted(state: &WebState, peer: Option<SocketAddr>) -> bool {
    state.trust_local_dev && peer.is_some_and(|peer| peer.ip().is_loopback())
}

fn unsafe_method(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn csrf_valid(state: &WebState, headers: &HeaderMap) -> bool {
    let Some(session_token) = session_token_from_headers(headers) else {
        return true;
    };
    let Some(csrf_token) = headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    state.core.session_csrf_matches(&session_token, csrf_token)
}

fn admin_only_path(path: &str) -> bool {
    path.starts_with("/api/v1/admin/")
        || path.starts_with("/api/v1/control-plane/")
        || path.starts_with("/api/v1/workcells")
        || path.starts_with("/api/v1/agent-runs")
        || path.starts_with("/api/v1/fleet/")
        || path.starts_with("/api/v1/codegraph/tool-build/")
        || path.starts_with("/api/v1/tool-finder/")
}

fn auth_rate_limit_exceeded(
    state: &WebState,
    peer: Option<&ConnectInfo<SocketAddr>>,
    headers: &HeaderMap,
    action: &str,
    login: &str,
) -> bool {
    let ip = client_ip(peer.map(|connect| connect.0), headers);
    let login = login.trim().to_ascii_lowercase();
    let key = format!("{action}:{ip}:{login}");
    let now = Utc::now();
    let mut limits = state
        .auth_rate_limits
        .lock()
        .expect("auth rate-limit mutex poisoned");
    let bucket = limits.entry(key).or_insert_with(|| RateLimitBucket {
        attempts: 0,
        reset_at: now + chrono::Duration::seconds(AUTH_LIMIT_WINDOW_SECS),
    });
    if bucket.reset_at <= now {
        bucket.attempts = 0;
        bucket.reset_at = now + chrono::Duration::seconds(AUTH_LIMIT_WINDOW_SECS);
    }
    bucket.attempts = bucket.attempts.saturating_add(1);
    bucket.attempts > AUTH_LIMIT_MAX
}

fn rate_limited() -> AxumResponse {
    api_error(
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        "too many authentication attempts",
    )
}

fn client_ip(peer: Option<SocketAddr>, headers: &HeaderMap) -> IpAddr {
    let peer_ip = peer
        .map(|addr| addr.ip())
        .unwrap_or(IpAddr::from([0, 0, 0, 0]));
    if !trusted_proxy_ips().contains(&peer_ip) {
        return peer_ip;
    }
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .and_then(|value| value.trim().parse::<IpAddr>().ok())
        .unwrap_or(peer_ip)
}

fn trusted_proxy_ips() -> Vec<IpAddr> {
    std::env::var("JERYU_TRUSTED_PROXIES")
        .ok()
        .into_iter()
        .flat_map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .filter_map(|part| part.parse::<IpAddr>().ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn repo_id_from_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/v1/repos/")?;
    let id = rest.split('/').next()?;
    if id.is_empty() {
        None
    } else {
        Some(percent_decode(id))
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        if let Some((name, value)) = part.trim().split_once('=')
            && (name == HOST_SESSION_COOKIE || name == LOCAL_SESSION_COOKIE)
        {
            return Some(value.to_string());
        }
    }
    None
}

fn cookie_header(
    state: &WebState,
    token: &str,
    max_age_secs: Option<i64>,
) -> Result<axum::http::HeaderValue, axum::http::header::InvalidHeaderValue> {
    let name = if state.secure_cookies {
        HOST_SESSION_COOKIE
    } else {
        LOCAL_SESSION_COOKIE
    };
    let secure = if state.secure_cookies { "; Secure" } else { "" };
    let max_age = max_age_secs
        .map(|seconds| format!("; Max-Age={seconds}"))
        .unwrap_or_default();
    axum::http::HeaderValue::from_str(&format!(
        "{name}={token}; Path=/; HttpOnly; SameSite=Lax{max_age}{secure}"
    ))
}

fn expired_cookie_header(
    state: &WebState,
) -> Result<axum::http::HeaderValue, axum::http::header::InvalidHeaderValue> {
    let name = if state.secure_cookies {
        HOST_SESSION_COOKIE
    } else {
        LOCAL_SESSION_COOKIE
    };
    let secure = if state.secure_cookies { "; Secure" } else { "" };
    axum::http::HeaderValue::from_str(&format!(
        "{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}"
    ))
}

#[allow(dead_code)]
fn _grant_wire(_grant: &RepoAccessGrant) {}
