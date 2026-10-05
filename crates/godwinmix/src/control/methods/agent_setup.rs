//! `agent.tools` and `agent.setup`: the Set up buttons in Help > Connect an
//! AI agent. See `crate::agents`.
//!
//! Both are admin: they read and write files in the home folder of whoever
//! runs this mixer. A setup changes another program's configuration, so it is
//! marked destructive, answers `dry_run` with every file it would write, and
//! on a token whose policy asks for it wants a confirmation first.

use super::{body, handler};
use crate::agents::{self, Dirs, SetupRequest};
use crate::control::call::Call;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "agent.tools",
            Scope::Admin,
            "The AI agent tools this mixer can set up, the ones installed on its machine \
             first, each with what was found: a command on PATH or a config folder.",
            handler(|_call: Call, _| async move {
                let dirs = dirs()?;
                body(agents::detect::detect(&dirs, std::env::var_os("PATH").as_deref()))
            }),
        )
        .result(schema_of::<Vec<agents::detect::Detected>>)
        .mutating(false),
    );
    reg.register(
        MethodDef::new(
            "agent.setup",
            Scope::Admin,
            "Set an AI agent tool up to use this mixer: its MCP config gets one entry, \
             godwinmix, that runs this mixer's own executable, and its skills folder gets \
             the GodwinMix skills. Other entries are kept and a changed file is copied \
             aside first. dry_run answers every file it would write. The answer says how \
             to start the tool and a first thing to ask it.",
            handler(setup),
        )
        .params(schema_of::<SetupRequest>)
        .result(schema_of::<agents::Setup>)
        .destructive(),
    );
}

fn dirs() -> Result<Dirs, RpcError> {
    Dirs::from_env().ok_or_else(|| {
        RpcError::internal("this mixer has no home folder (HOME or USERPROFILE is unset), so there is nowhere to set an agent up. Start it from a user account")
    })
}

async fn setup(call: Call, params: Value) -> Result<Value, RpcError> {
    let mut req: SetupRequest = call.params(&params)?;
    req.dry_run = call.dry_run;
    let exe = std::env::current_exe().map_err(|e| RpcError::internal(format!("this mixer cannot find its own executable: {e}")))?;
    let dirs = dirs()?;
    let mut setup = agents::plan(&req, &exe, &dirs).map_err(RpcError::invalid_params)?;
    if crate::address::desktop().is_none() && req.env.is_empty() {
        setup.notes.push(
            "This mixer is not the desktop app's, so `godwinmix mcp` will not find it by itself. \
             Set it up again with env GODWINMIX_URL (and GODWINMIX_TOKEN if it has one)."
                .into(),
        );
    }
    if req.dry_run {
        let diff = setup.writes.iter().map(|w| format!("{:?} {} ({})", w.action, w.path, w.what).to_lowercase()).collect();
        let changes = setup.writes.iter().any(|w| w.action != agents::merge::Action::Unchanged);
        let mut answer = call.dry_run_answer(changes, diff);
        merge_into(&mut answer, serde_json::to_value(&setup).unwrap_or_default());
        return Ok(answer);
    }
    agents::apply(&mut setup).map_err(RpcError::internal)?;
    body(setup)
}

fn merge_into(answer: &mut Value, extra: Value) {
    if let (Some(a), Value::Object(e)) = (answer.as_object_mut(), extra) {
        for (k, v) in e {
            a.entry(k).or_insert(v);
        }
    }
}
