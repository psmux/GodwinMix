//! Discovery against a camera that behaves like one: it answers the probe on
//! UDP, serves GetCapabilities, GetProfiles and GetStreamUri over HTTP, and
//! checks the WS-Security digest before it says anything.

use super::*;
use base64::Engine;
use std::io::{Read, Write};
use std::net::{TcpListener, UdpSocket};

pub const PASSWORD: &str = "camera-pw";

/// The answer to one SOAP body, or `None` for a refused login.
fn answer(body: &str, media: &str) -> Option<String> {
    let nonce = wsd::element(body, "Nonce").and_then(|n| base64::engine::general_purpose::STANDARD.decode(n).ok())?;
    let created = wsd::element(body, "Created")?;
    if wsd::element(body, "Password")? != soap::digest(&nonce, created, PASSWORD) {
        return None;
    }
    Some(if body.contains("GetCapabilities") {
        format!("<tds:Capabilities><tt:Media><tt:XAddr>{media}</tt:XAddr></tt:Media></tds:Capabilities>")
    } else if body.contains("GetProfiles") {
        "<trt:Profiles token=\"main\" fixed=\"true\"><tt:Name>Main</tt:Name></trt:Profiles>\
         <trt:Profiles token=\"sub\"><tt:Name>Sub</tt:Name></trt:Profiles>"
            .to_string()
    } else {
        let token = wsd::element(body, "ProfileToken").unwrap_or("?");
        format!("<trt:MediaUri><tt:Uri>rtsp://127.0.0.1:554/{token}?a=1&amp;b=2</tt:Uri></trt:MediaUri>")
    })
}

/// Head and body, however many reads they take.
fn read_request(conn: &mut std::net::TcpStream) -> String {
    let (mut raw, mut buf) = (Vec::new(), vec![0u8; 16_384]);
    loop {
        let text = String::from_utf8_lossy(&raw).to_string();
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let want = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0))).unwrap_or(0);
            if body.len() >= want {
                return text;
            }
        }
        match conn.read(&mut buf) {
            Ok(0) | Err(_) => return text,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
        }
    }
}

/// A camera on loopback: the probe port it answers on.
pub fn camera() -> std::net::SocketAddr {
    let http = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", http.local_addr().unwrap());
    let media = format!("{base}/onvif/media");
    std::thread::spawn(move || {
        for conn in http.incoming().flatten() {
            let mut conn = conn;
            conn.set_read_timeout(Some(Duration::from_secs(2))).ok();
            let text = read_request(&mut conn);
            let (status, body) = match answer(&text, &media) {
                Some(b) => ("200 OK", format!("<s:Envelope><s:Body>{b}</s:Body></s:Envelope>")),
                None => ("400 Bad Request", "<s:Fault><s:Subcode><s:Value>ter:NotAuthorized</s:Value></s:Subcode></s:Fault>".into()),
            };
            let _ = write!(conn, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let probe_at = udp.local_addr().unwrap();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 8192];
        while let Ok((_, from)) = udp.recv_from(&mut buf) {
            let reply = format!(
                "<e:Envelope><e:Body><d:ProbeMatches><d:ProbeMatch><d:Scopes>onvif://www.onvif.org/name/Door</d:Scopes>\
                 <d:XAddrs>{base}/onvif/device_service</d:XAddrs></d:ProbeMatch></d:ProbeMatches></e:Body></e:Envelope>"
            );
            let _ = udp.send_to(reply.as_bytes(), from);
        }
    });
    probe_at
}

#[test]
fn a_camera_with_the_right_login_gives_every_profiles_rtsp_address() {
    let login = Login { user: "admin".into(), password: PASSWORD.into() };
    let found = discover(camera(), Duration::from_secs(3), &login);
    assert!(found.locked.is_empty(), "{:?}", found.locked);
    let names: Vec<&str> = found.streams.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Door (Main)", "Door (Sub)"]);
    assert_eq!(found.streams[0].uri, "rtsp://admin:camera-pw@127.0.0.1:554/main?a=1&b=2");
}

#[test]
fn a_camera_that_wants_a_login_is_named_and_gives_nothing() {
    for login in [Login::default(), Login { user: "admin".into(), password: "wrong".into() }] {
        let found = discover(camera(), Duration::from_secs(3), &login);
        assert!(found.streams.is_empty());
        assert_eq!(found.locked, ["Door"]);
    }
}

#[test]
fn a_login_is_escaped_into_the_address() {
    let login = Login { user: "ad min".into(), password: "p@ss:1".into() };
    assert_eq!(with_login("rtsp://10.0.0.5/s1", &login), "rtsp://ad%20min:p%40ss%3A1@10.0.0.5/s1");
    assert_eq!(with_login("rtsp://u:p@10.0.0.5/s1", &login), "rtsp://u:p@10.0.0.5/s1");
}
