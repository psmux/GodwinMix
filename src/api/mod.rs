//! The public contract, in one place.
//!
//! Every request, response, event and status type the mixer carries lives
//! here with `serde` and `schemars` derives, beside the method table that
//! describes what can be called and the generator that turns both into
//! `protocol.json`, `protocol.md`, `core.api` and the MCP tool list.
//!
//! The point is that there is one source of truth. Before this module,
//! request shapes were private structs in control.rs, MCP input schemas were
//! written out by hand in mcp.rs, ctl.rs assembled bodies with `json!`, and
//! the README carried a table somebody maintained. Four descriptions of one
//! protocol drift; one description cannot.
//!
//! ```text
//!   src/api/types.rs      status and event records
//!   src/api/requests.rs   request and result bodies
//!   src/api/error.rs      the code table, one error shape
//!   src/api/method.rs     the method table and the REST transform
//!   src/api/protocol.rs   protocol.json, protocol.md, core.api
//!   src/api/rpc.rs        JSON-RPC framing, subscriptions, frame headers
//!   src/api/scope.rs      tokens, scopes, confirmation, rehearsal
//!   src/api/idempotency.rs  the 24 hour replay cache
//!   src/api/trace.rs      one trace id per call
//!   src/api/streams.rs    subscriber counts for the expensive streams
//!   src/api/mcp_tools.rs  the tool list, generated, in two profiles
//! ```
//!
//! See `src/api/README.md` for how another module adds a method.

pub mod error;
pub mod idempotency;
pub mod mcp_tools;
pub mod method;
pub mod protocol;
pub mod requests;
pub mod rpc;
pub mod scope;
pub mod streams;
pub mod trace;
pub mod types;

pub use error::{ErrorCode, RpcError};
pub use method::{MethodDef, Registry, Rest, Tier};
pub use requests::*;
pub use scope::{ConfirmPolicy, Profile, Scope, Token, Tokens};
pub use types::*;

/// What this build speaks.
pub const API_LEVEL: u32 = 1;

/// The oldest level this build still answers. A client that speaks level 1
/// works against every core from here on until this number moves.
pub const API_COMPATIBLE: u32 = 1;

/// No method blocks longer than this before it answers or hands back a task.
/// The tightest client default in the wild, so nothing here can time a client
/// out that used its own default.
pub const MAX_CALL_SECS: u64 = 5;
