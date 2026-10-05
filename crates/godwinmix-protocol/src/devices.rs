//! Device tokens: a credential an admin makes while the core runs, for one
//! phone or tablet, and takes back with one call.
//!
//! The configured `[[tokens]]` table never changes while the core runs, and a
//! show run from three phones should not need a config edit and a restart per
//! phone. So `token.create` makes one, hands its secret back once, and the
//! core keeps only a digest of it. The types and the seam are here; the store
//! that hashes, persists and reloads is in the binary, because this crate has
//! no file system and no hash function.

use crate::error::RpcError;
use crate::scope::{Scope, Token};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `token.create`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TokenCreateRequest {
    /// What a person calls the device: "Sam's phone". Defaults to "Phone".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// A slug to use as the id. Made from the label when absent, with `-2`,
    /// `-3` on the end when that one is taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// `read`, `operate` (the default) or `admin`.
    #[serde(default = "operate")]
    pub scope: Scope,
}

fn operate() -> Scope {
    Scope::Operate
}

/// One device token, without its secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeviceToken {
    /// Recorded against every take in `program.history`, like any token id.
    pub id: String,
    pub label: String,
    pub scope: Scope,
    /// When it was made, RFC 3339 in UTC.
    pub created: String,
}

/// What `token.create` answers with. The only time the secret is shown.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TokenCreated {
    #[serde(flatten)]
    pub device: DeviceToken,
    /// The secret. Send it as `Authorization: Bearer <token>`, or open the
    /// page at `https://<host>:<port>/#token=<token>`. It cannot be read back.
    pub token: String,
}

/// `token.list`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TokenList {
    pub tokens: Vec<DeviceToken>,
}

/// `token.revoke`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TokenRevokeRequest {
    /// The device token's id, from `token.list`.
    pub id: String,
}

/// What `token.revoke` answers with.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TokenRevoked {
    pub revoked: DeviceToken,
}

/// Where device tokens are kept and checked. `Tokens` asks it about any
/// secret the configured and minted lists do not know.
///
/// Every method is quick and never blocks on anything but a small file: it
/// is called on the request path of every call that presents an unknown
/// secret.
pub trait DeviceRegistry: Send + Sync + std::fmt::Debug {
    /// The token for a presented secret, when it is a live device token.
    fn find(&self, presented: &str) -> Option<Token>;
    /// True when `token` came from here and has since been revoked, so a
    /// connection that authenticated before the revoke is refused after it.
    fn revoked(&self, token: &Token) -> bool;
    /// Make one. `taken` says whether an id is already some other token's.
    fn create(&self, req: TokenCreateRequest, taken: &dyn Fn(&str) -> bool) -> Result<TokenCreated, RpcError>;
    fn list(&self) -> Vec<DeviceToken>;
    fn revoke(&self, id: &str) -> Result<DeviceToken, RpcError>;
}
