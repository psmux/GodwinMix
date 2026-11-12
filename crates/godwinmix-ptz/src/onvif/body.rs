//! The SOAP bodies for ONVIF PTZ (ver20) and imaging (ver20), and the
//! readers for what comes back. Speeds and positions are in the generic
//! spaces every ONVIF PTZ camera supports: pan and tilt -1 to 1, zoom 0 to 1
//! for a position and -1 to 1 for a speed.

use super::xml::{attr, each, element, escape};
use crate::types::{Exposure, ExposureMode};

pub fn capabilities() -> &'static str {
    "<tds:GetCapabilities><tds:Category>All</tds:Category></tds:GetCapabilities>"
}

/// The XAddr of one service in a GetCapabilities answer: `PTZ`, `Media`,
/// `Imaging`.
pub fn service(answer: &str, name: &str) -> Option<String> {
    let at = answer.find(&format!(":{name}>")).or_else(|| answer.find(&format!("<{name}>")))?;
    element(&answer[at..], "XAddr").map(|x| x.trim().to_string())
}

pub fn profiles() -> &'static str {
    "<trt:GetProfiles/>"
}

/// Each profile's token, and the video source it reads from.
pub fn read_profiles(answer: &str) -> Vec<(String, Option<String>)> {
    each(answer, "Profiles")
        .into_iter()
        .filter_map(|p| Some((attr(p, "Profiles", "token")?.to_string(), element(p, "SourceToken").map(|s| s.trim().to_string()))))
        .collect()
}

fn token(profile: &str) -> String {
    format!("<tptz:ProfileToken>{}</tptz:ProfileToken>", escape(profile))
}

pub fn continuous_move(profile: &str, pan: f32, tilt: f32, zoom: f32) -> String {
    format!(
        "<tptz:ContinuousMove>{}<tptz:Velocity><tt:PanTilt x=\"{pan:.3}\" y=\"{tilt:.3}\"/><tt:Zoom x=\"{zoom:.3}\"/></tptz:Velocity></tptz:ContinuousMove>",
        token(profile)
    )
}

pub fn stop(profile: &str) -> String {
    format!("<tptz:Stop>{}<tptz:PanTilt>true</tptz:PanTilt><tptz:Zoom>true</tptz:Zoom></tptz:Stop>", token(profile))
}

pub fn absolute_move(profile: &str, pan: f32, tilt: f32, zoom: f32) -> String {
    format!(
        "<tptz:AbsoluteMove>{}<tptz:Position><tt:PanTilt x=\"{pan:.4}\" y=\"{tilt:.4}\"/><tt:Zoom x=\"{zoom:.4}\"/></tptz:Position></tptz:AbsoluteMove>",
        token(profile)
    )
}

pub fn status(profile: &str) -> String {
    format!("<tptz:GetStatus>{}</tptz:GetStatus>", token(profile))
}

/// Pan, tilt and zoom out of a GetStatusResponse.
pub fn read_status(answer: &str) -> Option<(f32, f32, f32)> {
    let at = answer.find("Position")?;
    let p = &answer[at..];
    let num = |e: &str, a: &str| attr(p, e, a).and_then(|v| v.trim().parse::<f32>().ok());
    Some((num("PanTilt", "x")?, num("PanTilt", "y")?, num("Zoom", "x").unwrap_or(0.0)))
}

pub fn presets(profile: &str) -> String {
    format!("<tptz:GetPresets>{}</tptz:GetPresets>", token(profile))
}

/// Each preset's token and name.
pub fn read_presets(answer: &str) -> Vec<(String, String)> {
    each(answer, "Preset")
        .into_iter()
        .filter_map(|p| Some((attr(p, "Preset", "token")?.to_string(), element(p, "Name").unwrap_or("").trim().to_string())))
        .collect()
}

/// SetPreset: a new one by name, or over an existing token.
pub fn set_preset(profile: &str, name: &str, existing: Option<&str>) -> String {
    let over = existing.map(|t| format!("<tptz:PresetToken>{}</tptz:PresetToken>", escape(t))).unwrap_or_default();
    format!("<tptz:SetPreset>{}<tptz:PresetName>{}</tptz:PresetName>{over}</tptz:SetPreset>", token(profile), escape(name))
}

pub fn goto_preset(profile: &str, preset: &str) -> String {
    format!("<tptz:GotoPreset>{}<tptz:PresetToken>{}</tptz:PresetToken></tptz:GotoPreset>", token(profile), escape(preset))
}

fn source(token: &str) -> String {
    format!("<timg:VideoSourceToken>{}</timg:VideoSourceToken>", escape(token))
}

/// SetImagingSettings with only the parts given, in schema order (Exposure
/// before Focus).
pub fn imaging(video: &str, exposure: Option<&Exposure>, focus_auto: Option<bool>) -> String {
    let mut settings = String::new();
    if let Some(e) = exposure {
        settings.push_str("<tt:Exposure>");
        if let Some(mode) = e.mode {
            settings.push_str(if mode == ExposureMode::Auto { "<tt:Mode>AUTO</tt:Mode>" } else { "<tt:Mode>MANUAL</tt:Mode>" });
        }
        let parts = [("ExposureTime", e.shutter.map(f32::from)), ("Gain", e.gain.map(f32::from)), ("Iris", e.iris.map(|i| -f32::from(i) / 10.0))];
        for (name, v) in parts {
            if let Some(v) = v {
                settings.push_str(&format!("<tt:{name}>{v}</tt:{name}>"));
            }
        }
        settings.push_str("</tt:Exposure>");
    }
    if let Some(auto) = focus_auto {
        settings.push_str(&format!("<tt:Focus><tt:AutoFocusMode>{}</tt:AutoFocusMode></tt:Focus>", if auto { "AUTO" } else { "MANUAL" }));
    }
    format!("<timg:SetImagingSettings>{}<timg:ImagingSettings>{settings}</timg:ImagingSettings></timg:SetImagingSettings>", source(video))
}

pub fn focus_move(video: &str, speed: f32) -> String {
    format!("<timg:Move>{}<timg:Focus><tt:Continuous><tt:Speed>{speed:.3}</tt:Speed></tt:Continuous></timg:Focus></timg:Move>", source(video))
}

pub fn focus_stop(video: &str) -> String {
    format!("<timg:Stop>{}</timg:Stop>", source(video))
}
