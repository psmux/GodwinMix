//! `gpu.status` and `machine.status` as rows of the method table. The same
//! two answer in a single process core and in a station, which answers them
//! itself rather than ask a show.

use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::machine::{GpuStatus, GpuStatusRequest, MachineStatus};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

/// The methods this module answers, for the station's own list.
pub const METHODS: &[&str] = &["gpu.status", "machine.status"];

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "gpu.status",
            Scope::Read,
            "Every NVIDIA GPU on this machine: encoder, decoder and 3D use, video memory, \
             encoder sessions, temperature, power and clocks, now and over the last two \
             minutes. Read once a second from NVML or nvidia-smi while a client keeps \
             asking, and not at all otherwise. No GPUs, and a note, on a machine without one.",
            handler(|call: Call, params: Value| async move { gpu(&call, &params).await }),
        )
        .params(schema_of::<GpuStatusRequest>)
        .result(schema_of::<GpuStatus>),
    );
    reg.register(
        MethodDef::new(
            "machine.status",
            Scope::Read,
            "The machine itself: its name, cores, CPU use of every process, memory, how \
             long it and this process have been up, and its load average.",
            handler(|_call: Call, _| async move { body(super::machine_status().await) }),
        )
        .result(schema_of::<MachineStatus>),
    );
}

async fn gpu(call: &Call, params: &Value) -> Result<Value, RpcError> {
    let req: GpuStatusRequest = call.params(params)?;
    body(super::gpu_status(req.since_ms).await)
}

/// The station's answer to one of [`METHODS`], with the params as sent.
pub async fn answer(method: &str, params: Value) -> Result<Value, RpcError> {
    if method == "machine.status" {
        return body(super::machine_status().await);
    }
    let params = if params.is_null() { Value::Object(Default::default()) } else { params };
    let req: GpuStatusRequest = serde_json::from_value(params)
        .map_err(|e| RpcError::invalid_params(format!("gpu.status could not read its params: {e}. Send {{}} or {{\"since_ms\": <unix ms>}}.")))?;
    body(super::gpu_status(req.since_ms).await)
}
