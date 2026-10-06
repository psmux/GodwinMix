//! What a listener address says: where to listen, and the query values
//! `srtsrc` used to read from it.

use std::net::SocketAddr;

/// Where `srt://host:port?...` listens; no host is every interface.
pub fn address(uri: &str) -> Result<SocketAddr, String> {
    let rest = uri.trim_start_matches("srt://").split(['?', '/']).next().unwrap_or("");
    let (host, port) = rest.rsplit_once(':').unwrap_or((rest, ""));
    let host = if host.is_empty() || host == "@" { "0.0.0.0" } else { host };
    format!("{host}:{port}")
        .parse()
        .map_err(|_| format!("'{uri}' names no address to listen on. Write it as srt://@:9000 or srt://0.0.0.0:9000."))
}

/// One value from the address's query.
pub fn query(uri: &str, key: &str) -> Option<String> {
    let (_, q) = uri.split_once('?')?;
    q.split('&').find_map(|kv| kv.strip_prefix(key)?.strip_prefix('=')).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listener_address_is_read_with_or_without_a_host() {
        assert_eq!(address("srt://:9000?mode=listener").unwrap().to_string(), "0.0.0.0:9000");
        assert_eq!(address("srt://@:9000").unwrap().to_string(), "0.0.0.0:9000");
        assert_eq!(address("srt://10.0.0.2:9000?mode=listener").unwrap().to_string(), "10.0.0.2:9000");
        assert!(address("srt://:nine").unwrap_err().contains("srt://@:9000"));
        assert_eq!(query("srt://:9000?mode=listener&latency=200", "latency").as_deref(), Some("200"));
        assert_eq!(query("srt://:9000?mode=listener", "passphrase"), None);
    }
}
