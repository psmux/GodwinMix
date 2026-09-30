//! `rendition.presets`, `rendition.plan`, `governor.status` and
//! `governor.calibrate`.
//!
//! None of these asks the mixer thread anything: the plan, the shed list and
//! the station are shared with it, so the page can poll them while a take is
//! running. `configure` is called once from `run`, the same way `presets`
//! gets its config, so the control plane gained no field for them.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::render::{status, RenditionsHandle, PROGRAMME};
use godwinmix_protocol::action::{ActionKind, ErrorAction};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::rendition::{
    CalibrateRequest, CalibrateResult, GovernorStatus, PlanRequest, PlanView, PresetsResult,
};
use godwinmix_protocol::scope::Scope;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

/// A channel's plan, by channel id, for `rendition.plan {scope: "channel:<id>"}`.
/// Installed by whatever plans channel destinations.
pub type ChannelPlans = Arc<dyn Fn(&str) -> Option<PlanView> + Send + Sync>;

static HANDLE: OnceLock<RwLock<Option<RenditionsHandle>>> = OnceLock::new();
static CHANNELS: OnceLock<RwLock<Option<ChannelPlans>>> = OnceLock::new();

/// Called once at startup with the mixer's renditions.
pub fn configure(handle: RenditionsHandle) {
    *HANDLE.get_or_init(|| RwLock::new(None)).write() = Some(handle);
}

/// Called by the channel side, so a channel's plan answers here too.
pub fn configure_channels(plans: ChannelPlans) {
    *CHANNELS.get_or_init(|| RwLock::new(None)).write() = Some(plans);
}

/// The station the programme's renditions run on, once `configure` has run.
/// The channel side takes its governor from here, so the machine has one.
pub fn station() -> Option<godwinmix_core::render::Station> {
    HANDLE.get().and_then(|h| h.read().as_ref().map(|h| h.station.clone()))
}

fn held() -> Result<RenditionsHandle, RpcError> {
    HANDLE
        .get()
        .and_then(|h| h.read().clone())
        .ok_or_else(|| RpcError::not_in_state("renditions are not running on this core yet; try again once it has started"))
}

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "rendition.presets",
            Scope::Read,
            "Every rendition preset, priced on this machine by the governor. One this machine \
             cannot make says so, with why.",
            handler(|_call: Call, _| async move {
                let h = held()?;
                body(PresetsResult { presets: status::presets_here(&h.station, &h.source) })
            }),
        )
        .result(schema_of::<PresetsResult>)
        .tool(
            "rendition_presets",
            Tier::Search,
            "The formats an output can ask to be sent in (YouTube 1080p30, an adaptive ladder, \
             sound only), each with what it would cost on this machine. Pass one as \
             `rendition: {preset: <id>}` to add_output.",
        ),
    );
    reg.register(
        MethodDef::new(
            "rendition.plan",
            Scope::Read,
            "What the planner built for every output that asked for a rendition: each node, \
             what it serves, which encoder and why, and the totals.",
            handler(plan),
        )
        .params(schema_of::<PlanRequest>)
        .result(schema_of::<PlanView>),
    );
    reg.register(
        MethodDef::new(
            "governor.status",
            Scope::Read,
            "The resource governor: when this machine was measured, what is in use and free \
             on the CPU and each GPU encoder, and what was shed to keep the programme whole.",
            handler(|_call: Call, _| async move {
                let h = held()?;
                let shed = h.shed.read().clone();
                body(status::governor_status(&h.station, &shed))
            }),
        )
        .result(schema_of::<GovernorStatus>),
    );
    reg.register(
        MethodDef::new(
            "governor.calibrate",
            Scope::Admin,
            "Measure this machine's encoders again, in the background, a few seconds of every \
             core. Refused while anything is on air unless `confirm` is true.",
            handler(calibrate),
        )
        .params(schema_of::<CalibrateRequest>)
        .result(schema_of::<CalibrateResult>),
    );
}

async fn plan(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: PlanRequest = call.params(&params)?;
    let scope = req.scope.unwrap_or_else(|| PROGRAMME.to_string());
    if scope == PROGRAMME {
        return body(held()?.plan.read().clone());
    }
    let channels = CHANNELS.get().and_then(|c| c.read().clone());
    let found = scope.strip_prefix("channel:").and_then(|id| channels.and_then(|f| f(id)));
    match found {
        Some(view) => body(view),
        None => Err(RpcError::not_found("plan scope", &scope, &[PROGRAMME.to_string()])),
    }
}

async fn calibrate(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: CalibrateRequest = call.params(&params)?;
    let h = held()?;
    if h.station.on_air() && !req.confirm {
        let action = ErrorAction {
            value: Some(serde_json::json!({ "confirm": true })),
            after_ms: Some(0),
            ..ErrorAction::new("Measure anyway", ActionKind::Retry)
        };
        return Err(RpcError::not_in_state(
            "Something is on air, and measuring takes every core for a few seconds, which can \
             cost it frames. Measure once nothing is going out, or send confirm true to measure \
             now anyway.",
        )
        .with("on_air", true)
        .with_action(action));
    }
    let started = h.station.calibrate_now();
    body(CalibrateResult { started })
}
