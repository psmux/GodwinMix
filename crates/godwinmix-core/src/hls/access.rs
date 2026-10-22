//! Two params of an HLS output that say what it serves and to whom:
//! `segments` (CMAF, MPEG-TS or both) and `access` (a viewer key, or open).
//!
//! An open output answers without a key or a token. It is meant for an
//! output behind a gateway that checks a signature of its own, so it
//! answers only a player on loopback or a private network unless
//! `open_public = true`; anyone else still needs the key.

use crate::config::Params;
use anyhow::{bail, Result};
use std::net::IpAddr;

/// Which segments an output makes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Segments {
    /// CMAF, for HLS, LL-HLS and DASH.
    #[default]
    Cmaf,
    /// MPEG-TS alone. The picture is still cut as CMAF, since that is
    /// where the segments are cut, but only the TS playlists are served.
    Ts,
    Both,
}

impl Segments {
    pub fn cmaf(self) -> bool {
        self != Segments::Ts
    }

    pub fn ts(self) -> bool {
        self != Segments::Cmaf
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Segments::Cmaf => "cmaf",
            Segments::Ts => "ts",
            Segments::Both => "both",
        }
    }
}

/// Who may read an output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// The viewer key or a control token with `read`.
    #[default]
    Keyed,
    /// Anyone on loopback or a private network.
    Open,
    /// Anyone at all: `access = "open"` with `open_public = true`.
    OpenPublic,
}

impl Access {
    /// Whether a player at `peer` may read without a key.
    pub fn lets_in(self, peer: Option<IpAddr>) -> bool {
        match self {
            Access::Keyed => false,
            Access::OpenPublic => true,
            Access::Open => peer.is_some_and(is_local),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Access::Keyed => "keyed",
            Access::Open | Access::OpenPublic => "open",
        }
    }
}

/// Loopback, a private network (RFC 1918, RFC 4193), link local, or CGNAT
/// (RFC 6598, which an ISP's own boxes sit behind).
pub fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_loopback() || v.is_private() || v.is_link_local() || (v.octets()[0] == 100 && (64..128).contains(&v.octets()[1])),
        IpAddr::V6(v) => match v.to_ipv4_mapped() {
            Some(v4) => is_local(IpAddr::V4(v4)),
            None => v.is_loopback() || (v.segments()[0] & 0xfe00) == 0xfc00 || (v.segments()[0] & 0xffc0) == 0xfe80,
        },
    }
}

/// Read `segments`, `access` and `open_public` from an output's params.
pub fn from_params(params: &Params) -> Result<(Segments, Access)> {
    let word = |k: &str| -> Result<Option<String>> {
        match params.get(k) {
            None => Ok(None),
            Some(toml::Value::String(s)) => Ok(Some(s.trim().to_ascii_lowercase())),
            Some(v) => bail!("hls/output params.{k} must be a word, not `{v}`"),
        }
    };
    let segments = match word("segments")?.as_deref() {
        None | Some("cmaf") => Segments::Cmaf,
        Some("ts") => Segments::Ts,
        Some("both") => Segments::Both,
        Some(other) => bail!("hls/output params.segments must be cmaf, ts or both, not `{other}`. cmaf is the usual."),
    };
    let public = match params.get("open_public") {
        None => false,
        Some(toml::Value::Boolean(b)) => *b,
        Some(v) => bail!("hls/output params.open_public must be true or false, not `{v}`"),
    };
    let access = match (word("access")?.as_deref(), public) {
        (None | Some("keyed"), false) => Access::Keyed,
        (Some("open"), false) => Access::Open,
        (Some("open"), true) => Access::OpenPublic,
        (None | Some("keyed"), true) => bail!("hls/output params.open_public only goes with access = \"open\"; leave it out for a keyed output."),
        (Some(other), _) => bail!("hls/output params.access must be keyed or open, not `{other}`. keyed is the usual."),
    };
    Ok((segments, access))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_lets_in_loopback_and_private_networks_only_unless_public() {
        let at = |s: &str| Some(s.parse::<IpAddr>().unwrap());
        for local in ["127.0.0.1", "10.1.2.3", "192.168.0.9", "172.16.4.4", "169.254.1.1", "100.64.0.1", "::1", "fd00::1", "fe80::1", "::ffff:192.168.1.2"] {
            assert!(Access::Open.lets_in(at(local)), "{local}");
        }
        for public in ["8.8.8.8", "100.128.0.1", "2001:db8::1", "::ffff:8.8.8.8"] {
            assert!(!Access::Open.lets_in(at(public)), "{public}");
            assert!(Access::OpenPublic.lets_in(at(public)), "{public}");
        }
        assert!(!Access::Open.lets_in(None), "no address: no way in");
        assert!(!Access::Keyed.lets_in(at("127.0.0.1")));
    }

    #[test]
    fn the_words_read_and_a_wrong_one_is_refused_with_the_choices() {
        let p = |t: &str| from_params(&toml::from_str(t).unwrap());
        assert_eq!(p("").unwrap(), (Segments::Cmaf, Access::Keyed));
        assert_eq!(p("segments = \"both\"\naccess = \"open\"").unwrap(), (Segments::Both, Access::Open));
        assert_eq!(p("access = \"open\"\nopen_public = true").unwrap().1, Access::OpenPublic);
        assert!(p("segments = \"mp4\"").unwrap_err().to_string().contains("cmaf, ts or both"));
        assert!(p("open_public = true").unwrap_err().to_string().contains("access = \"open\""));
    }
}
