//! The viewer key: the read only way into one HLS output.
//!
//! Derived rather than stored, so the link a person handed out still works
//! after a restart: an HMAC of the output's id under this machine's secret
//! key (`~/.godwinmix/secrets/key`, the one the secret store already made).
//! Nobody without that file can work a key out from an output's name. A
//! machine with no such file yet gets a random key for the life of the
//! output, and `params.viewer_key` overrides both.
//!
//! To change a link that has leaked, give the output a new name, or set
//! `viewer_key`.

use ring::hmac;

const LEN: usize = 24;
const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";

/// The key for `output_id` on this machine.
pub fn viewer_key(output_id: &str) -> anyhow::Result<String> {
    let path = godwinmix_host::home::secrets_dir().join("key");
    match std::fs::read(&path) {
        Ok(secret) if secret.len() == 32 => Ok(derive(&secret, output_id)),
        _ => crate::secrets::random_key(LEN),
    }
}

fn derive(secret: &[u8], output_id: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let tag = hmac::sign(&key, format!("godwinmix hls viewer key\0{output_id}").as_bytes());
    tag.as_ref().iter().take(LEN).map(|b| ALPHABET[(b & 31) as usize] as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_the_same_every_time_and_different_per_output() {
        let secret = [7u8; 32];
        let a = derive(&secret, "viewers");
        assert_eq!(a, derive(&secret, "viewers"));
        assert_ne!(a, derive(&secret, "church"));
        assert_ne!(a, derive(&[8u8; 32], "viewers"));
        assert_eq!(a.len(), 24);
        assert!(a.bytes().all(|b| ALPHABET.contains(&b)));
    }
}
