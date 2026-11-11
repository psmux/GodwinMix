//! A fake ONVIF PTZ camera: the device, media, PTZ and imaging services on
//! one loopback HTTP port. It checks the WS-Security digest before it says
//! anything, and reads each call with the same local name matching a real
//! camera's answers are read with.

use base64::Engine;
use std::net::SocketAddr;

use super::http::{serve, Reply};
use super::{Head, Shared};
use crate::onvif::soap::digest;
use crate::onvif::xml::{attr, element};
use crate::types::{ExposureMode, Focus, Velocity};

pub const USER: &str = "admin";
pub const PASSWORD: &str = "camera-pw";

fn logged_in(body: &str) -> bool {
    let nonce = element(body, "Nonce").and_then(|n| base64::engine::general_purpose::STANDARD.decode(n).ok());
    let (Some(nonce), Some(created), Some(password)) = (nonce, element(body, "Created"), element(body, "Password")) else { return false };
    element(body, "Username") == Some(USER) && password == digest(&nonce, created, PASSWORD)
}

fn num(body: &str, el: &str, a: &str) -> f32 {
    attr(body, el, a).and_then(|v| v.parse().ok()).unwrap_or(0.0)
}

/// The answer to one call, inside the Body.
pub fn answer(head: &mut Head, body: &str, base: &str) -> String {
    let has = |op: &str| body.contains(&format!(":{op}>")) || body.contains(&format!(":{op}/>"));
    if has("GetCapabilities") {
        return format!(
            "<tds:GetCapabilitiesResponse><tds:Capabilities><tt:Imaging><tt:XAddr>{base}/onvif/imaging</tt:XAddr></tt:Imaging>\
<tt:Media><tt:XAddr>{base}/onvif/media</tt:XAddr></tt:Media><tt:PTZ><tt:XAddr>{base}/onvif/ptz</tt:XAddr></tt:PTZ></tds:Capabilities></tds:GetCapabilitiesResponse>"
        );
    }
    if has("GetProfiles") {
        return "<trt:GetProfilesResponse><trt:Profiles token=\"main\" fixed=\"true\"><tt:Name>Main</tt:Name>\
<tt:VideoSourceConfiguration token=\"vsc\"><tt:SourceToken>video-1</tt:SourceToken></tt:VideoSourceConfiguration></trt:Profiles>\
<trt:Profiles token=\"sub\"><tt:Name>Sub</tt:Name></trt:Profiles></trt:GetProfilesResponse>"
            .into();
    }
    if has("ContinuousMove") {
        head.set_velocity(Velocity { pan: num(body, "PanTilt", "x"), tilt: num(body, "PanTilt", "y"), zoom: num(body, "Zoom", "x") });
    } else if body.contains("<tptz:Stop>") {
        head.set_velocity(Velocity::STOP);
    } else if has("AbsoluteMove") {
        head.jump(num(body, "PanTilt", "x"), num(body, "PanTilt", "y"), num(body, "Zoom", "x"));
    } else if has("GetStatus") {
        let p = head.position();
        return format!(
            "<tptz:GetStatusResponse><tptz:PTZStatus><tt:Position><tt:PanTilt x=\"{}\" y=\"{}\" space=\"http://www.onvif.org/ver10/tptz/PanTiltSpaces/PositionGenericSpace\"/>\
<tt:Zoom x=\"{}\"/></tt:Position><tt:MoveStatus><tt:PanTilt>IDLE</tt:PanTilt></tt:MoveStatus></tptz:PTZStatus></tptz:GetStatusResponse>",
            p.pan, p.tilt, p.zoom
        );
    } else if has("GetPresets") {
        let list: String = head.presets.keys().map(|n| format!("<tptz:Preset token=\"tok{n}\"><tt:Name>preset-{n}</tt:Name></tptz:Preset>")).collect();
        return format!("<tptz:GetPresetsResponse>{list}</tptz:GetPresetsResponse>");
    } else if has("SetPreset") {
        let n = element(body, "PresetName").and_then(|s| s.strip_prefix("preset-")).and_then(|s| s.parse().ok()).unwrap_or(0);
        head.save(n);
        return format!("<tptz:SetPresetResponse><tptz:PresetToken>tok{n}</tptz:PresetToken></tptz:SetPresetResponse>");
    } else if has("GotoPreset") {
        let n = element(body, "PresetToken").and_then(|t| t.strip_prefix("tok")).and_then(|s| s.parse().ok()).unwrap_or(0);
        head.recall(n);
    } else if has("SetImagingSettings") {
        if let Some(mode) = element(body, "AutoFocusMode") {
            head.focus(if mode == "AUTO" { Focus::Auto } else { Focus::Manual });
        }
        if let Some(mode) = element(body, "Mode") {
            head.exposure.mode = Some(if mode == "AUTO" { ExposureMode::Auto } else { ExposureMode::Manual });
        }
        if let Some(t) = element(body, "ExposureTime").and_then(|v| v.parse::<f32>().ok()) {
            head.exposure.shutter = Some(t as u16);
        }
    } else if body.contains("<timg:Move>") {
        head.focus(Focus::Drive(element(body, "Speed").and_then(|v| v.parse().ok()).unwrap_or(0.0)));
    } else if body.contains("<timg:Stop>") {
        head.focus(Focus::Drive(0.0));
    } else {
        return String::new();
    }
    "<ok/>".into()
}

/// The camera on a loopback port. Answers its device service URL.
pub fn serve_onvif(head: Shared) -> String {
    let base = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let b = base.clone();
    let at: SocketAddr = serve(move |request| {
        let body = request.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
        if !logged_in(body) {
            return Reply { status: 400, headers: Vec::new(), body: "<s:Fault><s:Subcode><s:Value>ter:NotAuthorized</s:Value></s:Subcode></s:Fault>".into() };
        }
        let inner = answer(&mut head.lock().unwrap(), body, &b.lock().unwrap());
        if inner.is_empty() {
            return Reply { status: 500, headers: Vec::new(), body: "<s:Fault><s:Reason>ActionNotSupported</s:Reason></s:Fault>".into() };
        }
        Reply::ok(format!("<s:Envelope><s:Body>{inner}</s:Body></s:Envelope>"))
    });
    *base.lock().unwrap() = format!("http://{at}");
    format!("http://{at}/onvif/device_service")
}
