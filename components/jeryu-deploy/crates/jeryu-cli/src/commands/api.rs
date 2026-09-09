//! Authenticated HTTP JSON transport for live operator commands.

use std::time::Duration;

use reqwest::{Method, Url, blocking::Client, redirect::Policy};
use serde_json::Value;

use crate::client::{ClientError, ClientResult};

pub(crate) struct ApiClient {
    base: Url,
    client: Client,
    token: Option<String>,
}

impl ApiClient {
    pub(crate) fn new(api_url: &str) -> ClientResult<Self> {
        let token = match std::env::var_os("JERYU_TOKEN_FILE") {
            Some(path) => Some(
                std::fs::read_to_string(path)
                    .map_err(|_| ClientError::Invalid("cannot read JERYU_TOKEN_FILE".into()))?
                    .trim()
                    .to_owned(),
            ),
            None => std::env::var("JERYU_TOKEN").ok(),
        };
        Self::with_token(api_url, token)
    }

    fn with_token(api_url: &str, token: Option<String>) -> ClientResult<Self> {
        let base =
            Url::parse(api_url).map_err(|_| ClientError::Invalid("invalid API URL".into()))?;
        if !matches!(base.scheme(), "http" | "https")
            || base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err(ClientError::Invalid(
                "API URL must be HTTP(S), without credentials, query or fragment".into(),
            ));
        }
        if token
            .as_ref()
            .is_some_and(|t| t.is_empty() || t.chars().any(char::is_control))
        {
            return Err(ClientError::Invalid("invalid API token".into()));
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(Policy::none())
            .build()
            .map_err(|_| ClientError::NotWired("cannot initialize HTTP client".into()))?;
        Ok(Self {
            base,
            client,
            token,
        })
    }

    pub(crate) fn get(&self, path: &str) -> ClientResult<Value> {
        self.request(Method::GET, path, None)
    }

    pub(crate) fn post(&self, path: &str, body: Value) -> ClientResult<Value> {
        self.request(Method::POST, path, Some(body))
    }

    pub(crate) fn put(&self, path: &str, body: Value) -> ClientResult<Value> {
        self.request(Method::PUT, path, Some(body))
    }

    fn request(&self, method: Method, path: &str, body: Option<Value>) -> ClientResult<Value> {
        let url = format!("{}{}", self.base.as_str().trim_end_matches('/'), path);
        let mut request = self
            .client
            .request(method, &url)
            .header("Accept", "application/json");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().map_err(|error| {
            ClientError::NotWired(format!("API request failed: {}", error.without_url()))
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(ClientError::Conflict(format!("API returned HTTP {status}")));
        }
        response
            .json()
            .map_err(|_| ClientError::Invalid("API returned invalid JSON".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    #[test]
    fn sends_bearer_auth_and_decodes_chunked_json() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/prefix", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = vec![];
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            assert!(request.starts_with("get /prefix/repos http/1.1"));
            assert!(request.contains("authorization: bearer fixture-token\r\n"));
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n[]\r\n0\r\n\r\n").unwrap();
        });
        assert_eq!(
            ApiClient::with_token(&url, Some("fixture-token".into()))
                .unwrap()
                .get("/repos")
                .unwrap(),
            serde_json::json!([])
        );
        server.join().unwrap();
    }

    #[test]
    fn refuses_credentials_in_url_and_unreachable_server() {
        assert!(ApiClient::with_token("http://user:password@localhost", None).is_err());
        assert!(ApiClient::with_token("https://example.org", None).is_ok());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        assert!(
            ApiClient::with_token(&format!("http://{address}"), None)
                .unwrap()
                .get("/repos")
                .is_err()
        );
    }
}
