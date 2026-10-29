//! Signed links: a viewer link that stops working at a time, and can be held
//! to one address.
//!
//! `?exp=<unix seconds>&sig=<signature>`, and `&ip=<address>` when the link
//! is for one viewer. The signature is an HMAC of the output, the expiry and
//! the address under the output's own viewer key, so nobody without that key
//! can make one, a link handed to a paying viewer can be let run out, and
//! changing the viewer key (a new output name, or `params.viewer_key`) ends
//! every signed link at once. Nothing is stored: the link carries all of it.

use ring::hmac;
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

const LEN: usize = 26;
const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";

/// The longest a signed link may live: thirty days.
pub const MAX_TTL_SECS: u64 = 30 * 24 * 3600;

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The signature for `output` until `exp`, for `ip` when the link is one
/// viewer's.
pub fn sign(viewer_key: &str, output: &str, exp: u64, ip: Option<&str>) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, viewer_key.as_bytes());
    let text = format!("godwinmix hls signed link\0{output}\0{exp}\0{}", ip.unwrap_or(""));
    hmac::sign(&key, text.as_bytes()).as_ref().iter().take(LEN).map(|b| ALPHABET[(b & 31) as usize] as char).collect()
}

/// Whether a request carrying `exp`, `sig` and maybe `ip`, from `peer`, may
/// read `output` at `now`.
pub fn verify(viewer_key: &str, output: &str, exp: &str, sig: &str, ip: Option<&str>, peer: Option<IpAddr>, now: u64) -> bool {
    let Ok(exp) = exp.parse::<u64>() else { return false };
    if viewer_key.is_empty() || exp < now {
        return false;
    }
    if let Some(bound) = ip {
        let Ok(bound) = bound.parse::<IpAddr>() else { return false };
        if peer != Some(bound) {
            return false;
        }
    }
    let want = sign(viewer_key, output, exp, ip);
    want.len() == sig.len() && want.bytes().zip(sig.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0
}

/// The query a signed link carries, without the leading `?`, and when it
/// runs out.
pub fn query(viewer_key: &str, output: &str, ttl_secs: u64, ip: Option<&str>) -> (String, u64) {
    let exp = now_secs() + ttl_secs.clamp(1, MAX_TTL_SECS);
    let sig = sign(viewer_key, output, exp, ip);
    let q = match ip {
        Some(ip) => format!("exp={exp}&sig={sig}&ip={ip}"),
        None => format!("exp={exp}&sig={sig}"),
    };
    (q, exp)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "a-viewer-key-of-24-chars";

    #[test]
    fn a_link_is_good_until_it_runs_out() {
        let sig = sign(KEY, "viewers", 1000, None);
        assert!(verify(KEY, "viewers", "1000", &sig, None, None, 999));
        assert!(verify(KEY, "viewers", "1000", &sig, None, None, 1000));
        assert!(!verify(KEY, "viewers", "1000", &sig, None, None, 1001), "run out");
    }

    #[test]
    fn a_link_opens_its_own_output_only_and_cannot_be_stretched() {
        let sig = sign(KEY, "viewers", 1000, None);
        assert!(!verify(KEY, "church", "1000", &sig, None, None, 1), "another output");
        assert!(!verify(KEY, "viewers", "9999", &sig, None, None, 1), "a later expiry");
        assert!(!verify("another-key-entirely-xxx", "viewers", "1000", &sig, None, None, 1), "a changed viewer key");
    }

    #[test]
    fn a_link_held_to_an_address_opens_from_that_address_only() {
        let sig = sign(KEY, "viewers", 1000, Some("203.0.113.7"));
        let from = |ip: &str| Some(ip.parse().unwrap());
        assert!(verify(KEY, "viewers", "1000", &sig, Some("203.0.113.7"), from("203.0.113.7"), 1));
        assert!(!verify(KEY, "viewers", "1000", &sig, Some("203.0.113.7"), from("198.51.100.1"), 1));
        assert!(!verify(KEY, "viewers", "1000", &sig, None, from("203.0.113.7"), 1), "the address taken off");
    }

    #[test]
    fn the_query_carries_what_verify_reads() {
        let (q, _) = query(KEY, "viewers", 60, None);
        let get = |k: &str| q.split('&').find_map(|p| p.strip_prefix(&format!("{k}="))).unwrap().to_string();
        assert!(verify(KEY, "viewers", &get("exp"), &get("sig"), None, None, now_secs()));
    }
}
