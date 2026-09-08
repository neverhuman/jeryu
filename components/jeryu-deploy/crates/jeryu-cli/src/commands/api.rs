//! Shared minimal HTTP JSON client for live `jeryu-api` commands.

use std::io::{Read, Write};
use std::net::TcpStream;

use serde_json::Value;

use crate::client::{ClientError, ClientResult};

pub(crate) struct ApiClient {
    endpoint: Endpoint,
}

impl ApiClient {
    pub(crate) fn new(api_url: &str) -> ClientResult<Self> {
        Ok(Self {
            endpoint: Endpoint::parse(api_url)?,
        })
    }

    pub(crate) fn get(&self, path: &str) -> ClientResult<Value> {
        self.request("GET", path, None)
    }

    pub(crate) fn post(&self, path: &str, body: Value) -> ClientResult<Value> {
        self.request("POST", path, Some(body))
    }

    pub(crate) fn put(&self, path: &str, body: Value) -> ClientResult<Value> {
        self.request("PUT", path, Some(body))
    }

    fn request(&self, method: &str, path: &str, body: Option<Value>) -> ClientResult<Value> {
        let body_text = body.map(|value| value.to_string()).unwrap_or_default();
        let mut stream = TcpStream::connect((self.endpoint.host.as_str(), self.endpoint.port))
            .map_err(|err| {
                ClientError::NotWired(format!("connect {}: {err}", self.endpoint.origin()))
            })?;
        let full_path = format!("{}{}", self.endpoint.base_path, path);
        let request = if body_text.is_empty() {
            format!(
                "{method} {full_path} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
                self.endpoint.host_header()
            )
        } else {
            format!(
                "{method} {full_path} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                self.endpoint.host_header(),
                body_text.len(),
                body_text
            )
        };
        stream
            .write_all(request.as_bytes())
            .map_err(|err| ClientError::NotWired(format!("write request: {err}")))?;
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|err| ClientError::NotWired(format!("read response: {err}")))?;
        let (head, body) = response
            .split_once("\r\n\r\n")
            .ok_or_else(|| ClientError::Invalid("malformed HTTP response".to_string()))?;
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .unwrap_or(0);
        let value = if body.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(body)
                .map_err(|err| ClientError::Invalid(format!("parse JSON response: {err}")))?
        };
        if !(200..300).contains(&status) {
            return Err(ClientError::Conflict(format!(
                "HTTP {status}: {}",
                value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or(body.trim())
            )));
        }
        Ok(value)
    }
}

struct Endpoint {
    host: String,
    port: u16,
    base_path: String,
}

impl Endpoint {
    fn parse(raw: &str) -> ClientResult<Self> {
        let rest = raw.strip_prefix("http://").ok_or_else(|| {
            ClientError::Invalid("only http:// API URLs are supported".to_string())
        })?;
        let (authority, base_path) = rest.split_once('/').unwrap_or((rest, ""));
        let (host, port) = authority
            .rsplit_once(':')
            .and_then(|(host, port)| port.parse::<u16>().ok().map(|port| (host, port)))
            .unwrap_or((authority, 80));
        if host.is_empty() {
            return Err(ClientError::Invalid("API URL host is empty".to_string()));
        }
        Ok(Self {
            host: host.to_string(),
            port,
            base_path: if base_path.is_empty() {
                String::new()
            } else {
                format!("/{base_path}")
            },
        })
    }

    fn host_header(&self) -> String {
        if self.port == 80 {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    fn origin(&self) -> String {
        format!("http://{}", self.host_header())
    }
}

#[cfg(test)]
mod tests {
    use super::Endpoint;

    #[test]
    fn parses_loopback_url_with_base_path() {
        let endpoint = Endpoint::parse("http://127.0.0.1:8787/jeryu").unwrap();
        assert_eq!(endpoint.host, "127.0.0.1");
        assert_eq!(endpoint.port, 8787);
        assert_eq!(endpoint.base_path, "/jeryu");
        assert_eq!(endpoint.host_header(), "127.0.0.1:8787");
    }

    #[test]
    fn rejects_non_http_urls() {
        assert!(Endpoint::parse("https://127.0.0.1:8787").is_err());
    }
}
