//! The link a guest is sent, and the same as a QR code.
//!
//! `https://<host>:<port>/join/#guest=<id>&key=<key>&title=<name>`, with
//! `&show=<id>` under a station so the page reaches this show. Everything
//! is in the fragment, which a browser never sends to a server. The host is
//! the first https address of this machine that is not loopback, as the
//! phone camera code uses, unless the request named a `base`.

use super::store::Record;
use super::Guests;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::guests::InviteLink;

impl Guests {
    pub(super) fn link_of(&self, r: &Record, base: Option<&str>) -> Result<InviteLink, RpcError> {
        let (base, note) = match base.map(str::trim).filter(|b| !b.is_empty()) {
            Some(b) if b.starts_with("https://") || b.starts_with("http://") => (b.trim_end_matches('/').to_string(), None),
            Some(b) => {
                let msg = format!("base is '{b}'; it is the address the link starts with, such as https://studio.example.com. Leave it out for this machine's own.");
                return Err(RpcError::invalid_params(msg).with("field", "base"));
            }
            None => this_machine(),
        };
        let show = crate::station::show::mode().map(|m| format!("&show={}", m.id)).unwrap_or_default();
        let link = format!("{base}/join/#guest={}&key={}&title={}{show}", r.id, r.key, escape(&r.name));
        let qr_svg = super::qr::svg(&link).unwrap_or_default();
        Ok(InviteLink { id: r.id.clone(), link, qr_svg, note })
    }

    /// The link for an invite that exists, for the desk to show again.
    pub fn link(&self, id: &str) -> Result<InviteLink, RpcError> {
        let Some(r) = self.invites.lock().get(id).cloned() else { return Err(self.no_invite(id)) };
        self.link_of(&r, None)
    }
}

/// The first https address that another device can open, or why there is none.
fn this_machine() -> (String, Option<String>) {
    let Some(info) = crate::tls::info() else {
        let why = "HTTPS is off on this mixer, and a browser on another device gives a page its camera only over https. Turn it on with [control.tls] enabled = true and restart, or pass base with an https address that reaches this mixer.";
        return (String::new(), Some(why.into()));
    };
    let lan = info.urls.iter().find(|u| !is_loopback(u));
    match lan {
        Some(u) => (u.trim_end_matches('/').to_string(), None),
        None => {
            let first = info.urls.first().map(|u| u.trim_end_matches('/').to_string()).unwrap_or_default();
            let why = "This mixer only listens on this computer, so a guest on another device cannot open the link. Set [control] bind = \"0.0.0.0:8080\" and restart, or in the desktop app switch on Let other devices on this network connect.";
            (first, Some(why.into()))
        }
    }
}

fn is_loopback(url: &str) -> bool {
    let host = url.trim_start_matches("https://").split([':', '/']).next().unwrap_or("");
    host == "localhost" || host.starts_with("127.") || url.contains("://[::1]")
}

/// A name as it goes in the fragment: letters and digits as they are, the
/// rest percent encoded.
fn escape(text: &str) -> String {
    text.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_escaped_and_loopback_is_told_apart() {
        assert_eq!(escape("Ana López"), "Ana%20L%C3%B3pez");
        assert!(is_loopback("https://localhost:8080/"));
        assert!(is_loopback("https://127.0.0.1:8080/"));
        assert!(is_loopback("https://[::1]:8080/"));
        assert!(!is_loopback("https://192.168.1.20:8080/"));
        assert!(!is_loopback("https://studio.local:8443/"));
    }
}
