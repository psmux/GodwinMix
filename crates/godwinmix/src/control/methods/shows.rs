//! `show.*` in the method table.
//!
//! A station answers these itself (`crate::station::shows_api`) and never
//! passes them to a show, so what is registered here is what a core answers
//! when there is no station in front of it: a single process core started
//! with `--show`, an embedded one, or a show reached on its own socket. It
//! has one show, itself, and says so; the changes need a station.

mod direct;

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{no_params, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::requests::IdRequest;
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::shows::{Show, ShowAddRequest, ShowList, ShowRemoved, ShowRenameRequest, ShowState};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "show.list",
            Scope::Read,
            "Every show on this machine: its name, whether it is running, what is on air, \
             what its outputs send and what its process costs. `current` is the show a \
             client reaches when it names none.",
            handler(|call: Call, _| async move { body(alone(&call).await?) }),
        )
        .params(no_params)
        .result(schema_of::<ShowList>)
        .tool(
            "list_shows",
            Tier::Search,
            "The independent programmes this machine runs, each with what is on air and \
             whether it is running. Every other tool works on one show; the station picks \
             the first unless a show is named.",
        ),
    );
    reg.register(
        MethodDef::new(
            "show.add",
            Scope::Admin,
            "Make another show and start it: empty, a copy of a show (without its outputs, \
             so nothing goes out twice), or from a project file.",
            handler(|_call: Call, _| async move { Err::<serde_json::Value, _>(needs_station("show.add")) }),
        )
        .params(schema_of::<ShowAddRequest>)
        .result(schema_of::<Show>)
        .not_idempotent(),
    );
    reg.register(
        MethodDef::new("show.rename", Scope::Admin, "Give a show another name. Its id stays.", handler(|_call: Call, _| async move {
            Err::<serde_json::Value, _>(needs_station("show.rename"))
        }))
        .params(schema_of::<ShowRenameRequest>)
        .result(schema_of::<Show>),
    );
    reg.register(
        MethodDef::new(
            "show.remove",
            Scope::Admin,
            "Stop a show and remove it with its folder. Refused for the last show and for \
             main, the show the station was started with.",
            handler(|_call: Call, _| async move { Err::<serde_json::Value, _>(needs_station("show.remove")) }),
        )
        .params(schema_of::<IdRequest>)
        .result(schema_of::<ShowRemoved>)
        .destructive(),
    );
    reg.register(
        MethodDef::new("show.start", Scope::Admin, "Start a stopped or failed show.", handler(|_call: Call, _| async move {
            Err::<serde_json::Value, _>(needs_station("show.start"))
        }))
        .params(schema_of::<IdRequest>)
        .result(schema_of::<Show>),
    );
    reg.register(
        MethodDef::new(
            "show.stop",
            Scope::Admin,
            "Stop a show. It keeps its config, and stays stopped when the station starts \
             again, until show.start.",
            handler(|_call: Call, _| async move { Err::<serde_json::Value, _>(needs_station("show.stop")) }),
        )
        .params(schema_of::<IdRequest>)
        .result(schema_of::<Show>),
    );
    direct::register(reg);
}

pub(super) fn needs_station(method: &str) -> RpcError {
    let under = crate::station::show::mode().map(|m| m.id.clone());
    let message = match &under {
        Some(id) => format!("this is show {id}'s own socket; {method} is answered by the station in front of it. Call it on the station's control port."),
        None => format!("this core runs one show on its own, with no station, so {method} has nothing to change. Start GodwinMix without --show to run several."),
    };
    RpcError::not_in_state(message).with("station", under.is_some())
}

/// The one show a core with no station is.
async fn alone(call: &Call) -> Result<ShowList, RpcError> {
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    let id = crate::station::show::mode().map(|m| m.id.clone()).unwrap_or_else(|| "main".into());
    let show = Show {
        id: id.clone(),
        name: "Main".into(),
        state: ShowState::Running,
        on_air: status.scene.clone().or(status.program.clone()),
        programme_kbps: 0,
        cpu_millicores: 0,
        memory_mib: 0,
        restarts: 0,
        error: None,
        compositing: true,
        input: None,
        outputs: Vec::new(),
        health: Default::default(),
    };
    Ok(ShowList { shows: vec![show], current: id })
}
