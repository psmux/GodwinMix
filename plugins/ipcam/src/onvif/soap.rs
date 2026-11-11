//! The three ONVIF calls it takes to turn a device into a stream address:
//! where its media service is, what profiles it has, and each profile's RTSP
//! address. With a user name, every call carries a WS-Security
//! UsernameToken with a password digest, which is what ONVIF asks for. The
//! envelope and the login are `godwinmix-ptz`'s, shared with PTZ control.

use std::time::Duration;

use godwinmix_ptz::onvif::soap::call;
pub use godwinmix_ptz::onvif::soap::{digest, utc, Login};
use godwinmix_ptz::onvif::xml::escape;

use super::http::Failure;
use super::wsd::element;

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
