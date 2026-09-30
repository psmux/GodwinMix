//! The station calling a show itself, over the same public protocol any
//! client uses, with the credential the show was given at start (its link
//! secret, which a show under a station accepts as an admin token when it
//! has tokens at all).

use super::relay::pipe::Upstream;
use super::state::Station;
use futures_util::{SinkExt, StreamExt};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::MixerStatus;
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

impl Station {
    fn secret_of(&self, id: &str) -> String {
        self.procs.lock().get(id).map(|p| p.secret.clone()).unwrap_or_default()
    }

    /// `core.status` of a running show, or None when it did not answer in
    /// `wait`.
    pub async fn status_of(&self, id: &str, wait: Duration) -> Option<MixerStatus> {
        let addr = (*self.procs.lock().get(id)?.addr.borrow())?;
        let answer = self
            .http
            .get(format!("http://{addr}/api/v1/core/status"))
            .bearer_auth(self.secret_of(id))
            .timeout(wait)
            .send()
            .await
            .ok()?;
        answer.json::<MixerStatus>().await.ok()
    }

    /// One JSON-RPC call on a show's `/rpc`, answered within `wait`.
    pub async fn ask_show(&self, id: &str, method: &str, params: Value, wait: Duration) -> Result<Value, RpcError> {
        let addr = self.addr_of(id).await?;
        let secret = self.secret_of(id);
        let call = async {
            let mut request = format!("ws://{addr}/rpc").into_client_request().map_err(|e| e.to_string())?;
            let bearer = format!("Bearer {secret}").parse().map_err(|_| "a secret that is not a header".to_string())?;
            request.headers_mut().insert("authorization", bearer);
            let (mut ws, _): (Upstream, _) = tokio_tungstenite::connect_async(request).await.map_err(|e| e.to_string())?;
            let frame = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
            ws.send(Message::Text(frame.to_string().into())).await.map_err(|e| e.to_string())?;
            while let Some(Ok(m)) = ws.next().await {
                let Message::Text(text) = m else { continue };
                let v: Value = serde_json::from_str(text.as_str()).unwrap_or_default();
                if v.get("id") == Some(&json!(1)) {
                    let _ = ws.close(None).await;
                    return Ok(v);
                }
            }
            Err("the show closed the connection before it answered".to_string())
        };
        let reply = match tokio::time::timeout(wait, call).await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => return Err(RpcError::internal(format!("show {id} could not be asked {method}: {e}")).with("show", id)),
            Err(_) => return Err(RpcError::internal(format!("show {id} did not answer {method} within {} seconds", wait.as_secs())).with("show", id)),
        };
        match reply.get("error") {
            Some(e) => Err(serde_json::from_value::<RpcError>(e.clone()).unwrap_or_else(|_| RpcError::internal(e.to_string()))),
            None => Ok(reply.get("result").cloned().unwrap_or(Value::Null)),
        }
    }
}
