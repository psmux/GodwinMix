//! What `rendition.presets` and `governor.status` answer: every preset
//! priced on this machine, and the governor's book and headroom. Read from
//! any thread; nothing here touches a pipeline.

use super::model::GovernorModel;
use super::{Station, PROGRAMME};
use godwinmix_govern::headroom::UNLIMITED;
use godwinmix_protocol::rendition::{
    Cost, CpuUse, DeviceUse, GovernorStatus, PresetRef, RenditionChoice, RenditionPreset, ShedNote,
    StreamInfo,
};
use godwinmix_render::{plan, presets};
use std::collections::BTreeMap;

/// What `choice` would cost on its own on this machine, or why it cannot
/// be made here.
pub fn price(station: &Station, source: &StreamInfo, choice: &RenditionChoice) -> Result<Cost, String> {
    let Some(requests) = presets::expand("preset", choice)? else {
        return Ok(Cost::default());
    };
    let model = GovernorModel::new(station.governor().clone(), station.slots(), station.audio(), BTreeMap::new());
    let sources = [(PROGRAMME.to_string(), source.clone())];
    let requests: Vec<_> = requests.into_iter().map(|r| (PROGRAMME.to_string(), r)).collect();
    plan(&sources, &requests, &model).map(|p| p.total).map_err(|e| e.to_string())
}

/// Every built in preset, priced here, with the ones this machine cannot
/// make marked and the reason given.
pub fn presets_here(station: &Station, source: &StreamInfo) -> Vec<RenditionPreset> {
    presets::builtin()
        .into_iter()
        .map(|p| {
            let choice = RenditionChoice::Preset(PresetRef { preset: p.id.clone() });
            match price(station, source, &choice) {
                Ok(cost) => RenditionPreset { cost: Some(cost), ..p },
                Err(why) => RenditionPreset { available: false, why: Some(why), ..p },
            }
        })
        .collect()
}

/// The governor as it stands.
pub fn governor_status(station: &Station, shed: &[ShedNote]) -> GovernorStatus {
    let g = station.governor();
    let profile = g.profile();
    let cal = profile.calibration();
    let held = g.held();
    let committed = held.iter().fold(Cost::default(), |a, h| a.plus(h.cost));
    let load = g.load();
    let cores = cal.machine.cores.max(std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1));
    let used = if load.samples > 0 { load.own_millicores.max(committed.cpu_millicores) } else { committed.cpu_millicores };
    let mut devices: Vec<String> = profile.devices();
    for d in station.slots().into_iter().filter_map(|s| s.device) {
        if !devices.contains(&d) {
            devices.push(d);
        }
    }
    let devices = devices
        .into_iter()
        .map(|d| {
            let mine = held.iter().filter(|h| h.device.as_deref() == Some(d.as_str()));
            let (millis, sessions) = mine.fold((0, 0), |(m, s), h| (m + h.cost.device_millis, s + h.cost.device_sessions));
            let room = g.headroom(Some(&d));
            DeviceUse {
                kind: d.clone(),
                used_millis: millis,
                room_millis: room.device_millis,
                sessions_used: sessions,
                sessions_max: profile.session_limit(&d),
                id: d,
            }
        })
        .collect();
    let room = g.headroom(None);
    GovernorStatus {
        calibrated_at: profile.is_calibrated().then_some(cal.taken_unix),
        fingerprint: profile.is_calibrated().then(|| cal.fingerprint.clone()),
        calibrating: station.calibrating(),
        cpu: CpuUse { cores, used_millicores: used, room_millicores: room.cpu_millicores },
        devices,
        egress_kbps: if committed.egress_kbps == UNLIMITED { 0 } else { committed.egress_kbps },
        shed: shed.to_vec(),
    }
}
