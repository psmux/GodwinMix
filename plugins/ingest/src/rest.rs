//! Calling the core back, over its own public REST layer.
//!
//! `add_publishers` has to add and remove sources, and the only method a plugin
//! has for that is the core's ordinary control surface: `GMX_RPC` names the
//! WebSocket, and `GMX_TOKEN` is the token scoped to this plugin. The REST
//! transform in the plugin architecture (section 6) maps `source.add` onto
//! `POST /api/v1/sources` and `source.remove` onto `DELETE /api/v1/sources/{id}`
//! on the same host and port, so a plain HTTP request reaches the same code
//! with no WebSocket client in the tree.
//!
//! Forty lines of HTTP/1.1 over `std::net::TcpStream` is the whole client. It
//! handles `http://` only; an operator who has put the control surface behind
//! TLS gets a message saying so rather than a dependency on a TLS stack inside
//! every plugin.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// A tiny client for one core's REST layer.
///
/// `Debug` never prints the token: a struct that carries a credential and
/// derives Debug ends up in a log line sooner or later.
pub struct Core {
    host: String,
    port: u16,
    token: String,
}

impl Core {
    /// Build one from the environment the core gave this process.
    ///
    /// `GMX_RPC` is `ws://host:port/rpc`. Everything else about the address is
    /// the same, so the base is derived rather than configured.
    pub fn from_env(rpc: &str, token: &str) -> Result<Core, String> {
        if rpc.trim().is_empty() {
            return Err(
                "GMX_RPC is empty, so there is no control surface to call. An embedded \
                 core sets no address; run the mixer as a server (godwinmix --config ...) \
                 and the plugin gets one."
                    .into(),
            );
        }
        let rest = rpc
            .strip_prefix("ws://")
            .ok_or_else(|| {
                format!(
                    "GMX_RPC is '{rpc}'. This plugin speaks plain HTTP to the core's REST \
                     layer and cannot do TLS: a plugin carrying a TLS stack to talk to a \
                     process on the same machine is not worth the weight. Bind the control \
                     surface on loopback without TLS, or add the sources by hand."
                )
            })?
            .trim_end_matches("/rpc");
        let (host, port) = rest.rsplit_once(':').ok_or_else(|| {
            format!("GMX_RPC is '{rpc}', which carries no port. Expected ws://host:port/rpc.")
        })?;
        let port: u16 = port
            .parse()
            .map_err(|_| format!("GMX_RPC is '{rpc}': '{port}' is not a port number."))?;
        Ok(Core {
            host: host.to_string(),
            port,
            token: token.to_string(),
        })
    }

    /// `source.add`, through `POST /api/v1/sources`.
    pub fn add_source(&self, body: &serde_json::Value) -> Result<String, String> {
        self.request("POST", "/api/v1/sources", Some(&body.to_string()))
    }

    /// `source.remove`, through `DELETE /api/v1/sources/{id}`.
    pub fn remove_source(&self, id: &str) -> Result<String, String> {
        self.request("DELETE", &format!("/api/v1/sources/{id}"), None)
    }

    fn request(&self, method: &str, path: &str, body: Option<&str>) -> Result<String, String> {
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|e| format!("could not reach the core at {}:{}: {e}", self.host, self.port))?;
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

        let body = body.unwrap_or("");
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\
             Accept: application/json\r\n",
            self.host, self.port
        );
        if !self.token.is_empty() {
            request.push_str(&format!("Authorization: Bearer {}\r\n", self.token));
        }
        if !body.is_empty() {
            request.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body.len()
            ));
        }
        request.push_str("\r\n");
        request.push_str(body);

        stream
            .write_all(request.as_bytes())
            .map_err(|e| format!("could not send {method} {path} to the core: {e}"))?;
        let mut answer = String::new();
        stream
            .read_to_string(&mut answer)
            .map_err(|e| format!("the core stopped answering {method} {path}: {e}"))?;
        split(&answer, method, path)
    }
}

/// Split an HTTP answer into the status and the body, and refuse a bad status.
fn split(answer: &str, method: &str, path: &str) -> Result<String, String> {
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .ok_or_else(|| format!("the core's answer to {method} {path} had no body"))?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("");
    if status.starts_with('2') {
        return Ok(body.to_string());
    }
    if status == "401" || status == "403" {
        return Err(format!(
            "the core refused {method} {path} with {status}. GMX_TOKEN is scoped to this \
             plugin and adding a source needs the 'operate' scope; issue the plugin a \
             token that has it, or add the sources by hand."
        ));
    }
    Err(format!(
        "the core answered {status} to {method} {path}: {}",
        body.trim().chars().take(300).collect::<String>()
    ))
}

impl std::fmt::Debug for Core {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Core {{ host: {}, port: {}, token: {} }}",
            self.host,
            self.port,
            if self.token.is_empty() { "<none>" } else { "<set>" }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_token() {
        let core = Core::from_env("ws://127.0.0.1:8080/rpc", "super-secret").expect("address");
        assert!(!format!("{core:?}").contains("super-secret"));
    }

    #[test]
    fn the_rest_address_is_derived_from_the_websocket_one() {
        let core = Core::from_env("ws://127.0.0.1:8080/rpc", "t").expect("a normal address");
        assert_eq!(core.host, "127.0.0.1");
        assert_eq!(core.port, 8080);
    }

    #[test]
    fn an_empty_rpc_address_says_why_there_is_none() {
        let err = Core::from_env("", "t").unwrap_err();
        assert!(err.contains("embedded core"), "{err}");
    }

    #[test]
    fn a_tls_address_is_refused_with_the_reason_rather_than_a_tls_stack() {
        let err = Core::from_env("wss://example.com:443/rpc", "t").unwrap_err();
        assert!(err.contains("TLS"), "{err}");
        assert!(err.contains("loopback"), "{err}");
    }

    #[test]
    fn an_address_with_no_port_is_named() {
        let err = Core::from_env("ws://example.com/rpc", "t").unwrap_err();
        assert!(err.contains("no port"), "{err}");
    }

    #[test]
    fn a_two_hundred_answer_yields_its_body() {
        let answer = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"id\":\"cam1\"}";
        assert_eq!(split(answer, "POST", "/x").unwrap(), "{\"id\":\"cam1\"}");
    }

    #[test]
    fn a_refusal_names_the_scope_the_token_needs() {
        let answer = "HTTP/1.1 403 Forbidden\r\n\r\nno";
        let err = split(answer, "POST", "/api/v1/sources").unwrap_err();
        assert!(err.contains("operate"), "{err}");
    }

    #[test]
    fn another_failure_carries_the_status_and_the_start_of_the_body() {
        let answer = "HTTP/1.1 409 Conflict\r\n\r\nthere is already a source called cam1";
        let err = split(answer, "POST", "/api/v1/sources").unwrap_err();
        assert!(err.contains("409"), "{err}");
        assert!(err.contains("already a source"), "{err}");
    }
}
