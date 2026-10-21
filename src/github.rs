//! GraphQL transports: live HTTPS, recording (for offline fixtures) and replay.

use crate::Error;
use serde_json::{json, Value};
use std::time::Duration;

pub const API_URL: &str = "https://api.github.com/graphql";
const MAX_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;

/// One GraphQL exchange: `name` identifies the query, `variables` must match exactly.
pub trait Transport {
    fn call(&mut self, name: &str, query: &str, variables: &Value) -> Result<Value, Error>;
}

/// Serves recorded responses in order; any deviation from the recording is an error.
pub struct ReplayTransport {
    exchanges: Vec<Value>,
    position: usize,
}

impl ReplayTransport {
    pub fn new(exchanges: Vec<Value>) -> Self {
        ReplayTransport {
            exchanges,
            position: 0,
        }
    }

    pub fn assert_consumed(&self) -> Result<(), Error> {
        if self.position == self.exchanges.len() {
            Ok(())
        } else {
            Err(Error::github(format!(
                "replay has {} unused exchanges",
                self.exchanges.len() - self.position
            )))
        }
    }
}

impl Transport for ReplayTransport {
    fn call(&mut self, name: &str, _query: &str, variables: &Value) -> Result<Value, Error> {
        let Some(expected) = self.exchanges.get(self.position) else {
            return Err(Error::github(format!(
                "replay exhausted at {name} {variables}"
            )));
        };
        if expected["name"] != name || &expected["variables"] != variables {
            return Err(Error::github(format!(
                "replay mismatch: expected {} {}, got {name} {variables}",
                expected["name"], expected["variables"]
            )));
        }
        self.position += 1;
        Ok(expected["response"].clone())
    }
}

/// Sends read-only GraphQL queries over HTTPS (rustls, bundled webpki roots).
/// The token is sent only in the Authorization header and never logged or returned.
pub struct LiveTransport {
    token: String,
    url: String,
    agent: ureq::Agent,
}

impl LiveTransport {
    /// Token from `GH_TOKEN`, then `GITHUB_TOKEN`.
    pub fn from_env() -> Result<Self, Error> {
        let token = ["GH_TOKEN", "GITHUB_TOKEN"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find(|t| !t.is_empty())
            .ok_or_else(|| {
                Error::github(
                    "no token: set GH_TOKEN or GITHUB_TOKEN (read-only scope is sufficient)",
                )
            })?;
        Ok(Self::new(token, API_URL))
    }

    pub fn new(token: impl Into<String>, url: impl Into<String>) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            // Never follow redirects: the Authorization header must only reach the API host.
            .max_redirects(0)
            .https_only(true)
            .user_agent("gitmapsgame")
            .build()
            .into();
        LiveTransport {
            token: token.into(),
            url: url.into(),
            agent,
        }
    }
}

impl Transport for LiveTransport {
    fn call(&mut self, name: &str, query: &str, variables: &Value) -> Result<Value, Error> {
        let body = serde_json::to_vec(&json!({"query": query, "variables": variables}))
            .map_err(|e| Error::value(format!("cannot encode {name} request: {e}")))?;
        let mut response = self
            .agent
            .post(&self.url)
            .header("Authorization", &format!("bearer {}", self.token))
            .header("Content-Type", "application/json")
            .send(&body[..])
            .map_err(|e| Error::github(format!("GitHub GraphQL unreachable for {name}: {e}")))?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(Error::github(format!(
                "GitHub GraphQL HTTP {status} for {name}"
            )));
        }
        let data = response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_vec()
            .map_err(|e| {
                Error::github(format!(
                    "GitHub GraphQL response unreadable for {name}: {e}"
                ))
            })?;
        serde_json::from_slice(&data).map_err(|e| {
            Error::value(format!(
                "GitHub GraphQL returned invalid JSON for {name}: {e}"
            ))
        })
    }
}

/// Wraps another transport and keeps an untouched copy of every exchange for offline replay.
pub struct RecordingTransport<T: Transport> {
    inner: T,
    redact: Option<fn(&str, Value) -> Value>,
    pub exchanges: Vec<Value>,
}

impl<T: Transport> RecordingTransport<T> {
    pub fn new(inner: T, redact: Option<fn(&str, Value) -> Value>) -> Self {
        RecordingTransport {
            inner,
            redact,
            exchanges: Vec::new(),
        }
    }
}

impl<T: Transport> Transport for RecordingTransport<T> {
    fn call(&mut self, name: &str, query: &str, variables: &Value) -> Result<Value, Error> {
        let mut response = self.inner.call(name, query, variables)?;
        if let Some(redact) = self.redact {
            response = redact(name, response);
        }
        self.exchanges
            .push(json!({"name": name, "variables": variables, "response": response.clone()}));
        Ok(response)
    }
}

/// For third-party fixtures: blank every issue `body` (and so any Requires lines) before recording.
/// Signature matches [`RecordingTransport`]'s redact hook; the query name is not needed.
pub fn redact_bodies(_name: &str, response: Value) -> Value {
    blank_bodies(response)
}

fn blank_bodies(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let v = if k == "body" {
                        Value::String(String::new())
                    } else {
                        blank_bodies(v)
                    };
                    (k, v)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(blank_bodies).collect()),
        other => other,
    }
}
