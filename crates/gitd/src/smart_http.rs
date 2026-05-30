//! Minimal smart HTTP server for Phase 1 Git operations.

use crate::error::{GitdError, Result};
use crate::pack::{advertise_refs, stateless_rpc, PackService};
use crate::pktline;
use crate::repo::RepoManager;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

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
        let request = HttpRequest::read(&mut stream)?;
        let response = self.route(request);
        response.write(&mut stream)
    }

    /// Route an HTTP request and return a response.
    pub fn route(&self, request: HttpRequest) -> HttpResponse {
        match self.route_inner(request) {
            Ok(response) => response,
            Err(err) => HttpResponse::text(500, &format!("gitd error: {err}\n")),
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
        let repo = self.manager.open_parts(&owner, &repo_name)?;
        let mut body = pktline::encode_str(&format!("# service={}\n", service.http_name()));
        body.extend(pktline::flush());
        body.extend(advertise_refs(
            &self.manager.config().git_bin,
            &repo,
            service,
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
        let repo = self.manager.open_parts(&owner, &repo_name)?;
        let body = stateless_rpc(
            &self.manager.config().git_bin,
            &repo,
            service,
            &request.body,
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
        let repo = self.manager.open_parts(&owner, &repo_name)?;
        let store = crate::lfs::LfsStore::for_repo(&repo.path);
        let text = String::from_utf8_lossy(&request.body);
        let body = store.batch_response_from_jsonish(&text);
        Ok(HttpResponse::bytes(
            200,
            "application/vnd.git-lfs+json",
            body.into_bytes(),
        ))
    }
}

fn parse_repo_from_path(path: &str) -> Result<(String, String)> {
    let path = path.trim_matches('/');
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() != 2 {
        return Err(GitdError::Http(format!(
            "expected /owner/repo.git path, got /{path}"
        )));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
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
}

impl HttpRequest {
    fn read(stream: &mut TcpStream) -> Result<Self> {
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
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = buffer[header_end + 4..].to_vec();
        while body.len() < content_length {
            let n = stream.read(&mut tmp)?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&tmp[..n]);
        }
        body.truncate(content_length);
        Ok(Self {
            method,
            path,
            query,
            headers,
            body,
        })
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
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(value) = u8::from_str_radix(hex, 16) {
                    out.push(value);
                    i += 3;
                    continue;
                }
            }
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
}

impl HttpResponse {
    /// Text response.
    #[must_use]
    pub fn text(status: u16, body: &str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: body.as_bytes().to_vec(),
        }
    }

    /// Bytes response.
    #[must_use]
    pub fn bytes(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type: content_type.to_string(),
            body,
        }
    }

    fn write(&self, stream: &mut TcpStream) -> Result<()> {
        let status_text = match self.status {
            200 => "OK",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "OK",
        };
        write!(
            stream,
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
            self.status,
            status_text,
            self.content_type,
            self.body.len()
        )?;
        stream.write_all(&self.body)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_http_splits_query() {
        let (path, query) = split_path_query("/acme/demo.git/info/refs?service=git-upload-pack");
        assert_eq!(path, "/acme/demo.git/info/refs");
        assert_eq!(query.get("service"), Some(&"git-upload-pack".to_string()));
    }
}
