//! The names this machine is reached by: what a self signed certificate is
//! made for, and the https addresses a client is told about.

use std::net::IpAddr;

/// This machine's name, as the operating system has it. None when it has
/// none worth putting in a certificate.
pub fn hostname() -> Option<String> {
    let name = system_hostname()?.trim().trim_end_matches('.').to_ascii_lowercase();
    let usable = !name.is_empty() && name != "localhost" && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    usable.then_some(name)
}

#[cfg(unix)]
fn system_hostname() -> Option<String> {
    let mut buf = [0u8; 256];
    // SAFETY: the buffer is valid for its whole length, and gethostname
    // writes at most that many bytes into it.
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if rc != 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8(buf[..end].to_vec()).ok()
}

/// Windows keeps the NetBIOS name in the environment of every process, which
/// is the name other machines on a home or office network find it by.
#[cfg(not(unix))]
fn system_hostname() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

/// Every name a browser might use to reach the control port at `bind`.
///
/// `localhost` and the loopback addresses, the machine's name and its
/// `.local` form (what mDNS answers on macOS and most Linux desktops), the
/// LAN address the routing table picks, and the bind address itself when it
/// is one particular address.
pub fn for_machine(bind: &str) -> Vec<String> {
    let mut out: Vec<String> = vec!["localhost".into(), "127.0.0.1".into(), "::1".into()];
    if let Some(host) = hostname() {
        if !host.contains('.') {
            out.push(format!("{host}.local"));
        }
        out.push(host);
    }
    out.push(crate::channels::net::first_address());
    if let Some(ip) = bind_ip(bind).filter(|ip| !ip.is_unspecified()) {
        out.push(ip.to_string());
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|n| seen.insert(n.clone()));
    out
}

/// The https addresses worth telling a person about, the LAN one first. A
/// mixer bound to loopback is only reachable from this machine, so it gets
/// only `localhost`.
pub fn urls(bind: &str, names: &[String]) -> Vec<String> {
    let port = bind.rsplit(':').next().unwrap_or("8080");
    let local_only = bind_ip(bind).is_some_and(|ip| ip.is_loopback());
    let mut hosts: Vec<&str> = Vec::new();
    if !local_only {
        let lan = names.iter().filter(|n| n.parse::<IpAddr>().is_ok_and(|ip| !ip.is_loopback()));
        hosts.extend(lan.map(String::as_str));
        hosts.extend(names.iter().filter(|n| n.parse::<IpAddr>().is_err() && *n != "localhost").map(String::as_str));
    }
    hosts.push("localhost");
    hosts
        .into_iter()
        .map(|h| match h.parse::<IpAddr>() {
            Ok(IpAddr::V6(_)) => format!("https://[{h}]:{port}/"),
            _ => format!("https://{h}:{port}/"),
        })
        .collect()
}

/// The address part of a bind string: `0.0.0.0:8080`, `[::1]:8080`.
fn bind_ip(bind: &str) -> Option<IpAddr> {
    bind.parse::<std::net::SocketAddr>().ok().map(|a| a.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_cover_localhost_and_a_particular_bind_address_once_each() {
        let names = for_machine("10.9.8.7:8080");
        for want in ["localhost", "127.0.0.1", "::1", "10.9.8.7"] {
            assert_eq!(names.iter().filter(|n| *n == want).count(), 1, "{want} in {names:?}");
        }
        assert!(!for_machine("0.0.0.0:8080").contains(&"0.0.0.0".to_string()));
    }

    #[test]
    fn the_lan_address_comes_first_and_loopback_binds_get_only_localhost() {
        let names: Vec<String> = ["localhost", "127.0.0.1", "studio.local", "studio", "192.168.1.20"].map(String::from).into();
        let urls = urls("0.0.0.0:8443", &names);
        assert_eq!(urls[0], "https://192.168.1.20:8443/");
        assert!(urls.contains(&"https://studio.local:8443/".to_string()), "{urls:?}");
        assert_eq!(urls.last().unwrap(), "https://localhost:8443/");
        assert_eq!(super::urls("127.0.0.1:8080", &names), vec!["https://localhost:8080/"]);
    }
}
