//! WS-Discovery: one Probe for ONVIF video transmitters, and the
//! ProbeMatches that come back. This is how every NVR finds cameras on a LAN
//! without anybody typing an address.

use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

/// The group and port every ONVIF device listens on.
pub const MULTICAST: &str = "239.255.255.250:3702";

/// A device that answered: its service addresses and what its scopes say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Device service URLs, `http://10.0.0.64/onvif/device_service`.
    pub xaddrs: Vec<String>,
    pub name: String,
    pub hardware: String,
}

pub fn probe_message(id: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<e:Envelope xmlns:e=\"http://www.w3.org/2003/05/soap-envelope\" xmlns:w=\"http://schemas.xmlsoap.org/ws/2004/08/addressing\" \
xmlns:d=\"http://schemas.xmlsoap.org/ws/2005/04/discovery\" xmlns:dn=\"http://www.onvif.org/ver10/network/wsdl\">\
<e:Header><w:MessageID>uuid:{id}</w:MessageID><w:To e:mustUnderstand=\"true\">urn:schemas-xmlsoap-org:ws:2005:04:discovery</w:To>\
<w:Action e:mustUnderstand=\"true\">http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</w:Action></e:Header>\
<e:Body><d:Probe><d:Types>dn:NetworkVideoTransmitter</d:Types></d:Probe></e:Body></e:Envelope>"
    )
}

/// The text inside the first element whose local name is `name`, whatever
/// namespace prefix the device chose.
pub fn element<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = xml;
    loop {
        let at = rest.find('<')?;
        rest = &rest[at + 1..];
        let tag_end = rest.find('>')?;
        let tag = &rest[..tag_end];
        let local = tag.split_whitespace().next().unwrap_or("").rsplit(':').next().unwrap_or("");
        if local == name && !tag.starts_with('/') && !tag.ends_with('/') {
            let body = &rest[tag_end + 1..];
            return Some(&body[..body.find("</")?]);
        }
    }
}

/// Every ProbeMatch in one answer.
pub fn matches(xml: &str) -> Option<Found> {
    let xaddrs: Vec<String> = element(xml, "XAddrs")?.split_whitespace().map(str::to_string).collect();
    let scopes = element(xml, "Scopes").unwrap_or("");
    let scope = |key: &str| {
        scopes
            .split_whitespace()
            .find_map(|s| s.strip_prefix(&format!("onvif://www.onvif.org/{key}/")))
            .map(|v| v.replace("%20", " "))
            .unwrap_or_default()
    };
    (!xaddrs.is_empty()).then(|| Found { xaddrs, name: scope("name"), hardware: scope("hardware") })
}

/// Send the Probe to `to` and gather answers until `wait` is up. Several
/// answers from one device are kept once.
pub fn probe(to: SocketAddr, wait: Duration) -> std::io::Result<Vec<Found>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_multicast_ttl_v4(1)?;
    let id = format!("{:032x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    socket.send_to(probe_message(&id).as_bytes(), to)?;
    let until = Instant::now() + wait;
    let mut found: Vec<Found> = Vec::new();
    let mut buf = vec![0u8; 65_536];
    while let Some(left) = until.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()) {
        socket.set_read_timeout(Some(left))?;
        let Ok((n, _)) = socket.recv_from(&mut buf) else { break };
        if let Some(f) = matches(&String::from_utf8_lossy(&buf[..n])) {
            if !found.iter().any(|x| x.xaddrs == f.xaddrs) {
                found.push(f);
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const ANSWER: &str = "<SOAP-ENV:Envelope><SOAP-ENV:Body><d:ProbeMatches><d:ProbeMatch>\
<d:Scopes>onvif://www.onvif.org/type/video_encoder onvif://www.onvif.org/name/Studio%20Cam onvif://www.onvif.org/hardware/IPC-HX</d:Scopes>\
<d:XAddrs>http://10.0.0.64/onvif/device_service http://[fe80::1]/onvif/device_service</d:XAddrs>\
</d:ProbeMatch></d:ProbeMatches></SOAP-ENV:Body></SOAP-ENV:Envelope>";

    #[test]
    fn a_probe_match_reads_whatever_the_prefixes() {
        let f = matches(ANSWER).unwrap();
        assert_eq!(f.xaddrs[0], "http://10.0.0.64/onvif/device_service");
        assert_eq!(f.name, "Studio Cam");
        assert_eq!(f.hardware, "IPC-HX");
        assert!(probe_message("x").contains("NetworkVideoTransmitter"));
        assert_eq!(element("<a:B x=\"1\">hi</a:B>", "B"), Some("hi"));
        assert_eq!(matches("<nothing/>"), None);
    }
}
