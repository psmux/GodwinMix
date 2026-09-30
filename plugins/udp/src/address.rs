//! `udp://` and `rtp://` addresses, spelled the way VLC and ffmpeg spell them.
//!
//! | Written | Means |
//! |---|---|
//! | `udp://@239.1.1.1:5000` | join multicast group 239.1.1.1, port 5000 |
//! | `udp://239.1.1.1:5000` | the same; the `@` is optional for a group |
//! | `udp://10.0.0.9@232.1.1.1:5000` | source specific: only what 10.0.0.9 sends to the group |
//! | `udp://0.0.0.0:5000` or `udp://@:5000` | unicast, every interface |
//! | `rtp://@239.1.1.1:5004` | the same forms, for RTP; the plugin tells them apart by itself |
//!
//! Anything after `?` is ignored, so an address copied out of an ffmpeg
//! command line with `?pkt_size=1316` on the end still works.

use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The group to join, or the local address to bind for unicast.
    pub host: String,
    pub port: u16,
    /// Source specific multicast: the one sender to accept.
    pub source: Option<String>,
}

impl Endpoint {
    pub fn multicast(&self) -> bool {
        self.host.parse::<IpAddr>().map(|ip| ip.is_multicast()).unwrap_or(false)
    }

    /// The address as a person would write it back.
    pub fn display(&self, scheme: &str) -> String {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        match (&self.source, self.multicast()) {
            (Some(s), _) => format!("{scheme}://{s}@{host}:{}", self.port),
            (None, true) => format!("{scheme}://@{host}:{}", self.port),
            (None, false) => format!("{scheme}://{host}:{}", self.port),
        }
    }
}

/// Read `udp://...` or `rtp://...`. The error says what a good one looks like.
pub fn parse(uri: &str) -> Result<Endpoint, String> {
    let lower = uri.trim().to_ascii_lowercase();
    let rest = ["udp://", "rtp://"]
        .iter()
        .find_map(|s| lower.strip_prefix(s))
        .ok_or_else(|| {
            format!(
                "'{uri}' is not a udp:// or rtp:// address. Write udp://@239.1.1.1:5000 for \
                 a multicast group or udp://0.0.0.0:5000 to receive on a port."
            )
        })?;
    let rest = rest.split(['?', '/']).next().unwrap_or_default();
    let (source, hostport) = match rest.rsplit_once('@') {
        Some((s, h)) => ((!s.is_empty()).then(|| s.to_string()), h),
        None => (None, rest),
    };
    let (host, port) = split_port(hostport).ok_or_else(|| {
        format!("'{uri}' has no port. Add one after a colon: udp://@239.1.1.1:5000.")
    })?;
    let host = if host.is_empty() { "0.0.0.0".to_string() } else { host };
    Ok(Endpoint { host, port, source })
}

fn split_port(s: &str) -> Option<(String, u16)> {
    let (host, port) = if let Some(v6) = s.strip_prefix('[') {
        let (h, p) = v6.split_once("]:")?;
        (h, p)
    } else {
        s.rsplit_once(':')?
    };
    Some((host.to_string(), port.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vlc_forms_all_read() {
        let group = parse("udp://@239.1.1.1:5000").unwrap();
        assert_eq!((group.host.as_str(), group.port, group.multicast()), ("239.1.1.1", 5000, true));
        assert_eq!(parse("udp://239.1.1.1:5000").unwrap(), group);
        let ssm = parse("udp://10.0.0.9@232.1.1.1:5000").unwrap();
        assert_eq!(ssm.source.as_deref(), Some("10.0.0.9"));
        assert_eq!(parse("udp://@:5000").unwrap().host, "0.0.0.0");
        assert!(!parse("udp://0.0.0.0:5000").unwrap().multicast());
        assert_eq!(parse("RTP://[ff05::1]:5004").unwrap().host, "ff05::1");
    }

    #[test]
    fn an_ffmpeg_query_string_is_ignored() {
        assert_eq!(parse("udp://239.1.1.1:5000?pkt_size=1316").unwrap().port, 5000);
    }

    #[test]
    fn a_bad_address_says_what_a_good_one_looks_like() {
        assert!(parse("http://x").unwrap_err().contains("udp://@239.1.1.1:5000"));
        assert!(parse("udp://239.1.1.1").unwrap_err().contains("no port"));
    }

    #[test]
    fn an_address_writes_back_the_way_it_was_meant() {
        assert_eq!(parse("udp://239.1.1.1:5000").unwrap().display("udp"), "udp://@239.1.1.1:5000");
        let ssm = parse("udp://10.0.0.9@232.1.1.1:5000").unwrap();
        assert_eq!(ssm.display("rtp"), "rtp://10.0.0.9@232.1.1.1:5000");
    }
}
