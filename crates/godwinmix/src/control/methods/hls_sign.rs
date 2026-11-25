//! `hls.sign`: a viewer link to one HLS output that runs out, and can be held
//! to one viewer's address.
//!
//! For a link handed to someone who should watch for an evening and no
//! longer: a paying viewer, a guest's family, a reviewer. The link carries its
//! own expiry and signature (`godwinmix_core::hls::signed`), so nothing is
//! stored and nothing has to be revoked; changing the output's viewer key ends
//! every signed link to it at once.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::hls::signed::MAX_TTL_SECS;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SignRequest {
    /// The HLS output, by id.
    pub output: String,
    /// How long the link works, in seconds: 3600 when left out, at most
    /// 2592000 (thirty days).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<u64>,
    /// An address the link opens from and from nowhere else, for a link meant
    /// for one viewer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SignedLink {
    /// The multivariant playlist's path with the signed query on it. Put the
    /// page's origin in front of it for a link.
    pub master_url_path: String,
    /// When it stops working, in Unix seconds.
    pub expires: u64,
}

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "hls.sign",
            Scope::Operate,
            "A viewer link to one HLS output that stops working after a time, and can be held to one address.",
            handler(sign),
        )
        .params(schema_of::<SignRequest>)
        .result(schema_of::<SignedLink>)
        .tool(
            "sign_viewer_link",
            Tier::Search,
            "Make a watch link to one HLS output that runs out after `ttl_secs` (an hour unless given, a month \
             at most), and with `ip` opens from that address only. Returns its path; put the page's origin \
             in front of it.",
        ),
    );
}

async fn sign(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: SignRequest = call.params(&params)?;
    let ttl = req.ttl_secs.unwrap_or(3600);
    if ttl == 0 || ttl > MAX_TTL_SECS {
        return Err(RpcError::invalid_params(format!(
            "ttl_secs is 1 to {MAX_TTL_SECS} (thirty days), not {ttl}. Give a shorter time, and send a new link when it runs out."
        ))
        .with("field", "ttl_secs")
        .with("maximum", MAX_TTL_SECS));
    }
    if let Some(ip) = &req.ip {
        if ip.parse::<std::net::IpAddr>().is_err() {
            return Err(RpcError::invalid_params(format!("ip `{ip}` is not an address. Give one such as 203.0.113.7, or leave it out."))
                .with("field", "ip"));
        }
    }
    let Some(stream) = godwinmix_core::hls::stream::get(&req.output) else {
        let have = godwinmix_core::hls::stream::ids();
        return Err(RpcError::not_found("HLS output", &req.output, &have).with("outputs", json!(have)));
    };
    let (master_url_path, expires) = stream.signed_url_path(ttl, req.ip.as_deref());
    body(SignedLink { master_url_path, expires })
}
