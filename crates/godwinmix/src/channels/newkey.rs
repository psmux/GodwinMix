//! Making a key: one the mixer draws at random, or one a person typed because
//! their encoders already send it (a password moved over from Livebox, say).
//!
//! Either way the secret goes to the secret store and nowhere else. The
//! channels file gets its id, label and last four characters, and a typed
//! one is marked `imported` so a list can say it was not made here. Nothing
//! in this module puts a secret in an error, a log line or an event.

use godwinmix_protocol::channels::NewKey;
use godwinmix_protocol::error::RpcError;

use super::keys::{self, free, slug};
use super::store::KeyRecord;
use super::Channels;

/// The shortest secret a person may type. A generated one is `KEY_LEN`.
pub const MIN_LEN: usize = 6;
/// The longest, which is far past anything an encoder's box holds.
pub const MAX_LEN: usize = 128;
/// What may be in one besides letters and digits. These four never need
/// escaping in a URL query; a space does, but encoders send it either raw or
/// as `%20` or `+`, and the listener takes all three.
const MARKS: &str = "-_.~ ";

impl Channels {
    /// Make a key, seal it, and add its record. `typed` is a secret a person
    /// chose, already through `check_secret`; without one the mixer makes
    /// one. Answers with the secret, which `channel.key.reveal` can read back
    /// later from the store.
    pub(super) fn make_key(&self, channel: &str, label: Option<String>, typed: Option<String>) -> Result<NewKey, RpcError> {
        let imported = typed.is_some();
        let secret = match typed {
            Some(s) => s,
            None => godwinmix_core::secrets::random_key(keys::KEY_LEN)
                .map_err(|e| RpcError::internal(format!("making a key: {e:#}")))?,
        };
        let mut records = self.records.lock();
        let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
        let record = records
            .iter_mut()
            .find(|r| r.id == channel)
            .ok_or_else(|| RpcError::not_found("channel", channel, &ids))?;
        if imported {
            let scope = keys::scope(channel);
            let twin = record.keys.iter().find(|k| self.secrets.get(&scope, &k.id).as_deref() == Some(secret.as_str()));
            if let Some(twin) = twin {
                return Err(already(channel, &twin.id));
            }
        }
        let label = label
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| format!("Key {}", record.keys.len() + 1));
        let base = match slug(&label) {
            s if s.is_empty() => "key".to_string(),
            s => s,
        };
        let id = free(&base, |id| record.keys.iter().any(|k| k.id == id));
        self.secrets
            .set(&keys::scope(channel), &id, &secret)
            .map_err(|e| RpcError::internal(format!("sealing the key: {e:#}")))?;
        let hint = keys::hint(&secret);
        record.keys.push(KeyRecord { id: id.clone(), label: label.clone(), created: keys::now(), hint, imported });
        Ok(NewKey { id, label, secret })
    }
}

/// A typed secret, trimmed, or the rule it broke. The message never repeats
/// the secret: it would end up in a toast, a log or an agent's transcript.
pub fn check_secret(raw: &str) -> Result<String, RpcError> {
    let secret = raw.trim().to_string();
    let len = secret.chars().count();
    let fits = secret.chars().all(|c| c.is_ascii_alphanumeric() || MARKS.contains(c));
    if (MIN_LEN..=MAX_LEN).contains(&len) && fits {
        return Ok(secret);
    }
    let why = if !fits {
        "it has a character an RTMP address cannot carry after ?psk= as it is".to_string()
    } else if len < MIN_LEN {
        format!("it is {len} characters long, and a key needs at least {MIN_LEN}")
    } else {
        format!("it is {len} characters long, and a key can have at most {MAX_LEN}")
    };
    Err(RpcError::invalid_params(format!(
        "that secret cannot be a key: {why}. Use {MIN_LEN} to {MAX_LEN} letters, digits, dashes, \
         underscores, dots, tildes or spaces, or leave it out and the mixer makes one."
    ))
    .with("field", "secret")
    .with("min_len", MIN_LEN)
    .with("max_len", MAX_LEN)
    .with("allowed", "A-Z a-z 0-9 - _ . ~ space"))
}

fn already(channel: &str, key: &str) -> RpcError {
    RpcError::invalid_params(format!(
        "the channel '{channel}' already has that secret, as its key '{key}'. Encoders that \
         send it are already let in; nothing needs adding."
    ))
    .with("field", "secret")
    .with("channel", channel)
    .with("key", key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_from_livebox_is_kept_as_typed_less_the_ends() {
        assert_eq!(check_secret("Sunday-2024").unwrap(), "Sunday-2024");
        assert_eq!(check_secret("  my church pw ").unwrap(), "my church pw");
        assert_eq!(check_secret("a.b_c~d").unwrap(), "a.b_c~d");
    }

    #[test]
    fn a_short_secret_is_refused_with_the_rule_and_the_numbers() {
        let err = check_secret("abc").unwrap_err();
        assert!(err.message.contains("at least 6"), "{}", err.message);
        assert_eq!(err.data["min_len"], 6);
        assert_eq!(err.data["max_len"], 128);
        assert_eq!(err.data["field"], "secret");
        assert!(check_secret(&"x".repeat(129)).unwrap_err().message.contains("at most 128"));
    }

    #[test]
    fn a_character_a_query_cannot_carry_is_refused_without_repeating_the_secret() {
        for bad in ["pass&word", "pass=word", "pass?word", "pass/word", "pass#word", "pässword", "pass+word"] {
            let err = check_secret(bad).unwrap_err();
            assert!(err.message.contains("cannot carry"), "{bad}: {}", err.message);
            assert!(!err.message.contains(bad), "the secret is never echoed: {}", err.message);
            assert!(!err.data.to_string().contains(bad));
        }
    }
}

#[cfg(test)]
#[path = "typed_tests.rs"]
mod typed_tests;
