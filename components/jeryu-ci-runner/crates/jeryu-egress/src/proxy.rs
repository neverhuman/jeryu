#![doc = "Async (tokio) forward proxy that enforces the allowlist decision."]
#![doc = ""]
#![doc = "Both the HTTP `CONNECT` tunnel path and the plain-HTTP forward path consult"]
#![doc = "[`crate::egress_decision`] BEFORE opening any upstream socket. A denied"]
#![doc = "request gets a `403` and the connection is closed; the proxy never performs"]
#![doc = "the DNS resolution or TCP connect for a denied host."]

use crate::{Allowlist, Budget, Decision, egress_decision};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// How long to wait for an upstream TCP connection before giving up.
const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Maximum bytes we will buffer while reading a request line + headers. A real
/// proxy request prelude is tiny; this bounds memory for a hostile client.
const MAX_PRELUDE_BYTES: usize = 16 * 1024;

/// Static configuration for a [`Proxy`].
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// The host allowlist to enforce.
    pub allowlist: Allowlist,
    /// The address to bind the listener on (e.g. `127.0.0.1:8889`).
    pub bind: SocketAddr,
}

impl ProxyConfig {
    /// Build a config with the default allowlist bound to `bind`.
    #[must_use]
    pub fn with_default_allowlist(bind: SocketAddr) -> Self {
        Self {
            allowlist: Allowlist::default(),
            bind,
        }
    }
}

/// The forward proxy.
///
/// Holds the shared allowlist and budget so that every accepted connection task
/// reaches the same verdict source.
#[derive(Debug, Clone)]
pub struct Proxy {
    allowlist: Arc<Allowlist>,
    budget: Budget,
}

impl Proxy {
    /// Create a proxy from an allowlist and a shared budget kill switch.
    #[must_use]
    pub fn new(allowlist: Allowlist, budget: Budget) -> Self {
        Self {
            allowlist: Arc::new(allowlist),
            budget,
        }
    }

    /// The shared budget kill switch (clone to trip it from elsewhere).
    #[must_use]
    pub fn budget(&self) -> Budget {
        self.budget.clone()
    }

    /// Bind a listener and serve forever, spawning a task per connection.
    ///
    /// Returns only on a fatal accept error.
    ///
    /// # Errors
    /// Returns an error if binding the listener fails.
    pub async fn serve(&self, bind: SocketAddr) -> io::Result<()> {
        let listener = TcpListener::bind(bind).await?;
        let local = listener.local_addr()?;
        tracing::info!(addr = %local, "egress proxy listening");
        loop {
            let (client, peer) = listener.accept().await?;
            let proxy = self.clone();
            tokio::spawn(async move {
                if let Err(err) = proxy.handle(client).await {
                    tracing::debug!(%peer, error = %err, "connection closed");
                }
            });
        }
    }

    /// Handle a single client connection end to end.
    async fn handle(&self, mut client: TcpStream) -> io::Result<()> {
        let prelude = read_prelude(&mut client).await?;
        let Some(request) = ParsedRequest::parse(&prelude) else {
            respond_status(&mut client, 400, "Bad Request").await?;
            return Ok(());
        };

        let decision = egress_decision(&request.host, &self.allowlist, self.budget.is_exceeded());
        if decision != Decision::Allow {
            tracing::info!(
                host = %request.host,
                reason = decision.reason(),
                method = %request.method,
                "egress denied"
            );
            respond_status(&mut client, 403, "Forbidden").await?;
            return Ok(());
        }

        match request.method.as_str() {
            "CONNECT" => self.tunnel_connect(client, &request).await,
            _ => self.forward_plain(client, &request, &prelude).await,
        }
    }

    /// HTTP `CONNECT`: open the tunnel, ack 200, then splice bidirectionally.
    async fn tunnel_connect(
        &self,
        mut client: TcpStream,
        request: &ParsedRequest,
    ) -> io::Result<()> {
        let upstream = match connect_upstream(&request.host, request.port).await {
            Ok(stream) => stream,
            Err(_) => {
                respond_status(&mut client, 502, "Bad Gateway").await?;
                return Ok(());
            }
        };
        client
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await?;
        client.flush().await?;
        let mut client = client;
        let mut upstream = upstream;
        tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
        Ok(())
    }

    /// Plain HTTP: replay the original prelude upstream, then splice.
    async fn forward_plain(
        &self,
        mut client: TcpStream,
        request: &ParsedRequest,
        prelude: &[u8],
    ) -> io::Result<()> {
        let mut upstream = match connect_upstream(&request.host, request.port).await {
            Ok(stream) => stream,
            Err(_) => {
                respond_status(&mut client, 502, "Bad Gateway").await?;
                return Ok(());
            }
        };
        // Forward the client's request bytes verbatim, then pipe both ways.
        upstream.write_all(prelude).await?;
        upstream.flush().await?;
        tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
        Ok(())
    }
}

/// Open an upstream TCP connection with a bounded timeout.
async fn connect_upstream(host: &str, port: u16) -> io::Result<TcpStream> {
    let target = format!("{host}:{port}");
    match tokio::time::timeout(UPSTREAM_CONNECT_TIMEOUT, TcpStream::connect(target)).await {
        Ok(result) => result,
        Err(_) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "upstream connect timed out",
        )),
    }
}

/// Read the request line + headers (up to the blank line) into a buffer.
///
/// Bounded by [`MAX_PRELUDE_BYTES`] so a client cannot exhaust memory by never
/// terminating the header block.
async fn read_prelude(client: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    loop {
        let n = client.read(&mut byte).await?;
        if n == 0 {
            break; // EOF before headers terminated
        }
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") || buf.ends_with(b"\n\n") {
            break;
        }
        if buf.len() >= MAX_PRELUDE_BYTES {
            break;
        }
    }
    Ok(buf)
}

/// Write a minimal HTTP/1.1 status response and close.
async fn respond_status(client: &mut TcpStream, code: u16, text: &str) -> io::Result<()> {
    let body = format!("{code} {text}\n");
    let response = format!(
        "HTTP/1.1 {code} {text}\r\nContent-Type: text/plain\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}",
        len = body.len()
    );
    client.write_all(response.as_bytes()).await?;
    client.flush().await?;
    Ok(())
}

/// A parsed request prelude: enough to reach an allowlist verdict and to know
/// where to forward.
#[derive(Debug)]
struct ParsedRequest {
    method: String,
    host: String,
    port: u16,
}

impl ParsedRequest {
    /// Parse the request line out of a prelude buffer.
    ///
    /// For `CONNECT`, the authority is in the request target (`host:port`). For
    /// plain HTTP the target is an absolute URI; we extract the authority from it
    /// and fall back to the `Host:` header if needed.
    fn parse(prelude: &[u8]) -> Option<Self> {
        let text = String::from_utf8_lossy(prelude);
        let mut lines = text.split("\r\n").flat_map(|l| l.split('\n'));
        let request_line = lines.next()?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next()?.to_ascii_uppercase();
        let target = parts.next()?;

        if method == "CONNECT" {
            let (host, port) = split_authority(target, 443);
            if host.is_empty() {
                return None;
            }
            return Some(Self { method, host, port });
        }

        // Plain HTTP: prefer the authority in an absolute-form target.
        if let Some(authority) = absolute_uri_authority(target) {
            let (host, port) = split_authority(&authority, 80);
            if !host.is_empty() {
                return Some(Self { method, host, port });
            }
        }

        // Origin-form target: fall back to the Host header.
        for line in lines {
            if let Some(value) = line
                .strip_prefix("Host:")
                .or_else(|| line.strip_prefix("host:"))
            {
                let (host, port) = split_authority(value.trim(), 80);
                if !host.is_empty() {
                    return Some(Self { method, host, port });
                }
            }
        }
        None
    }
}

/// Extract the `host[:port]` authority from an absolute-form URI target, if any.
fn absolute_uri_authority(target: &str) -> Option<String> {
    let after_scheme = target.split_once("://")?.1;
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    Some(authority.to_string())
}

/// Split a `host[:port]` authority into a (host, port) pair.
fn split_authority(authority: &str, default_port: u16) -> (String, u16) {
    let authority = authority.trim();
    // Strip any userinfo (`user@host`) which is irrelevant to the host check.
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => {
            let port = port.trim().parse().unwrap_or(default_port);
            (host.to_string(), port)
        }
        _ => (authority.to_string(), default_port),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_connect_authority() {
        let req = ParsedRequest::parse(b"CONNECT api.anthropic.com:443 HTTP/1.1\r\n\r\n").unwrap();
        assert_eq!(req.method, "CONNECT");
        assert_eq!(req.host, "api.anthropic.com");
        assert_eq!(req.port, 443);
    }

    #[test]
    fn parses_connect_default_port() {
        let req = ParsedRequest::parse(b"CONNECT github.com HTTP/1.1\r\n\r\n").unwrap();
        assert_eq!(req.host, "github.com");
        assert_eq!(req.port, 443);
    }

    #[test]
    fn parses_absolute_http_target() {
        let req =
            ParsedRequest::parse(b"GET http://crates.io/api HTTP/1.1\r\nHost: crates.io\r\n\r\n")
                .unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.host, "crates.io");
        assert_eq!(req.port, 80);
    }

    #[test]
    fn parses_origin_form_via_host_header() {
        let req =
            ParsedRequest::parse(b"GET /api HTTP/1.1\r\nHost: index.crates.io\r\n\r\n").unwrap();
        assert_eq!(req.host, "index.crates.io");
        assert_eq!(req.port, 80);
    }

    #[test]
    fn rejects_garbage_request_line() {
        assert!(ParsedRequest::parse(b"\r\n\r\n").is_none());
        assert!(ParsedRequest::parse(b"GET\r\n\r\n").is_none());
    }

    #[test]
    fn split_authority_handles_userinfo_and_port() {
        assert_eq!(
            split_authority("user@host.example:8080", 80),
            ("host.example".to_string(), 8080)
        );
        assert_eq!(
            split_authority("host.example", 80),
            ("host.example".to_string(), 80)
        );
    }

    #[test]
    fn connect_target_authority_wins_over_host_header() {
        // For CONNECT, the request-target authority is the tunnel target; a
        // conflicting Host header must NOT redirect the decision elsewhere.
        let req = ParsedRequest::parse(
            b"CONNECT real.target.example:443 HTTP/1.1\r\nHost: decoy.example:80\r\n\r\n",
        )
        .unwrap();
        assert_eq!(req.method, "CONNECT");
        assert_eq!(req.host, "real.target.example");
        assert_eq!(req.port, 443);
    }

    #[test]
    fn plain_absolute_uri_authority_wins_over_host_header() {
        // Absolute-form target authority is preferred over the Host header.
        let req = ParsedRequest::parse(
            b"GET http://uri.example/path HTTP/1.1\r\nHost: header.example\r\n\r\n",
        )
        .unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.host, "uri.example");
        assert_eq!(req.port, 80);
    }

    #[test]
    fn userinfo_with_password_is_stripped_from_authority() {
        // user:pass@host[:port] -> userinfo discarded, host/port preserved.
        assert_eq!(
            split_authority("alice:secret@host.example:8443", 80),
            ("host.example".to_string(), 8443)
        );
        let req = ParsedRequest::parse(b"CONNECT bob@inner.example:443 HTTP/1.1\r\n\r\n").unwrap();
        assert_eq!(req.host, "inner.example");
        assert_eq!(req.port, 443);
    }

    #[test]
    fn non_numeric_port_falls_back_to_default_not_error() {
        // A non-numeric port is NOT a hard error: split_authority parses it with
        // unwrap_or(default), so the request still resolves (then the allowlist
        // decides). Pin this real behavior so a future "400 on bad port" change
        // is a conscious one.
        assert_eq!(
            split_authority("host.example:notaport", 80),
            ("host.example".to_string(), 80)
        );
        assert_eq!(
            split_authority("host.example:443x", 8080),
            ("host.example".to_string(), 8080)
        );
    }
}
