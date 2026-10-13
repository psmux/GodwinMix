//! A single process core's own mixer as the one show, `main`, for the
//! programme's outputs into its channels.

use godwinmix_core::config::OutputConfig;
use godwinmix_core::mixer::{Command, MixerHandle};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::requests::AddOutputRequest;
use serde_json::{json, Value};

use super::{Answer, Asker};

/// The only show a single process core has.
const MAIN: &str = "main";

pub struct OwnMixer {
    pub mixer: MixerHandle,
}

fn failed(e: anyhow::Error) -> RpcError {
    RpcError::internal(format!("the mixer said: {e:#}"))
}

impl OwnMixer {
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = params["id"].as_str().unwrap_or_default().to_string();
        match method {
            "core.status" => {
                let status = self.mixer.status().await.map_err(failed)?;
                Ok(json!({"outputs": status.outputs.iter().map(|o| json!({"id": o.id})).collect::<Vec<_>>()}))
            }
            "output.add" => {
                let req: AddOutputRequest = serde_json::from_value(params).map_err(|e| RpcError::invalid_params(e.to_string()))?;
                let cfg: OutputConfig = serde_json::from_value(req.to_config_json()).map_err(|e| RpcError::invalid_params(e.to_string()))?;
                self.mixer.request(|ack| Command::AddOutput(Box::new(cfg), Some(ack))).await.map_err(failed)?;
                Ok(json!({"id": id}))
            }
            "output.set" => {
                let configs = self.mixer.configs().await.map_err(failed)?;
                let mut cfg = configs.outputs.into_iter().find(|o| o.id == id).ok_or_else(|| RpcError::not_found("output", &id, &[]))?;
                cfg.uri = params["uri"].as_str().unwrap_or_default().to_string();
                self.mixer.request(|ack| Command::SetOutput(Box::new(cfg), Some(ack))).await.map_err(failed)?;
                Ok(json!({"id": id}))
            }
            "output.remove" => {
                self.mixer.request(|ack| Command::RemoveOutput(id.clone(), Some(ack))).await.map_err(failed)?;
                Ok(json!({"removed": id}))
            }
            other => Err(RpcError::not_found("method", other, &[])),
        }
    }
}

impl Asker for OwnMixer {
    fn first(&self) -> String {
        MAIN.into()
    }

    fn shows(&self) -> Vec<(String, Option<String>)> {
        vec![(MAIN.into(), None)]
    }

    fn ask<'a>(&'a self, _show: &'a str, method: &'a str, params: Value) -> Answer<'a> {
        Box::pin(self.call(method, params))
    }
}
