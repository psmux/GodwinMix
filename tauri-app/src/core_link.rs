//! Talking to a core over HTTP: what it is, and asking it to stop.
//!
//! Two requests, and the shell makes no others. The operator UI does all the
//! real work over the same public API from the page the core serves, which is
//! the point of keeping this shell thin.

use std::time::Duration;

use serde::Serialize;

/// Stands in for a version, for a core old enough to have no endpoint that
/// reports one. It goes in the title bar and the status line, so it has to
/// read as a sentence and not as a missing value.
const UNREPORTED: &str = "(version not reported)";

/// Where a core is and how to prove we may talk to it.
#[derive(Clone, Debug)]
pub struct Target {
    /// Base URL with no trailing slash, for example `http://127.0.0.1:53211`.
    pub base: String,
    /// Empty when the core has no token configured.
    pub token: String,
}

impl Target {
    pub fn new(base: impl Into<String>, token: impl Into<String>) -> Self {
        Self { base: base.into().trim_end_matches('/').to_string(), token: token.into() }
    }

    /// The address to point a window at. The token rides in the query because
    /// that is the one channel a page on another origin can be handed it
    /// through, and the core accepts it there on a GET. The window's
    /// initialisation script lifts it out of the URL and puts it where the UI
    /// looks, so it is gone from the address bar before the page draws.
    pub fn page_url(&self) -> String {
        if self.token.is_empty() {
            format!("{}/", self.base)
        } else {
            format!("{}/?token={}", self.base, urlencode(&self.token))
        }
    }
}

/// What a core says it is. Sent to the connect page and put in the title bar.
#[derive(Clone, Debug, Serialize)]
pub struct CoreInfo {
    /// "GodwinMix" from the core, or a plain fallback for an older one.
    pub name: String,
    /// The core's version, or a note in its place when the core predates the
    /// endpoint that reports one.
    pub version: String,
    /// "the mixer on this computer" or the address, for the status line.
    pub label: String,
    /// Where to send the window.
    pub url: String,
}

/// The HTTP client the whole shell shares. Short timeouts: every call here is
/// to a core that is either on this machine or on a link the operator is
/// waiting on, and a hang with no message is the worst outcome.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .user_agent(crate::USER_AGENT)
        .build()
        .expect("a plain HTTP client always builds")
}

/// Ask a core what it is.
///
/// `/api/v1/core/info` is the current answer. A core built before that
/// endpoint existed still answers `/api/status`, which proves it is there and
/// carries no version, so that is what we say. Anything else is an error the
/// operator can act on: a 401 means the token is wrong, a refused connection
/// means nothing is listening.
pub async fn info(http: &reqwest::Client, target: &Target, label: &str) -> Result<CoreInfo, String> {
    match ask(http, target, "/api/v1/core/info").await {
        Ok(body) => Ok(describe(&body, target, label)),
        Err(Trouble::Absent) => {
            ask(http, target, "/api/status").await.map_err(|e| e.say(&target.base)).map(|_| CoreInfo {
                name: "GodwinMix".into(),
                version: UNREPORTED.into(),
                label: label.into(),
                url: target.page_url(),
            })
        }
        Err(e) => Err(e.say(&target.base)),
    }
}

/// Read the name and version out of whatever `/api/v1/core/info` returned.
/// The endpoint is another agent's to shape, so this takes the two fields it
/// needs, looks one level down for them, and does not mind the rest.
fn describe(body: &serde_json::Value, target: &Target, label: &str) -> CoreInfo {
    let field = |key: &str| -> Option<String> {
        body.get(key)
            .or_else(|| body.get("core").and_then(|c| c.get(key)))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    CoreInfo {
        name: field("name").unwrap_or_else(|| "GodwinMix".into()),
        version: field("version").unwrap_or_else(|| UNREPORTED.into()),
        label: label.into(),
        url: target.page_url(),
    }
}

/// Ask a core to shut down. Used by "Quit and stop the mixer" and when the
/// app quits with a mixer of its own running.
pub async fn shutdown(http: &reqwest::Client, target: &Target) -> Result<(), String> {
    // Shorter than the client's own timeout. This request is made on the way
    // out, sometimes with the operating system counting how long the app is
    // taking to go, and a mixer that has not answered in three seconds is not
    // going to answer: it gets killed instead.
    let mut req = http
        .post(format!("{}/api/shutdown", target.base))
        .timeout(Duration::from_secs(3));
    if !target.token.is_empty() {
        req = req.bearer_auth(&target.token);
    }
    match req.send().await {
        Ok(r) if r.status().is_success() => Ok(()),
        Ok(r) => Err(format!("the mixer answered {} when asked to stop", r.status())),
        Err(e) => Err(plain(&e)),
    }
}

enum Trouble {
    /// A 404: something is listening, but not on that path.
    Absent,
    Unauthorised,
    Status(reqwest::StatusCode),
    Unreachable(String),
}

impl Trouble {
    fn say(self, base: &str) -> String {
        match self {
            Trouble::Absent => format!("{base} answered, but it is not a GodwinMix mixer."),
            Trouble::Unauthorised => {
                format!("{base} needs a token, and the one given was not accepted.")
            }
            Trouble::Status(s) => format!("{base} answered {s}."),
            Trouble::Unreachable(why) => format!("Could not reach {base}. {why}"),
        }
    }
}

async fn ask(
    http: &reqwest::Client,
    target: &Target,
    path: &str,
) -> Result<serde_json::Value, Trouble> {
    let mut req = http.get(format!("{}{path}", target.base));
    if !target.token.is_empty() {
        req = req.bearer_auth(&target.token);
    }
    let reply = req.send().await.map_err(|e| Trouble::Unreachable(plain(&e)))?;
    match reply.status() {
        s if s.is_success() => reply.json().await.map_err(|_| Trouble::Absent),
        reqwest::StatusCode::NOT_FOUND => Err(Trouble::Absent),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            Err(Trouble::Unauthorised)
        }
        s => Err(Trouble::Status(s)),
    }
}

/// reqwest's own message names the crate and the URL twice. The operator gets
/// the part that tells them what to do about it.
fn plain(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "It did not answer in time.".into()
    } else if e.is_connect() {
        "Nothing is listening there.".into()
    } else if e.is_builder() {
        "That address is not one this app can use.".into()
    } else {
        e.to_string()
    }
}

/// Percent encoding for the one place the shell needs it. A token is hex
/// today, but a remote core's token is whatever its operator typed.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// Turn what the operator typed into a base URL. They will type
/// `studio.local:8080`, or paste a URL with a path and a trailing slash, and
/// all of it has to land on the same place.
pub fn normalise(typed: &str) -> Result<String, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err("Type the address of the mixer, for example studio.local:8080.".into());
    }
    let with_scheme = if typed.contains("://") { typed.to_string() } else { format!("http://{typed}") };
    let url = reqwest::Url::parse(&with_scheme)
        .map_err(|_| format!("{typed} is not an address this app can use."))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("{typed} is not http or https."));
    }
    if url.host_str().is_none() {
        return Err(format!("{typed} has no host in it."));
    }
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    Ok(format!("{}://{}{port}", url.scheme(), url.host_str().unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_host_and_port_becomes_a_url() {
        assert_eq!(normalise("studio.local:8080").unwrap(), "http://studio.local:8080");
        assert_eq!(normalise(" 10.0.0.4 ").unwrap(), "http://10.0.0.4");
    }

    #[test]
    fn a_pasted_url_loses_its_path_and_slash() {
        assert_eq!(normalise("https://mix.example/ui/index.html").unwrap(), "https://mix.example");
        assert_eq!(normalise("http://127.0.0.1:8080/").unwrap(), "http://127.0.0.1:8080");
    }

    #[test]
    fn nonsense_is_refused_with_a_sentence() {
        assert!(normalise("").unwrap_err().contains("studio.local"));
        assert!(normalise("ftp://box").unwrap_err().contains("http"));
    }

    #[test]
    fn the_page_url_carries_the_token_only_when_there_is_one() {
        let open = Target::new("http://127.0.0.1:9000/", "");
        assert_eq!(open.page_url(), "http://127.0.0.1:9000/");
        let shut = Target::new("http://127.0.0.1:9000", "a b/c");
        assert_eq!(shut.page_url(), "http://127.0.0.1:9000/?token=a%20b%2Fc");
    }

    #[test]
    fn core_info_is_read_from_the_top_level_or_one_below() {
        let target = Target::new("http://x", "");
        let flat = serde_json::json!({"name": "GodwinMix", "version": "0.3.0"});
        assert_eq!(describe(&flat, &target, "here").version, "0.3.0");
        let nested = serde_json::json!({"core": {"name": "GodwinMix", "version": "0.4.1"}});
        assert_eq!(describe(&nested, &target, "here").version, "0.4.1");
        let neither = serde_json::json!({"uptime_secs": 3});
        assert_eq!(describe(&neither, &target, "here").name, "GodwinMix");
    }
}
