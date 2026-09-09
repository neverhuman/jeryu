//! Minimal smart HTTP server for Phase 1 Git operations.

use crate::auth::{AuthDecision, AuthRegistry, extract_bearer_or_basic};
use crate::command::StreamingCommand;
use crate::error::{GitdError, Result};
use crate::lfs::{LfsStore, LfsVerifyRequest, normalize_oid};
use crate::pack::{
    PackService, advertise_refs_with_protocol, ensure_receive_pack_policy,
    read_receive_pack_prefix, spawn_stateless_rpc, stateless_rpc_with_protocol,
};
use crate::pktline;
use crate::repo::RepoManager;
use std::collections::HashMap;
use std::io::{self, Cursor, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};

/// Realm advertised in `WWW-Authenticate` challenges.
const AUTH_REALM: &str = "jeryu";

/// Blocking smart HTTP server.
#[derive(Clone, Debug)]
pub struct SmartHttpServer {
    manager: RepoManager,
}

impl SmartHttpServer {
    /// Create a server.
    #[must_use]
    pub fn new(manager: RepoManager) -> Self {
        Self { manager }
    }

    /// Open the auth registry rooted under the manager's storage root.
    fn auth_registry(&self) -> Result<AuthRegistry> {
        AuthRegistry::open(&self.manager.config().storage_root)
    }

    /// Authorize a request against the credential registry.
    ///
    /// `write` selects receive-pack (mutating) semantics. On denial the
    /// returned response is a GitHub-shaped 401 (with `WWW-Authenticate`) or a
    /// 403 JSON body.
    fn authorize(&self, request: &HttpRequest, owner: &str, write: bool) -> Result<()> {
        if request.auth_prechecked {
            return Ok(());
        }
        let registry = self.auth_registry()?;
        let credential = request
            .headers
            .get("authorization")
            .and_then(|h| extract_bearer_or_basic(h));
        match registry.decide(credential.as_deref(), request.is_loopback, owner, write) {
            AuthDecision::Allow(_) => Ok(()),
            AuthDecision::Deny401 => Err(GitdError::Unauthorized),
            AuthDecision::Deny403 => Err(GitdError::Forbidden(format!(
                "principal not authorized to {} {owner}",
                if write { "write" } else { "read" }
            ))),
        }
    }

    /// Serve forever on the configured address.
    pub fn serve(&self, addr: &str) -> Result<()> {
        let listener = TcpListener::bind(addr)?;
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let server = self.clone();
                    std::thread::spawn(move || {
                        let _ = server.handle_stream(stream);
                    });
                }
                Err(err) => return Err(GitdError::Io(err)),
            }
        }
        Ok(())
    }

    fn handle_stream(&self, mut stream: TcpStream) -> Result<()> {
        let is_loopback = stream
            .peer_addr()
            .map(|addr| addr.ip().is_loopback())
            .unwrap_or(false);
        let (mut request, content_length, body_prefix) = HttpRequest::read_head(&mut stream)?;
        request.is_loopback = is_loopback;
        let pack_service = if request.method == "POST" && request.path.ends_with("/git-upload-pack")
        {
            Some(PackService::UploadPack)
        } else if request.method == "POST" && request.path.ends_with("/git-receive-pack") {
            Some(PackService::ReceivePack)
        } else {
            None
        };
        if let Some(service) = pack_service {
            let Some(content_length) = content_length else {
                return HttpResponse::text(411, "Git smart HTTP RPC requires Content-Length\n")
                    .write(&mut stream);
            };
            let mut input_stream = stream.try_clone()?;
            return match self.prepare_streaming_rpc(
                &request,
                service,
                content_length,
                body_prefix,
                &mut input_stream,
                &mut stream,
            ) {
                Ok(rpc) => rpc.stream(input_stream, &mut stream),
                Err(err) => error_response(err).write(&mut stream),
            };
        }
        if request.method == "GET" && is_lfs_object_transfer_path(&request.path) {
            return match self.prepare_lfs_download(&request) {
                Ok(Some(download)) => download.stream(&mut stream),
                Ok(None) => lfs_error_response(404, "object not found").write(&mut stream),
                Err(err) => error_response(err).write(&mut stream),
            };
        }
        let response = if request.method == "PUT" && is_lfs_object_transfer_path(&request.path) {
            match content_length {
                Some(content_length) => {
                    let remaining = content_length.saturating_sub(body_prefix.len() as u64);
                    let reader = Cursor::new(body_prefix).chain((&mut stream).take(remaining));
                    self.lfs_upload_stream(&request, Some(content_length), reader)
                }
                None => Ok(lfs_error_response(
                    411,
                    "LFS basic uploads require Content-Length",
                )),
            }
            .unwrap_or_else(error_response)
        } else {
            request.read_remaining_body(&mut stream, content_length, body_prefix)?;
            self.route(request)
        };
        response.write(&mut stream)
    }

    fn prepare_streaming_rpc(
        &self,
        request: &HttpRequest,
        service: PackService,
        content_length: u64,
        body_prefix: Vec<u8>,
        input: &mut TcpStream,
        output: &mut TcpStream,
    ) -> Result<PreparedRpc> {
        let suffix = format!("/{}", service.http_name());
        let base = request.path.trim_end_matches(&suffix);
        let (owner, repo_name) = parse_repo_from_path(base)?;
        self.authorize(request, &owner, service.is_write())?;
        let repo = self.manager.open_parts(&owner, &repo_name)?;
        write_continue_if_requested(request, output)?;
        let body_prefix = if service == PackService::ReceivePack {
            read_receive_pack_prefix(input, content_length, body_prefix)?
        } else {
            body_prefix
        };
        let prefix_len = u64::try_from(body_prefix.len())
            .map_err(|_| GitdError::Protocol("request body prefix is too large".to_string()))?;
        let remaining = content_length.checked_sub(prefix_len).ok_or_else(|| {
            GitdError::Protocol("request body prefix exceeds Content-Length".to_string())
        })?;
        let process = spawn_stateless_rpc(
            &self.manager.config().git_bin,
            &repo,
            service,
            git_protocol_header(request)?,
        )?;
        Ok(PreparedRpc {
            process,
            body_prefix,
            remaining,
            content_length,
            content_type: format!("application/x-{}-result", service.http_name()),
        })
    }

    fn prepare_lfs_download(&self, request: &HttpRequest) -> Result<Option<PreparedLfsDownload>> {
        let (owner, repo_name, oid) = lfs_object_path_parts(&request.path)?;
        self.authorize(request, &owner, false)?;
        let repo = self.manager.open_parts(&owner, &repo_name)?;
        let Some((file, size)) = LfsStore::for_repo(&repo.path).open_reader(&oid)? else {
            return Ok(None);
        };
        Ok(Some(PreparedLfsDownload { file, size }))
    }

    /// Route a fully materialized synthetic HTTP request and return a response.
    ///
    /// Authentication denials are rendered as GitHub-shaped responses: a 401
    /// carrying `WWW-Authenticate: Basic realm="jeryu"`, or a 403 JSON body.
    /// The production socket path bypasses this compatibility seam for Git pack
    /// POSTs and streams them with backpressure.
    pub fn route(&self, request: HttpRequest) -> HttpResponse {
        match self.route_inner(request) {
            Ok(response) => response,
            Err(err) => error_response(err),
        }
    }

    fn route_inner(&self, request: HttpRequest) -> Result<HttpResponse> {
        if request.method == "GET" && request.path.ends_with("/info/refs") {
            return self.info_refs(&request);
        }
        if request.method == "POST" && request.path.ends_with("/git-upload-pack") {
            return self.rpc(&request, PackService::UploadPack);
        }
        if request.method == "POST" && request.path.ends_with("/git-receive-pack") {
            return self.rpc(&request, PackService::ReceivePack);
        }
        if request.method == "POST" && request.path.ends_with("/info/lfs/objects/batch") {
            return self.lfs_batch(&request);
        }
        if request.method == "POST" && request.path.ends_with("/info/lfs/locks/verify") {
            return self.lfs_locks_verify(&request);
        }
        if request.method == "POST" && is_lfs_verify_path(&request.path) {
            return self.lfs_verify(&request);
        }
        if request.method == "GET" && is_lfs_object_transfer_path(&request.path) {
            return self.lfs_download(&request);
        }
        if request.method == "PUT" && is_lfs_object_transfer_path(&request.path) {
            return self.lfs_upload_stream(
                &request,
                Some(request.body.len() as u64),
                Cursor::new(&request.body),
            );
        }
        Err(GitdError::Http(format!(
            "no route for {} {}",
            request.method, request.path
        )))
    }

    fn info_refs(&self, request: &HttpRequest) -> Result<HttpResponse> {
        let service = request
            .query
            .get("service")
            .ok_or_else(|| GitdError::Http("missing service query parameter".to_string()))?;
        let service = PackService::parse(service)
            .ok_or_else(|| GitdError::Http(format!("unsupported service: {service}")))?;
        let (owner, repo_name) = parse_repo_from_path(request.path.trim_end_matches("/info/refs"))?;
        self.authorize(request, &owner, service.is_write())?;
        let repo = self.manager.open_parts(&owner, &repo_name)?;
        let mut body = pktline::encode_str(&format!("# service={}\n", service.http_name()));
        body.extend(pktline::flush());
        body.extend(advertise_refs_with_protocol(
            &self.manager.config().git_bin,
            &repo,
            service,
            git_protocol_header(request)?,
        )?);
        Ok(HttpResponse::bytes(
            200,
            &format!("application/x-{}-advertisement", service.http_name()),
            body,
        ))
    }

    fn rpc(&self, request: &HttpRequest, service: PackService) -> Result<HttpResponse> {
        let suffix = format!("/{}", service.http_name());
        let base = request.path.trim_end_matches(&suffix);
        let (owner, repo_name) = parse_repo_from_path(base)?;
        self.authorize(request, &owner, service.is_write())?;
        let repo = self.manager.open_parts(&owner, &repo_name)?;
        if service == PackService::ReceivePack {
            ensure_receive_pack_policy(&request.body)?;
        }
        let body = stateless_rpc_with_protocol(
            &self.manager.config().git_bin,
            &repo,
            service,
            &request.body,
            git_protocol_header(request)?,
        )?;
        Ok(HttpResponse::bytes(
            200,
            &format!("application/x-{}-result", service.http_name()),
            body,
        ))
    }

    fn lfs_batch(&self, request: &HttpRequest) -> Result<HttpResponse> {
        let (owner, repo_name) =
            parse_repo_from_path(request.path.trim_end_matches("/info/lfs/objects/batch"))?;
        self.authorize(request, &owner, lfs_batch_is_write(&request.body)?)?;
        let repo = self.manager.open_parts(&owner, &repo_name)?;
        let store = LfsStore::for_repo(&repo.path);
        let text = String::from_utf8_lossy(&request.body);
        let objects_url = lfs_objects_url(request);
        let auth_header = request.headers.get("authorization").map(String::as_str);
        let body = store.batch_response(
            &text,
            &objects_url,
            auth_header,
            self.manager.config().lfs_max_object_bytes,
        )?;
        Ok(HttpResponse::bytes(
            200,
            "application/vnd.git-lfs+json",
            body,
        ))
    }

    fn lfs_upload_stream(
        &self,
        request: &HttpRequest,
        expected_size: Option<u64>,
        reader: impl Read,
    ) -> Result<HttpResponse> {
        let (owner, repo_name, oid) = lfs_object_path_parts(&request.path)?;
        self.authorize(request, &owner, true)?;
        let repo = self.manager.open_parts(&owner, &repo_name)?;
        LfsStore::for_repo(&repo.path).put_reader_with_limit(
            &oid,
            expected_size,
            self.manager.config().lfs_max_object_bytes,
            reader,
        )?;
        Ok(HttpResponse::bytes(
            200,
            "application/vnd.git-lfs+json",
            b"{}".to_vec(),
        ))
    }

    fn lfs_download(&self, request: &HttpRequest) -> Result<HttpResponse> {
        let (owner, repo_name, oid) = lfs_object_path_parts(&request.path)?;
        self.authorize(request, &owner, false)?;
        let repo = self.manager.open_parts(&owner, &repo_name)?;
        let store = LfsStore::for_repo(&repo.path);
        if !store.exists(&oid) {
            return Ok(lfs_error_response(404, "object not found"));
        }
        Ok(HttpResponse::bytes(
            200,
            "application/octet-stream",
            store.get(&oid)?,
        ))
    }

    fn lfs_verify(&self, request: &HttpRequest) -> Result<HttpResponse> {
        let (owner, repo_name, oid) = lfs_object_path_parts(&request.path)?;
        self.authorize(request, &owner, true)?;
        let repo = self.manager.open_parts(&owner, &repo_name)?;
        let verify: LfsVerifyRequest = serde_json::from_slice(&request.body)
            .map_err(|err| GitdError::Lfs(format!("invalid LFS verify JSON: {err}")))?;
        let body_oid = normalize_oid(&verify.oid)?;
        if body_oid != oid {
            return Ok(lfs_error_response(
                422,
                "verify oid does not match transfer URL",
            ));
        }
        LfsStore::for_repo(&repo.path).verify(&oid, verify.size)?;
        Ok(HttpResponse::bytes(
            200,
            "application/vnd.git-lfs+json",
            b"{}".to_vec(),
        ))
    }

    fn lfs_locks_verify(&self, request: &HttpRequest) -> Result<HttpResponse> {
        let (owner, repo_name) =
            parse_repo_from_path(request.path.trim_end_matches("/info/lfs/locks/verify"))?;
        self.authorize(request, &owner, true)?;
        self.manager.open_parts(&owner, &repo_name)?;
        Ok(HttpResponse::bytes(
            200,
            "application/vnd.git-lfs+json",
            br#"{"ours":[],"theirs":[]}"#.to_vec(),
        ))
    }
}

struct PreparedRpc {
    process: StreamingCommand,
    body_prefix: Vec<u8>,
    remaining: u64,
    content_length: u64,
    content_type: String,
}

impl PreparedRpc {
    fn stream(self, input: TcpStream, output: &mut TcpStream) -> Result<()> {
        let cancel_input = input.try_clone()?;
        HttpResponse::write_streaming_head(output, &self.content_type)?;
        let body = Cursor::new(self.body_prefix).chain(input.take(self.remaining));
        self.process.pump(body, output, self.content_length, || {
            cancel_input.shutdown(Shutdown::Read)
        })
    }
}

struct PreparedLfsDownload {
    file: std::fs::File,
    size: u64,
}

impl PreparedLfsDownload {
    fn stream(mut self, output: &mut TcpStream) -> Result<()> {
        HttpResponse::write_sized_streaming_head(output, "application/octet-stream", self.size)?;
        let mut limited = Read::take(&mut self.file, self.size);
        let copied = io::copy(&mut limited, output)?;
        if copied != self.size {
            return Err(GitdError::Lfs(format!(
                "LFS object changed while streaming: expected {} bytes, copied {copied}",
                self.size
            )));
        }
        Ok(())
    }
}

fn parse_repo_from_path(path: &str) -> Result<(String, String)> {
    let path = path.trim_matches('/');
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        [owner, repo] => Ok(((*owner).to_string(), (*repo).to_string())),
        ["git", owner, repo] => Ok(((*owner).to_string(), (*repo).to_string())),
        _ => Err(GitdError::Http(format!(
            "expected /owner/repo.git path, got /{path}"
        ))),
    }
}

fn lfs_object_path_parts(path: &str) -> Result<(String, String, String)> {
    let path = path.trim_end_matches("/verify");
    let Some((repo_path, oid)) = path.rsplit_once("/info/lfs/objects/") else {
        return Err(GitdError::Http(format!(
            "expected LFS object path, got {path}"
        )));
    };
    if oid == "batch" || oid.is_empty() {
        return Err(GitdError::Http(format!(
            "expected LFS object oid, got {path}"
        )));
    }
    let oid = normalize_oid(oid)?;
    let (owner, repo_name) = parse_repo_from_path(repo_path)?;
    Ok((owner, repo_name, oid))
}

fn is_lfs_object_transfer_path(path: &str) -> bool {
    let path = path.trim_end_matches("/verify");
    path.contains("/info/lfs/objects/")
        && !path.ends_with("/info/lfs/objects/batch")
        && !path.ends_with("/info/lfs/objects/")
}

fn is_lfs_verify_path(path: &str) -> bool {
    path.ends_with("/verify") && is_lfs_object_transfer_path(path)
}

fn lfs_batch_is_write(body: &[u8]) -> Result<bool> {
    let value: serde_json::Value = serde_json::from_slice(body)
        .map_err(|err| GitdError::Lfs(format!("invalid LFS batch JSON: {err}")))?;
    match value
        .get("operation")
        .and_then(|operation| operation.as_str())
    {
        Some("upload") => Ok(true),
        Some("download") => Ok(false),
        Some(other) => Err(GitdError::Lfs(format!(
            "unsupported LFS batch operation: {other}"
        ))),
        None => Err(GitdError::Lfs("missing LFS batch operation".to_string())),
    }
}

fn lfs_objects_url(request: &HttpRequest) -> String {
    let objects_path = request.path.trim_end_matches("/batch");
    absolute_url(request, objects_path)
}

fn absolute_url(request: &HttpRequest, path: &str) -> String {
    let Some(host) = request.headers.get("host").filter(|host| !host.is_empty()) else {
        return path.to_string();
    };
    let scheme = request
        .headers
        .get("x-forwarded-proto")
        .map(String::as_str)
        .unwrap_or("http");
    format!("{scheme}://{host}{path}")
}

fn git_protocol_header(request: &HttpRequest) -> Result<Option<&str>> {
    match request.headers.get("git-protocol").map(String::as_str) {
        None => Ok(None),
        Some("version=2") => Ok(Some("version=2")),
        Some(_) => Err(GitdError::Protocol(
            "unsupported Git-Protocol header".to_string(),
        )),
    }
}

fn write_continue_if_requested(request: &HttpRequest, output: &mut TcpStream) -> Result<()> {
    let Some(expectation) = request.headers.get("expect") else {
        return Ok(());
    };
    if !expectation.eq_ignore_ascii_case("100-continue") {
        return Err(GitdError::Http(
            "unsupported Expect header for Git RPC".to_string(),
        ));
    }
    output.write_all(b"HTTP/1.1 100 Continue\r\n\r\n")?;
    output.flush()?;
    Ok(())
}

/// Parsed minimal HTTP request.
#[derive(Clone, Debug)]
pub struct HttpRequest {
    /// Method.
    pub method: String,
    /// Path without query string.
    pub path: String,
    /// Query map.
    pub query: HashMap<String, String>,
    /// Headers with lower-case names.
    pub headers: HashMap<String, String>,
    /// Request body.
    pub body: Vec<u8>,
    /// Whether the peer connected over the loopback interface.
    ///
    /// Set by the connection handler from the socket peer address; defaults to
    /// `false` for synthetic requests so authorization fails closed.
    pub is_loopback: bool,
    /// Whether an embedding HTTP edge has already authenticated and authorized
    /// this exact repository/action. Standalone gitd requests leave this false.
    pub auth_prechecked: bool,
}

impl HttpRequest {
    fn read_head(stream: &mut TcpStream) -> Result<(Self, Option<u64>, Vec<u8>)> {
        let mut buffer = Vec::new();
        let mut tmp = [0u8; 4096];
        loop {
            let n = stream.read(&mut tmp)?;
            if n == 0 {
                break;
            }
            buffer.extend_from_slice(&tmp[..n]);
            if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
            if buffer.len() > 1024 * 1024 {
                return Err(GitdError::Http("headers too large".to_string()));
            }
        }
        let header_end = buffer
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or_else(|| GitdError::Http("missing header terminator".to_string()))?;
        let header_bytes = &buffer[..header_end];
        let header_text = String::from_utf8_lossy(header_bytes);
        let mut lines = header_text.lines();
        let request_line = lines
            .next()
            .ok_or_else(|| GitdError::Http("missing request line".to_string()))?;
        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(GitdError::Http("bad request line".to_string()));
        }
        let method = parts[0].to_string();
        let (path, query) = split_path_query(parts[1]);
        let mut headers = HashMap::new();
        for line in lines {
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
            }
        }
        let content_length = headers
            .get("content-length")
            .map(|s| {
                s.parse::<u64>()
                    .map_err(|_| GitdError::Http("invalid Content-Length".to_string()))
            })
            .transpose()?;
        let mut body_prefix = buffer[header_end + 4..].to_vec();
        if let Some(content_length) = content_length
            && body_prefix.len() as u64 > content_length
        {
            body_prefix.truncate(content_length as usize);
        }
        Ok((
            Self {
                method,
                path,
                query,
                headers,
                body: Vec::new(),
                is_loopback: false,
                auth_prechecked: false,
            },
            content_length,
            body_prefix,
        ))
    }

    fn read_remaining_body(
        &mut self,
        stream: &mut TcpStream,
        content_length: Option<u64>,
        mut body: Vec<u8>,
    ) -> Result<()> {
        let content_length = content_length.unwrap_or(0);
        let target = usize::try_from(content_length)
            .map_err(|_| GitdError::Http("Content-Length is too large".to_string()))?;
        let mut tmp = [0u8; 4096];
        while body.len() < target {
            let n = stream.read(&mut tmp)?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&tmp[..n]);
        }
        body.truncate(target);
        self.body = body;
        Ok(())
    }
}

fn split_path_query(raw: &str) -> (String, HashMap<String, String>) {
    let (path, query_raw) = raw.split_once('?').unwrap_or((raw, ""));
    let mut query = HashMap::new();
    for pair in query_raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        query.insert(percent_decode(k), percent_decode(v));
    }
    (percent_decode(path), query)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(value) = u8::from_str_radix(hex, 16)
        {
            out.push(value);
            i += 3;
            continue;
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Minimal HTTP response.
#[derive(Clone, Debug)]
pub struct HttpResponse {
    status: u16,
    content_type: String,
    body: Vec<u8>,
    /// Extra response headers (name, value) emitted verbatim, e.g.
    /// `WWW-Authenticate`.
    extra_headers: Vec<(String, String)>,
}

impl HttpResponse {
    /// Text response.
    #[must_use]
    pub fn text(status: u16, body: &str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: body.as_bytes().to_vec(),
            extra_headers: Vec::new(),
        }
    }

    /// Bytes response.
    #[must_use]
    pub fn bytes(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type: content_type.to_string(),
            body,
            extra_headers: Vec::new(),
        }
    }

    /// Attach an extra response header.
    #[must_use]
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.extra_headers
            .push((name.to_string(), value.to_string()));
        self
    }

    /// The HTTP status code.
    #[must_use]
    pub fn status(&self) -> u16 {
        self.status
    }

    /// Look up an extra header value by case-insensitive name.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.extra_headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The response body bytes.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// The response `Content-Type`.
    #[must_use]
    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    /// Extra response headers (name, value) to emit verbatim.
    #[must_use]
    pub fn extra_headers(&self) -> &[(String, String)] {
        &self.extra_headers
    }

    fn write(&self, stream: &mut TcpStream) -> Result<()> {
        let status_text = match self.status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            411 => "Length Required",
            413 => "Payload Too Large",
            422 => "Unprocessable Entity",
            500 => "Internal Server Error",
            _ => "OK",
        };
        write!(
            stream,
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n",
            self.status,
            status_text,
            self.content_type,
            self.body.len()
        )?;
        for (name, value) in &self.extra_headers {
            write!(stream, "{name}: {value}\r\n")?;
        }
        write!(stream, "\r\n")?;
        stream.write_all(&self.body)?;
        Ok(())
    }

    fn write_streaming_head(stream: &mut TcpStream, content_type: &str) -> Result<()> {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n"
        )?;
        stream.flush()?;
        Ok(())
    }

    fn write_sized_streaming_head(
        stream: &mut TcpStream,
        content_type: &str,
        content_length: u64,
    ) -> Result<()> {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {content_length}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n"
        )?;
        stream.flush()?;
        Ok(())
    }
}

fn error_response(err: GitdError) -> HttpResponse {
    match err {
        GitdError::Unauthorized => unauthorized_response(),
        GitdError::Forbidden(msg) => forbidden_response(&msg),
        GitdError::ProtectedRefDenied(msg) => forbidden_response(&msg),
        GitdError::Lfs(msg) => lfs_error_response(422, &msg),
        err => HttpResponse::text(500, &format!("jeryu_gitd error: {err}\n")),
    }
}

/// Build a GitHub-shaped 401 challenge.
///
/// Mirrors GitHub's smart-HTTP 401: a `WWW-Authenticate: Basic realm="jeryu"`
/// header plus a short plaintext body so `git` re-prompts for credentials.
#[must_use]
fn unauthorized_response() -> HttpResponse {
    HttpResponse::text(401, "Requires authentication\n")
        .with_header("WWW-Authenticate", &format!("Basic realm=\"{AUTH_REALM}\""))
}

/// Build a GitHub-shaped 403 JSON body for an authorization failure.
#[must_use]
fn forbidden_response(message: &str) -> HttpResponse {
    let body = format!(
        "{{\"message\":{},\"documentation_url\":\"https://docs.jeryu/auth\"}}",
        json_string(message)
    );
    HttpResponse::bytes(403, "application/json; charset=utf-8", body.into_bytes())
}

fn lfs_error_response(status: u16, message: &str) -> HttpResponse {
    let body = format!("{{\"message\":{}}}", json_string(message));
    HttpResponse::bytes(status, "application/vnd.git-lfs+json", body.into_bytes())
}

/// Minimal JSON string escaper for the small, controlled messages we emit.
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
#[path = "smart_http_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lfs_stream_tests.rs"]
mod lfs_stream_tests;
