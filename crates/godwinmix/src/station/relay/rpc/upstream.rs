//! The show's side of a relayed `/rpc`.

use super::super::pipe;
use super::Relay;
use futures_util::SinkExt;
use godwinmix_protocol::error::RpcError;
use tokio_tungstenite::tungstenite::Message as Up;

impl Relay {
    pub(super) async fn connect(&self) -> Result<pipe::Upstream, RpcError> {
        let addr = self.st.addr_of(&self.show).await?;
        pipe::connect(addr, &self.uri, &self.headers)
            .await
            .map_err(|e| RpcError::internal(format!("show {} would not open /rpc: {e}", self.show)).with("show", self.show.as_str()))
    }

    pub(super) async fn send_show(&mut self, m: Up) {
        if let Some(up) = self.up.as_mut() {
            let _ = up.send(m).await;
        }
    }
}
