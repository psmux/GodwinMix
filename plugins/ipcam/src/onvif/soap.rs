//! The three ONVIF calls it takes to turn a device into a stream address:
//! where its media service is, what profiles it has, and each profile's RTSP
//! address. With a user name, every call carries a WS-Security
//! UsernameToken with a password digest, which is what ONVIF asks for.

use base64::Engine;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::http::{post, Failure};
use super::wsd::element;

/// Who to log in as. Empty user means no login.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Login {
    pub user: String,
    pub password: String,
}

/// base64(sha1(nonce + created + password)), as WS-Security defines it.
pub fn digest(nonce: &[u8], created: &str, password: &str) -> String {
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY);
    ctx.update(nonce);
    ctx.update(created.as_bytes());
    ctx.update(password.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(ctx.finish())
}

/// Seconds since 1970 as `2026-10-01T09:30:00Z`, without a date crate.
pub fn utc(secs: u64) -> String {
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest / 60 % 60, rest % 60)
}

fn security(login: &Login) -> String {
    if login.user.is_empty() {
        return String::new();
    }
    let mut nonce = [0u8; 16];
    let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut nonce);
    let created = utc(SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0));
    let b64 = base64::engine::general_purpose::STANDARD.encode(nonce);
    format!(
        "<Header><Security s:mustUnderstand=\"1\" xmlns=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd\">\
<UsernameToken><Username>{}</Username>\
<Password Type=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest\">{}</Password>\
<Nonce EncodingType=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-soap-message-security-1.0#Base64Binary\">{b64}</Nonce>\
<Created xmlns=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd\">{created}</Created>\
</UsernameToken></Security></Header>",
        escape(&login.user),
        digest(&nonce, &created, &login.password)
    )
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn call(url: &str, login: &Login, body: &str, timeout: Duration) -> Result<String, Failure> {
    let envelope = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><s:Envelope xmlns:s=\"http://www.w3.org/2003/05/soap-envelope\" \
xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\" xmlns:trt=\"http://www.onvif.org/ver10/media/wsdl\" \
xmlns:tt=\"http://www.onvif.org/ver10/schema\">{}<s:Body>{body}</s:Body></s:Envelope>",
        security(login)
    );
    post(url, &envelope, timeout)
}

/// The media service's address, or the device service's own when the device
/// does not say.
pub fn media_service(device: &str, login: &Login, timeout: Duration) -> Result<String, Failure> {
    let answer = call(device, login, "<tds:GetCapabilities><tds:Category>Media</tds:Category></tds:GetCapabilities>", timeout)?;
    let media = answer.find("Media>").map(|at| &answer[at..]).and_then(|m| element(m, "XAddr"));
    Ok(media.map(|m| m.trim().to_string()).unwrap_or_else(|| device.to_string()))
}

/// Each profile's token and name.
pub fn profiles(media: &str, login: &Login, timeout: Duration) -> Result<Vec<(String, String)>, Failure> {
    let answer = call(media, login, "<trt:GetProfiles/>", timeout)?;
    let mut out = Vec::new();
    for part in answer.split("Profiles").skip(1) {
        let Some(token) = part.split("token=\"").nth(1).and_then(|t| t.split('"').next()) else { continue };
        if out.iter().any(|(t, _)| t == token) {
            continue;
        }
        out.push((token.to_string(), element(part, "Name").unwrap_or(token).to_string()));
    }
    Ok(out)
}

/// The RTSP address of one profile.
pub fn stream_uri(media: &str, login: &Login, token: &str, timeout: Duration) -> Result<String, Failure> {
    let body = format!(
        "<trt:GetStreamUri><trt:StreamSetup><tt:Stream>RTP-Unicast</tt:Stream><tt:Transport><tt:Protocol>RTSP</tt:Protocol>\
</tt:Transport></trt:StreamSetup><trt:ProfileToken>{}</trt:ProfileToken></trt:GetStreamUri>",
        escape(token)
    );
    let answer = call(media, login, &body, timeout)?;
    element(&answer, "Uri").map(|u| u.trim().replace("&amp;", "&")).ok_or_else(|| Failure::Other(format!("{media} gave no stream address for {token}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_matches_the_ws_security_worked_example() {
        // The ONVIF Application Programmer's Guide, section 6.1.1.3.
        let nonce = base64::engine::general_purpose::STANDARD.decode("LKqI6G/AikKCQrN0zqZFlg==").unwrap();
        assert_eq!(digest(&nonce, "2010-09-16T07:50:45Z", "userpassword"), "tuOSpGlFlIXsozq4HFNeeGeFLEI=");
    }

    #[test]
    fn a_unix_time_prints_as_utc() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(1_284_623_445), "2010-09-16T07:50:45Z");
        assert_eq!(utc(1_790_812_800), "2026-10-01T00:00:00Z");
    }
}
