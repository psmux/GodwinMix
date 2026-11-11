//! A SOAP call with a WS-Security login, which is how every ONVIF service
//! is spoken to. With a user name, each call carries a UsernameToken with a
//! password digest, which is what ONVIF asks for. Shared with the ipcam
//! plugin's discovery, which was where it started.

use base64::Engine;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::xml::escape;
use crate::error::{Code, Error};
use crate::http::{post, Failure};

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
        "<s:Header><Security s:mustUnderstand=\"1\" xmlns=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd\">\
<UsernameToken><Username>{}</Username>\
<Password Type=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest\">{}</Password>\
<Nonce EncodingType=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-soap-message-security-1.0#Base64Binary\">{b64}</Nonce>\
<Created xmlns=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd\">{created}</Created>\
</UsernameToken></Security></s:Header>",
        escape(&login.user),
        digest(&nonce, &created, &login.password)
    )
}

/// The namespace prefixes every body may use: device, media, PTZ, imaging
/// and the shared schema.
const NAMESPACES: &str = "xmlns:s=\"http://www.w3.org/2003/05/soap-envelope\" \
xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\" xmlns:trt=\"http://www.onvif.org/ver10/media/wsdl\" \
xmlns:tptz=\"http://www.onvif.org/ver20/ptz/wsdl\" xmlns:timg=\"http://www.onvif.org/ver20/imaging/wsdl\" \
xmlns:tt=\"http://www.onvif.org/ver10/schema\"";

/// One SOAP call: `body` in an envelope with the login, POSTed to `url`.
pub fn call(url: &str, login: &Login, body: &str, timeout: Duration) -> Result<String, Failure> {
    let envelope = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><s:Envelope {NAMESPACES}>{}<s:Body>{body}</s:Body></s:Envelope>", security(login));
    post(url, &envelope, timeout)
}

/// One call, with the two ONVIF failures turned into errors that say what
/// to do.
pub fn checked(url: &str, login: &Login, body: &str, timeout: Duration) -> std::result::Result<String, Error> {
    call(url, login, body, timeout).map_err(|f| match f {
        Failure::Unauthorized => Error::new(Code::Unauthorized, format!("{url} refused the login. Check the user name and password, and that the camera's clock is within a few minutes of this machine's: ONVIF logins carry the time.")),
        Failure::Other(why) => Error::new(Code::Unreachable, format!("{why}. Check the camera is on and that ONVIF is switched on in its web page.")),
    })
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
