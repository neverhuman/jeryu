//! Git smart-HTTP transport handlers for the unified `jeryu serve`.
//!
//! These axum handlers mount `git clone`/`push` over HTTP by delegating to the
//! pure `jeryu_gitd::smart_http::SmartHttpServer`, run on the blocking pool so
//! the `git` subprocess never stalls an async worker. Kept in their own module
//! to keep `web.rs` focused on the REST/WS edge.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{ConnectInfo, Path as AxumPath, Query as AxumQuery, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response as AxumResponse};
use flate2::bufread::GzDecoder;
use futures_util::StreamExt;
use jeryu_gitd::lfs::{LfsStore, normalize_oid};
use jeryu_gitd::smart_http::{
    HttpRequest as GitHttpRequest, HttpResponse as GitHttpResponse, SmartHttpServer,
};
use jeryu_gitd::{GitdError, RepoManager};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::web::WebState;

// Bound both wire bytes and logical request bytes. Compressed RPCs get a tighter
// wire cap so a small authenticated request cannot become a decompression bomb;
// identity RPCs use the decoded cap because their wire and logical bytes are the
// same. This keeps equivalent identity and gzip requests on the same payload
// ceiling without buffering either form without bound.
const MAX_GIT_UPLOAD_PACK_ENCODED_BYTES: usize = 4 * 1024 * 1024;
const MAX_GIT_UPLOAD_PACK_DECODED_BYTES: usize = 16 * 1024 * 1024;
const MAX_GIT_RECEIVE_PACK_ENCODED_BYTES: usize = 64 * 1024 * 1024;
const MAX_GIT_RECEIVE_PACK_DECODED_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GitRpcContentEncoding {
    Identity,
    Gzip,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GitRpcBodyError {
    UnsupportedEncoding,
    MalformedGzip,
    TooLarge,
    DecodeTaskFailed,
}

fn forwarded_git_headers(headers: &HeaderMap) -> HashMap<String, String> {
    let mut forwarded = HashMap::new();
    for name in [
        header::HOST,
        header::AUTHORIZATION,
        HeaderName::from_static("git-protocol"),
        HeaderName::from_static("x-forwarded-proto"),
    ] {
        if let Some(value) = headers.get(&name).and_then(|value| value.to_str().ok()) {
            forwarded.insert(name.as_str().to_ascii_lowercase(), value.to_string());
        }
    }
    forwarded
}

fn git_rpc_content_encoding(headers: &HeaderMap) -> Result<GitRpcContentEncoding, GitRpcBodyError> {
    let values = headers
        .get_all(header::CONTENT_ENCODING)
        .iter()
        .collect::<Vec<_>>();
    if values.is_empty() {
        return Ok(GitRpcContentEncoding::Identity);
    }
    if values.len() != 1 {
        return Err(GitRpcBodyError::UnsupportedEncoding);
    }
    let value = values[0]
        .to_str()
        .map_err(|_| GitRpcBodyError::UnsupportedEncoding)?
        .trim();
    if value.eq_ignore_ascii_case("identity") {
        Ok(GitRpcContentEncoding::Identity)
    } else if value.eq_ignore_ascii_case("gzip") || value.eq_ignore_ascii_case("x-gzip") {
        Ok(GitRpcContentEncoding::Gzip)
    } else {
        // This also rejects empty, comma-separated, and stacked encodings.
        Err(GitRpcBodyError::UnsupportedEncoding)
    }
}

fn decode_git_rpc_body(
    encoding: GitRpcContentEncoding,
    encoded: Bytes,
    max_decoded_bytes: usize,
) -> Result<Vec<u8>, GitRpcBodyError> {
    if encoding == GitRpcContentEncoding::Identity {
        return Ok(encoded.to_vec());
    }

    let encoded_len = encoded.len() as u64;
    let mut decoder = GzDecoder::new(Cursor::new(encoded.as_ref()));
    let mut decoded = Vec::new();
    (&mut decoder)
        .take(max_decoded_bytes.saturating_add(1) as u64)
        .read_to_end(&mut decoded)
        .map_err(|_| GitRpcBodyError::MalformedGzip)?;
    if decoded.len() > max_decoded_bytes {
        return Err(GitRpcBodyError::TooLarge);
    }
    if decoder.into_inner().position() != encoded_len {
        return Err(GitRpcBodyError::MalformedGzip);
    }
    Ok(decoded)
}

async fn read_git_rpc_body(
    headers: &HeaderMap,
    body: Body,
    max_encoded_bytes: usize,
    max_decoded_bytes: usize,
) -> Result<Vec<u8>, GitRpcBodyError> {
    let encoding = git_rpc_content_encoding(headers)?;
    let max_wire_bytes = match encoding {
        GitRpcContentEncoding::Identity => max_decoded_bytes,
        GitRpcContentEncoding::Gzip => max_encoded_bytes,
    };
    let encoded = to_bytes(body, max_wire_bytes)
        .await
        .map_err(|_| GitRpcBodyError::TooLarge)?;
    if encoding == GitRpcContentEncoding::Identity {
        return Ok(encoded.to_vec());
    }
    tokio::task::spawn_blocking(move || decode_git_rpc_body(encoding, encoded, max_decoded_bytes))
        .await
        .map_err(|_| GitRpcBodyError::DecodeTaskFailed)?
}

fn git_rpc_body_error_response(error: GitRpcBodyError) -> AxumResponse {
    match error {
        GitRpcBodyError::UnsupportedEncoding => (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported Git RPC Content-Encoding\n",
        )
            .into_response(),
        GitRpcBodyError::MalformedGzip => {
            (StatusCode::BAD_REQUEST, "malformed gzip Git RPC body\n").into_response()
        }
        GitRpcBodyError::TooLarge => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "Git RPC body exceeds the configured limit\n",
        )
            .into_response(),
        GitRpcBodyError::DecodeTaskFailed => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Git RPC body decoder failed\n",
        )
            .into_response(),
    }
}

async fn route_git(
    state: &WebState,
    peer: SocketAddr,
    method: &str,
    path: String,
    query: HashMap<String, String>,
    headers: &HeaderMap,
    body: Vec<u8>,
) -> AxumResponse {
    let request = GitHttpRequest {
        method: method.to_string(),
        path,
        query,
        headers: forwarded_git_headers(headers),
        body,
        is_loopback: peer.ip().is_loopback(),
        auth_prechecked: true,
    };
    // The gitd router shells out to `git` (blocking) and does file IO; run it on
    // the blocking pool so it never stalls an async worker.
    let manager = (*state.repo_manager).clone();
    let response =
        tokio::task::spawn_blocking(move || SmartHttpServer::new(manager).route(request))
            .await
            .expect("git smart-http task panicked");
    gitd_to_axum_response(&response)
}

/// Convert a gitd [`GitHttpResponse`] into an axum response, preserving status,
/// content type, and any extra headers (e.g. `WWW-Authenticate`).
fn gitd_to_axum_response(response: &GitHttpResponse) -> AxumResponse {
    let status =
        StatusCode::from_u16(response.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut out = (status, response.body().to_vec()).into_response();
    let map = out.headers_mut();
    if let Ok(content_type) = HeaderValue::from_str(response.content_type()) {
        map.insert(header::CONTENT_TYPE, content_type);
    }
    for (name, value) in response.extra_headers() {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            map.insert(name, value);
        }
    }
    out
}

pub(crate) async fn git_info_refs(
    State(state): State<Arc<WebState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    AxumPath((owner, repo)): AxumPath<(String, String)>,
    AxumQuery(query): AxumQuery<HashMap<String, String>>,
    headers: HeaderMap,
) -> AxumResponse {
    let write = matches!(
        query.get("service").map(String::as_str),
        Some("git-receive-pack")
    );
    if let Err(response) = authorize_git_core(&state, peer, &headers, &owner, &repo, write) {
        return *response;
    }
    route_git(
        &state,
        peer,
        "GET",
        format!("/git/{owner}/{repo}/info/refs"),
        query,
        &headers,
        Vec::new(),
    )
    .await
}

pub(crate) async fn git_upload_pack(
    State(state): State<Arc<WebState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    AxumPath((owner, repo)): AxumPath<(String, String)>,
    request: axum::extract::Request,
) -> AxumResponse {
    let headers = request.headers().clone();
    if let Err(response) = authorize_git_core(&state, peer, &headers, &owner, &repo, false) {
        return *response;
    }
    let body = match read_git_rpc_body(
        &headers,
        request.into_body(),
        MAX_GIT_UPLOAD_PACK_ENCODED_BYTES,
        MAX_GIT_UPLOAD_PACK_DECODED_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(error) => return git_rpc_body_error_response(error),
    };
    route_git(
        &state,
        peer,
        "POST",
        format!("/git/{owner}/{repo}/git-upload-pack"),
        HashMap::new(),
        &headers,
        body,
    )
    .await
}

pub(crate) async fn git_receive_pack(
    State(state): State<Arc<WebState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    AxumPath((owner, repo)): AxumPath<(String, String)>,
    request: axum::extract::Request,
) -> AxumResponse {
    let headers = request.headers().clone();
    if let Err(response) = authorize_git_core(&state, peer, &headers, &owner, &repo, true) {
        return *response;
    }
    let body = match read_git_rpc_body(
        &headers,
        request.into_body(),
        MAX_GIT_RECEIVE_PACK_ENCODED_BYTES,
        MAX_GIT_RECEIVE_PACK_DECODED_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(error) => return git_rpc_body_error_response(error),
    };
    let manager = (*state.repo_manager).clone();
    let before = snapshot_refs(&manager, &owner, &repo);
    let origin_base_url = origin_base_url(&headers);
    let response = route_git(
        &state,
        peer,
        "POST",
        format!("/git/{owner}/{repo}/git-receive-pack"),
        HashMap::new(),
        &headers,
        body,
    )
    .await;
    // After a successful push, fire the push->CI bridge for any moved branch.
    if response.status().is_success() {
        let core = state.core.clone();
        let owner = owner.clone();
        let repo = repo.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let after = snapshot_refs(&manager, &owner, &repo);
            let updates = crate::ci_bridge::ref_updates(&before, &after);
            crate::ci_bridge::on_push(&core, &manager, &owner, &repo, &updates, &origin_base_url);
        })
        .await;
    }
    response
}

pub(crate) async fn git_lfs_batch(
    State(state): State<Arc<WebState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    AxumPath((owner, repo)): AxumPath<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> AxumResponse {
    if let Err(response) = authorize_git_core(&state, peer, &headers, &owner, &repo, true) {
        return *response;
    }
    route_git(
        &state,
        peer,
        "POST",
        format!("/git/{owner}/{repo}/info/lfs/objects/batch"),
        HashMap::new(),
        &headers,
        body.to_vec(),
    )
    .await
}

pub(crate) async fn git_lfs_download(
    State(state): State<Arc<WebState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    AxumPath((owner, repo, oid)): AxumPath<(String, String, String)>,
    headers: HeaderMap,
) -> AxumResponse {
    if let Err(response) = authorize_git_core(&state, peer, &headers, &owner, &repo, false) {
        return *response;
    }
    route_git(
        &state,
        peer,
        "GET",
        format!("/git/{owner}/{repo}/info/lfs/objects/{oid}"),
        HashMap::new(),
        &headers,
        Vec::new(),
    )
    .await
}

pub(crate) async fn git_lfs_verify(
    State(state): State<Arc<WebState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    AxumPath((owner, repo, oid)): AxumPath<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> AxumResponse {
    if let Err(response) = authorize_git_core(&state, peer, &headers, &owner, &repo, true) {
        return *response;
    }
    route_git(
        &state,
        peer,
        "POST",
        format!("/git/{owner}/{repo}/info/lfs/objects/{oid}/verify"),
        HashMap::new(),
        &headers,
        body.to_vec(),
    )
    .await
}

pub(crate) async fn git_lfs_locks_verify(
    State(state): State<Arc<WebState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    AxumPath((owner, repo)): AxumPath<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> AxumResponse {
    if let Err(response) = authorize_git_core(&state, peer, &headers, &owner, &repo, true) {
        return *response;
    }
    route_git(
        &state,
        peer,
        "POST",
        format!("/git/{owner}/{repo}/info/lfs/locks/verify"),
        HashMap::new(),
        &headers,
        body.to_vec(),
    )
    .await
}

pub(crate) async fn git_lfs_upload(
    State(state): State<Arc<WebState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    AxumPath((owner, repo, oid)): AxumPath<(String, String, String)>,
    request: axum::extract::Request,
) -> AxumResponse {
    let headers = request.headers().clone();
    let manager = (*state.repo_manager).clone();
    if let Err(response) = authorize_git_core(&state, peer, &headers, &owner, &repo, true) {
        return *response;
    }
    let oid = match normalize_oid(&oid) {
        Ok(oid) => oid,
        Err(err) => return gitd_error_to_axum(err),
    };
    let repo = match manager.open_parts(&owner, &repo) {
        Ok(repo) => repo,
        Err(err) => return gitd_error_to_axum(err),
    };
    let expected_size = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let max_size = manager.config().lfs_max_object_bytes;
    if expected_size.is_some_and(|size| size > max_size) {
        return lfs_json_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            "LFS object is larger than the configured limit",
        );
    }
    let store = LfsStore::for_repo(&repo.path);
    match stream_lfs_upload(store, oid, expected_size, max_size, request.into_body()).await {
        Ok(()) => lfs_empty_response(StatusCode::OK),
        Err(err) => gitd_error_to_axum(err),
    }
}

fn origin_base_url(headers: &HeaderMap) -> String {
    match headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|host| !host.trim().is_empty())
    {
        Some(host) => format!("http://{host}"),
        None => String::new(),
    }
}

fn authorize_git_core(
    state: &WebState,
    peer: SocketAddr,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
    write: bool,
) -> Result<(), Box<AxumResponse>> {
    if !state.auth_required || (state.trust_local_dev && peer.ip().is_loopback()) {
        return Ok(());
    }
    // Use the transport resolver's identity for every authorization decision.
    // Independently stripping a suffix can authorize a different metadata row
    // from the bare repository that Git actually opens.
    let resolved = state
        .repo_manager
        .resolve_parts(owner, repo)
        .map_err(|error| Box::new(gitd_error_to_axum(error)))?;
    let owner = resolved.id.owner.as_str();
    let logical_repo = resolved.id.name.as_str();
    let public_read = !write
        && state
            .core
            .get_repository(owner, logical_repo)
            .is_ok_and(|repository| !repository.private);
    let auth = crate::web::auth::authenticate_headers(state, headers).filter(|auth| {
        !headers.contains_key(header::AUTHORIZATION)
            || auth.source == crate::web::auth::AuthSource::Bearer
    });
    let Some(auth) = auth else {
        // Public Git reads need no credential. Supplied invalid authorization
        // still fails so a revoked or mistyped token is never silently accepted.
        if public_read && !headers.contains_key(header::AUTHORIZATION) {
            return Ok(());
        }
        return Err(Box::new(gitd_to_axum_response(
            &GitHttpResponse::text(401, "Requires authentication\n")
                .with_header("WWW-Authenticate", "Basic realm=\"jeryu\""),
        )));
    };
    let account = auth.account;
    let allowed = if write {
        state
            .core
            .user_can_write_repo(&account.login, owner, logical_repo)
    } else {
        public_read
            || state
                .core
                .user_can_read_repo(&account.login, owner, logical_repo)
    };
    if allowed {
        Ok(())
    } else {
        Err(Box::new(gitd_to_axum_response(&GitHttpResponse::bytes(
            403,
            "application/json; charset=utf-8",
            format!(
                "{{\"message\":\"principal not authorized to {} {owner}/{repo}\",\"documentation_url\":\"https://docs.jeryu/auth\"}}",
                if write { "write" } else { "read" }
            )
            .into_bytes(),
        ))))
    }
}

async fn stream_lfs_upload(
    store: LfsStore,
    oid: String,
    expected_size: Option<u64>,
    max_size: u64,
    body: Body,
) -> Result<(), GitdError> {
    let path = store.object_path(&oid)?;
    let parent = path
        .parent()
        .ok_or_else(|| GitdError::Lfs("LFS object path has no parent".to_string()))?;
    tokio::fs::create_dir_all(parent).await?;
    let tmp = lfs_tmp_path(&path);
    let result = stream_lfs_upload_inner(&oid, expected_size, max_size, body, &tmp, &path).await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&tmp).await;
    }
    result.map(|_| ())
}

async fn stream_lfs_upload_inner(
    oid: &str,
    expected_size: Option<u64>,
    max_size: u64,
    body: Body,
    tmp: &Path,
    path: &Path,
) -> Result<u64, GitdError> {
    let mut file = tokio::fs::File::create(tmp).await?;
    let mut stream = body.into_data_stream();
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| GitdError::Lfs(format!("read LFS upload body: {err}")))?;
        total = total
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| GitdError::Lfs("LFS object size overflowed u64".to_string()))?;
        if total > max_size {
            return Err(GitdError::Lfs(format!(
                "LFS object {oid} is larger than the configured limit of {max_size} bytes"
            )));
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.sync_all().await?;
    drop(file);
    if let Some(expected) = expected_size
        && total != expected
    {
        return Err(GitdError::Lfs(format!(
            "size mismatch for {oid}: expected {expected}, got {total}"
        )));
    }
    let actual = hex::encode(hasher.finalize());
    if actual != oid {
        return Err(GitdError::Lfs(format!(
            "sha256 mismatch: expected {oid}, got {actual}"
        )));
    }
    tokio::fs::rename(tmp, path).await?;
    Ok(total)
}

fn lfs_tmp_path(path: &Path) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("object");
    path.with_file_name(format!(".{file_name}.tmp.{}.{}", std::process::id(), now))
}

fn gitd_error_to_axum(err: GitdError) -> AxumResponse {
    match err {
        GitdError::Unauthorized => gitd_to_axum_response(
            &GitHttpResponse::text(401, "Requires authentication\n")
                .with_header("WWW-Authenticate", "Basic realm=\"jeryu\""),
        ),
        GitdError::Forbidden(msg) | GitdError::ProtectedRefDenied(msg) => {
            gitd_to_axum_response(&GitHttpResponse::bytes(
                403,
                "application/json; charset=utf-8",
                serde_json::json!({ "message": msg })
                    .to_string()
                    .into_bytes(),
            ))
        }
        GitdError::RepoNotFound(_) => {
            lfs_json_response(StatusCode::NOT_FOUND, "repository not found")
        }
        GitdError::Lfs(msg) => lfs_json_response(StatusCode::UNPROCESSABLE_ENTITY, &msg),
        err => {
            let body = format!("jeryu_gitd error: {err}\n");
            (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
        }
    }
}

fn lfs_empty_response(status: StatusCode) -> AxumResponse {
    let mut response = (status, "{}").into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/vnd.git-lfs+json"),
    );
    response
}

fn lfs_json_response(status: StatusCode, message: &str) -> AxumResponse {
    let mut response = (
        status,
        serde_json::json!({ "message": message }).to_string(),
    )
        .into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/vnd.git-lfs+json"),
    );
    response
}

/// Snapshot a repo's refs. A repo that cannot be resolved or listed yields an
/// empty snapshot, so the post-push diff simply finds no updates.
fn snapshot_refs(manager: &RepoManager, owner: &str, repo: &str) -> Vec<jeryu_gitd::refs::GitRef> {
    manager
        .resolve_parts(owner, repo)
        .and_then(|resolved| {
            jeryu_gitd::refs::RefService::new(manager.clone()).list_refs(&resolved)
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        GitRpcBodyError, GitRpcContentEncoding, decode_git_rpc_body, forwarded_git_headers,
        git_rpc_content_encoding, read_git_rpc_body,
    };
    use axum::body::{Body, Bytes};
    use axum::http::{HeaderMap, HeaderValue, header};
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn git_rpc_encoding_is_closed_and_forwards_only_git_protocol() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        headers.insert("git-protocol", HeaderValue::from_static("version=2"));
        assert_eq!(
            git_rpc_content_encoding(&headers),
            Ok(GitRpcContentEncoding::Gzip)
        );
        let forwarded = forwarded_git_headers(&headers);
        assert_eq!(
            forwarded.get("git-protocol").map(String::as_str),
            Some("version=2")
        );
        assert!(!forwarded.contains_key("content-encoding"));

        headers.insert(
            header::CONTENT_ENCODING,
            HeaderValue::from_static("gzip, br"),
        );
        assert_eq!(
            git_rpc_content_encoding(&headers),
            Err(GitRpcBodyError::UnsupportedEncoding)
        );
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("br"));
        assert_eq!(
            git_rpc_content_encoding(&headers),
            Err(GitRpcBodyError::UnsupportedEncoding)
        );
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("x-gzip"));
        assert_eq!(
            git_rpc_content_encoding(&headers),
            Ok(GitRpcContentEncoding::Gzip)
        );
        headers.append(
            header::CONTENT_ENCODING,
            HeaderValue::from_static("identity"),
        );
        assert_eq!(
            git_rpc_content_encoding(&headers),
            Err(GitRpcBodyError::UnsupportedEncoding)
        );
    }

    #[test]
    fn gzip_git_rpc_body_decodes_exactly_and_rejects_hostile_forms() {
        let mut request = Vec::new();
        while request.len() < 2_048 {
            request.extend_from_slice(b"0032want 0123456789012345678901234567890123456789\n");
        }
        let encoded = Bytes::from(gzip(&request));
        assert_eq!(
            decode_git_rpc_body(GitRpcContentEncoding::Gzip, encoded.clone(), request.len())
                .unwrap(),
            request
        );
        assert_eq!(
            decode_git_rpc_body(
                GitRpcContentEncoding::Gzip,
                encoded.clone(),
                request.len() - 1
            ),
            Err(GitRpcBodyError::TooLarge)
        );

        let mut trailing = encoded.to_vec();
        trailing.extend_from_slice(b"not-a-second-content-coding");
        assert_eq!(
            decode_git_rpc_body(
                GitRpcContentEncoding::Gzip,
                Bytes::from(trailing),
                request.len() + 64,
            ),
            Err(GitRpcBodyError::MalformedGzip)
        );
        let truncated = encoded.slice(..encoded.len() - 1);
        assert_eq!(
            decode_git_rpc_body(GitRpcContentEncoding::Gzip, truncated, request.len()),
            Err(GitRpcBodyError::MalformedGzip)
        );
        assert_eq!(
            decode_git_rpc_body(
                GitRpcContentEncoding::Gzip,
                Bytes::from_static(b"not gzip"),
                1024,
            ),
            Err(GitRpcBodyError::MalformedGzip)
        );
    }

    #[tokio::test]
    async fn git_rpc_reader_bounds_identity_and_gzip_bodies() {
        let plain = Bytes::from_static(b"0008done");
        assert_eq!(
            read_git_rpc_body(&HeaderMap::new(), Body::from(plain.clone()), 0, plain.len(),)
                .await
                .unwrap(),
            plain
        );
        assert_eq!(
            read_git_rpc_body(
                &HeaderMap::new(),
                Body::from(plain.clone()),
                usize::MAX,
                plain.len() - 1,
            )
            .await,
            Err(GitRpcBodyError::TooLarge)
        );

        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        let decoded = b"12345678";
        let encoded = gzip(decoded);
        assert_eq!(
            read_git_rpc_body(
                &headers,
                Body::from(encoded.clone()),
                encoded.len(),
                decoded.len(),
            )
            .await
            .unwrap(),
            decoded
        );
        assert_eq!(
            read_git_rpc_body(
                &headers,
                Body::from(encoded.clone()),
                encoded.len() - 1,
                decoded.len(),
            )
            .await,
            Err(GitRpcBodyError::TooLarge)
        );
        assert_eq!(
            read_git_rpc_body(&headers, Body::from(encoded), usize::MAX, decoded.len() - 1,).await,
            Err(GitRpcBodyError::TooLarge)
        );
    }
}
