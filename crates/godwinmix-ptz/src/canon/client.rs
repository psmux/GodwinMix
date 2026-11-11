//! One request to a Canon camera's XC web API: the login, the 30 ms gap the
//! protocol asks for between requests, and the reading of what comes back.
//!
//! The answer is plain text, one `name:=value` per line (`==` when the value
//! did not change, `==?` when the camera rejected it in its current mode), and
//! the outcome is the `Livescope-Status` header, 0 for success. HTTP 200 alone
//! does not mean the camera did it.

use std::time::{Duration, Instant};

use super::auth::{authorization, challenge, Challenge};
use crate::error::{Code, Error, Result};
use crate::http::{request, split, Answer};

pub struct Client {
    pub base: String,
    user: String,
    password: String,
    timeout: Duration,
    challenge: Option<Challenge>,
    nc: u32,
    last: Option<Instant>,
}

/// The XC protocol asks for 30 ms or more between control requests.
const GAP: Duration = Duration::from_millis(30);

impl Client {
    /// `host` is `192.0.2.20` or `192.0.2.20:8080`.
    pub fn new(host: &str, user: &str, password: &str, timeout: Duration) -> Client {
        Client { base: format!("http://{host}/-wvhttp-01-/"), user: user.into(), password: password.into(), timeout, challenge: None, nc: 0, last: None }
    }

    /// GET `path` (`control.cgi?pan=stop`) and answer its `name`, `value` pairs.
    pub fn get(&mut self, path: &str) -> Result<Vec<(String, String)>> {
        if let Some(wait) = self.last.map(|t| GAP.saturating_sub(t.elapsed())) {
            std::thread::sleep(wait);
        }
        let url = format!("{}{path}", self.base);
        let mut answer = self.send(&url)?;
        if answer.status == 401 && !self.user.is_empty() {
            self.challenge = challenge(&answer.headers);
            if self.challenge.is_some() {
                answer = self.send(&url)?;
            }
        }
        self.last = Some(Instant::now());
        check(&url, &answer)?;
        Ok(parse(&answer.body))
    }

    fn send(&mut self, url: &str) -> Result<Answer> {
        let mut headers = Vec::new();
        let auth;
        if let Some(c) = &self.challenge {
            self.nc += 1;
            let uri = split(url).map(|(_, _, p)| p).unwrap_or_default();
            let cnonce = format!("{:016x}", self.nc as u64 ^ 0x9e37_79b9_7f4a_7c15);
            auth = authorization(c, &self.user, &self.password, "GET", &uri, self.nc, &cnonce);
            headers.push(("Authorization", auth.as_str()));
        }
        request("GET", url, &headers, "", self.timeout).map_err(|f| {
            Error::new(Code::Unreachable, format!("cannot reach {url}: {f:?}. Check the address and that the camera is on; its HTTP port is 80 unless changed in [System] > [Server]."))
        })
    }
}

/// The HTTP status and the `Livescope-Status` header, as an error that says
/// what to do.
fn check(url: &str, a: &Answer) -> Result<()> {
    if a.status == 401 {
        return Err(Error::new(Code::Unauthorized, format!("{url} wants a login, or refused the one given. Set the camera's user and password, or give the guest account camera control in [User Authority].")));
    }
    if a.status != 200 {
        return Err(Error::new(Code::Refused, format!("{url} answered HTTP {}. The camera may not have this page; check its firmware is current.", a.status)));
    }
    let status = a.headers.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim().eq_ignore_ascii_case("livescope-status").then(|| v.trim().to_string())
    });
    let code = status.as_deref().and_then(|s| s.split_whitespace().next()).and_then(|s| s.parse::<u16>().ok()).unwrap_or(0);
    if code == 0 {
        return Ok(());
    }
    let (kind, why) = match code {
        301 => (Code::Refused, "another client holds control of the camera. Close the other controller or wait for it to let go"),
        401 => (Code::Unsupported, "the camera does not know this command. Its firmware may be older than the XC protocol this driver follows"),
        403 => (Code::BadArgument, "a value is out of the camera's range"),
        406 => (Code::BadArgument, "a parameter is missing"),
        503 => (Code::Refused, "too many clients are connected. Close a browser tab or another controller"),
        507 => (Code::Unauthorized, "this user has no camera control rights. Give it [Camera Control] in [User Authority]"),
        509 => (Code::Refused, "the camera is in standby. Wake it from its web page or with its power command"),
        _ => (Code::Refused, "the camera refused the request"),
    };
    Err(Error::new(kind, format!("{url}: {why} (Livescope-Status {code}).")))
}

/// `name:=value`, `name==value` and `name=value` lines as pairs. A rejected
/// value reads as `?`.
pub fn parse(body: &str) -> Vec<(String, String)> {
    body.lines()
        .filter_map(|l| {
            let at = l.find('=')?;
            let name = l[..at].trim_end_matches(':').trim();
            let value = l[at + 1..].trim_start_matches('=').trim();
            (!name.is_empty()).then(|| (name.to_string(), value.to_string()))
        })
        .collect()
}

/// The first value named `name`.
pub fn value<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}
