//! The certificate RTMPS answers with: uploaded from the page, or made here,
//! self signed. One for the whole mixer, whichever channels turn RTMPS on.
//!
//! The certificate and its key are sealed in the secret store, beside the
//! channels' keys; the channels file keeps only what a page shows (where it
//! came from, its names, its fingerprint). The listener gets the pair in its
//! settings under `tls`, the way it gets the keys.

use godwinmix_core::tls_cert::{self, Pair};
use godwinmix_protocol::channels::{CertificateGenerateRequest, CertificateInfo, CertificateSetRequest};
use godwinmix_protocol::error::RpcError;
use serde_json::json;
use tracing::warn;

use super::{keys, net, Channels, PLUGIN};

/// Where the pair is sealed.
const SCOPE: &str = "channels.tls";

impl Channels {
    /// `channel.certificate.set`: a certificate a person uploaded.
    pub fn certificate_set(&self, req: CertificateSetRequest) -> Result<CertificateInfo, RpcError> {
        let pair = Pair { cert: req.cert.trim().to_string(), key: req.key.trim().to_string() };
        tls_cert::check(&pair).map_err(|e| RpcError::invalid_params(format!("{e:#}")).with("field", "cert"))?;
        self.keep_certificate(pair, "uploaded", Vec::new())
    }

    /// `channel.certificate.generate`: a self signed one for this machine.
    pub fn certificate_generate(&self, req: CertificateGenerateRequest) -> Result<CertificateInfo, RpcError> {
        let names = if req.names.is_empty() {
            let mut names = net::hosts();
            names.push("localhost".into());
            names
        } else {
            req.names
        };
        let pair = tls_cert::self_signed(&names).map_err(|e| RpcError::internal(format!("{e:#}")))?;
        self.keep_certificate(pair, "self_signed", names)
    }

    fn keep_certificate(&self, pair: Pair, source: &str, names: Vec<String>) -> Result<CertificateInfo, RpcError> {
        let fingerprint = tls_cert::fingerprint(&pair.cert).map_err(|e| RpcError::invalid_params(format!("{e:#}")))?;
        let seal = |field: &str, value: &str| {
            self.secrets.set(SCOPE, field, value).map_err(|e| RpcError::internal(format!("sealing the certificate: {e:#}")))
        };
        seal("cert", &pair.cert)?;
        seal("key", &pair.key)?;
        let info = CertificateInfo { source: source.into(), names, fingerprint, created: keys::now() };
        *self.certificate.lock() = Some(info.clone());
        self.commit(None)?;
        // Every channel with RTMPS on shows the new state.
        let ids: Vec<String> = self.records.lock().iter().filter(|r| r.rtmps.enabled).map(|r| r.id.clone()).collect();
        for id in ids {
            self.announce(&id);
        }
        Ok(info)
    }

    /// Lay the pair over the listener's settings, or take it off.
    pub(super) fn hand_over_tls(&self) {
        let pair = self.certificate.lock().as_ref().and_then(|_| {
            let cert = self.secrets.get(SCOPE, "cert")?;
            let key = self.secrets.get(SCOPE, "key")?;
            Some(json!({"cert": cert, "key": key}))
        });
        let value = pair.and_then(|p| toml::Value::try_from(p).map_err(|e| warn!(%e, "the certificate would not convert")).ok());
        self.plugins().set_extra(PLUGIN, "tls", value);
    }
}
