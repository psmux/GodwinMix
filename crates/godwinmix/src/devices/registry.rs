//! The device registry's half that `Tokens` and the three methods call.

use godwinmix_protocol::devices::{DeviceRegistry, DeviceToken, TokenCreateRequest, TokenCreated};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::scope::Token;

use super::calls::Wanted;
use super::store::{self, Record};
use super::{Devices, SECRET_PREFIX};

impl DeviceRegistry for Devices {
    fn find(&self, presented: &str) -> Option<Token> {
        let digest = store::digest(presented);
        let mut state = self.state.lock();
        self.refresh(&mut state, false);
        if let Some(token) = self.lookup(&state, &digest) {
            return Some(token);
        }
        // Unknown here, but another process may have just made it.
        self.refresh(&mut state, true);
        self.lookup(&state, &digest)
    }

    fn revoked(&self, token: &Token) -> bool {
        let Some(digest) = token.secret.strip_prefix(SECRET_PREFIX) else { return false };
        let mut state = self.state.lock();
        self.refresh(&mut state, false);
        !state.records.iter().any(|r| r.sha256 == digest)
    }

    fn create(&self, req: TokenCreateRequest, taken: &dyn Fn(&str) -> bool) -> Result<TokenCreated, RpcError> {
        let wanted = Wanted::check(req)?;
        let secret = godwinmix_core::secrets::random_key(40)
            .map_err(|e| RpcError::internal(format!("{e:#}. No token was made; try again.")))?;
        let mut state = self.state.lock();
        self.refresh(&mut state, true);
        let used = |id: &str| taken(id) || state.records.iter().any(|r| r.id == id);
        let id = wanted.id(&used)?;
        let record = Record {
            id,
            label: wanted.label,
            scope: wanted.scope,
            created: crate::channels::keys::now(),
            sha256: store::digest(&secret),
        };
        state.records.push(record.clone());
        if let Err(e) = self.persist(&mut state) {
            state.records.pop();
            return Err(e);
        }
        tracing::info!(id = %record.id, scope = record.scope.as_str(), "device token made");
        Ok(TokenCreated { device: record.public(), token: secret })
    }

    fn list(&self) -> Vec<DeviceToken> {
        let mut state = self.state.lock();
        self.refresh(&mut state, true);
        state.records.iter().map(Record::public).collect()
    }

    fn revoke(&self, id: &str) -> Result<DeviceToken, RpcError> {
        let mut state = self.state.lock();
        self.refresh(&mut state, true);
        let Some(at) = state.records.iter().position(|r| r.id == id) else {
            let known: Vec<String> = state.records.iter().map(|r| r.id.clone()).collect();
            let there = if known.is_empty() { "There are none".to_string() } else { format!("There are: {}", known.join(", ")) };
            return Err(RpcError::new(
                godwinmix_protocol::ErrorCode::NotFound,
                format!(
                    "there is no device token '{id}'. {there}. A token in the config file's \
                     [[tokens]] table is not a device token; take it out of the file instead."
                ),
            )
            .with("id", id)
            .with("kind", "device token")
            .with("valid", known));
        };
        let gone = state.records.remove(at);
        if let Err(e) = self.persist(&mut state) {
            state.records.insert(at, gone);
            return Err(e);
        }
        tracing::info!(id = %gone.id, "device token revoked");
        Ok(gone.public())
    }
}
