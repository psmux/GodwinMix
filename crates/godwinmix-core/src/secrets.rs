//! Secrets in plugin settings, encrypted at rest.
//!
//! A settings schema marks a field `"format": "secret"`. A stream key, an NDI
//! password, an SRT passphrase, the bearer token for somebody's WHIP endpoint.
//! Three rules, which are Grafana's `secureJsonData` rules and worth copying
//! because they were arrived at the hard way:
//!
//! 1. The value never goes back to a surface. `plugin.settings.get` returns
//!    the sentinel `"__secret__"` where a secret is set and nothing where one
//!    is not, so a settings form can show "configured" without ever holding
//!    the string.
//! 2. Writing the sentinel back means "leave it alone". That is what makes a
//!    round trip through a form safe: the form did not have the value, so it
//!    cannot lose it.
//! 3. It is encrypted on disk with a key made on first run, kept at mode 0600
//!    beside the store.
//!
//! What this is not: a secret manager. The key is on the same disk as the
//! ciphertext, so this stops a config file in a git repository, a support
//! bundle, or a backup from carrying live credentials. It does not stop
//! somebody who already has the machine. The documentation says so rather than
//! implying more.

use anyhow::{Context, Result};
use parking_lot::Mutex;
use ring::aead;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What a surface sees instead of a secret. Writing it back means "unchanged".
pub const SENTINEL: &str = "__secret__";

/// The JSON Schema `format` that marks a field as one of these.
pub const FORMAT: &str = "secret";

/// One encrypted value, as it sits on disk.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Sealed {
    /// The nonce, hex. Fresh for every write: reusing one with AES-GCM is the
    /// classic way to lose the plaintext.
    nonce: String,
    /// Ciphertext and tag, hex.
    cipher: String,
}

/// The store: one file of sealed values, keyed `<plugin>.<field>`.
pub struct Secrets {
    dir: PathBuf,
    key: aead::LessSafeKey,
    values: Mutex<BTreeMap<String, Sealed>>,
}

impl Secrets {
    /// Open the store under `dir`, making the key on first run.
    ///
    /// `dir` is `~/.godwinmix/secrets`. Two files: `key` and `store.json`.
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("make the secrets directory {}", dir.display()))?;
        let key_path = dir.join("key");
        let bytes = match std::fs::read(&key_path) {
            Ok(bytes) if bytes.len() == 32 => bytes,
            _ => {
                let mut fresh = [0u8; 32];
                getrandom::fill(&mut fresh)
                    .context("the operating system would not give us random bytes for a key")?;
                write_private(&key_path, &fresh)?;
                tracing::info!(path = %key_path.display(), "made the key for secrets at rest");
                fresh.to_vec()
            }
        };
        let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &bytes)
            .map_err(|_| anyhow::anyhow!("the secrets key is not 32 bytes"))?;
        let values = std::fs::read_to_string(dir.join("store.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Ok(Self { dir: dir.to_path_buf(), key: aead::LessSafeKey::new(unbound), values: Mutex::new(values) })
    }

    /// Put one secret away. An empty value removes it.
    pub fn set(&self, plugin: &str, field: &str, value: &str) -> Result<()> {
        let key = format!("{plugin}.{field}");
        if value.is_empty() {
            self.values.lock().remove(&key);
            return self.save();
        }
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).context("random bytes for a nonce")?;
        let mut buffer = value.as_bytes().to_vec();
        self.key
            .seal_in_place_append_tag(
                aead::Nonce::assume_unique_for_key(nonce),
                // The key name is the associated data, so a sealed value moved
                // from one field to another under the same key does not open.
                aead::Aad::from(key.as_bytes()),
                &mut buffer,
            )
            .map_err(|_| anyhow::anyhow!("could not encrypt the secret"))?;
        self.values
            .lock()
            .insert(key, Sealed { nonce: hex(&nonce), cipher: hex(&buffer) });
        self.save()
    }

    /// Read one back. Only the engine calls this, to hand the value to the
    /// plugin that owns it.
    pub fn get(&self, plugin: &str, field: &str) -> Option<String> {
        let key = format!("{plugin}.{field}");
        let sealed = self.values.lock().get(&key).cloned()?;
        let nonce: [u8; 12] = unhex(&sealed.nonce)?.try_into().ok()?;
        let mut buffer = unhex(&sealed.cipher)?;
        let plain = self
            .key
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(key.as_bytes()),
                &mut buffer,
            )
            .ok()?;
        String::from_utf8(plain.to_vec()).ok()
    }

    pub fn has(&self, plugin: &str, field: &str) -> bool {
        self.values.lock().contains_key(&format!("{plugin}.{field}"))
    }

    /// Forget everything one plugin put away. Called by `plugin.remove`.
    pub fn forget(&self, plugin: &str) -> usize {
        let prefix = format!("{plugin}.");
        let removed = {
            let mut values = self.values.lock();
            let before = values.len();
            values.retain(|k, _| !k.starts_with(&prefix));
            before - values.len()
        };
        if removed > 0 {
            let _ = self.save();
        }
        removed
    }

    fn save(&self) -> Result<()> {
        let text = serde_json::to_string_pretty(&*self.values.lock())?;
        write_private(&self.dir.join("store.json"), text.as_bytes())
    }
}

/// Which fields of a settings schema are secret.
///
/// Walks `properties` one level, which is where a settings schema puts them.
/// A nested object holding a secret is not supported and the manifest linter
/// says so rather than this quietly missing it.
pub fn secret_fields(schema: &serde_json::Value) -> Vec<String> {
    let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) else {
        return Vec::new();
    };
    properties
        .iter()
        .filter(|(_, spec)| spec.get("format").and_then(|f| f.as_str()) == Some(FORMAT))
        .map(|(name, _)| name.clone())
        .collect()
}

/// Take the secrets out of a params table on their way to a surface.
///
/// Every secret field is replaced by the sentinel when one is stored, and
/// removed when one is not. This is what `plugin.settings.get` returns.
pub fn hide(params: &mut crate::config::Params, fields: &[String], stored: impl Fn(&str) -> bool) {
    for field in fields {
        if stored(field) {
            params.insert(field.clone(), toml::Value::String(SENTINEL.into()));
        } else {
            params.remove(field);
        }
    }
}

/// Put the secrets back on their way to a plugin, and decide what to store.
///
/// Returns the values that should be written to the store: a field carrying
/// the sentinel is left alone (it came back from a form that never had it),
/// and anything else is a new value.
pub fn take(params: &mut crate::config::Params, fields: &[String]) -> Vec<(String, String)> {
    let mut writes = Vec::new();
    for field in fields {
        let Some(value) = params.remove(field) else { continue };
        let Some(text) = value.as_str() else { continue };
        if text == SENTINEL {
            continue;
        }
        writes.push((field.clone(), text.to_string()));
    }
    writes
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

fn write_private(path: &Path, contents: &[u8]) -> Result<()> {
    std::fs::write(path, contents).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restrict {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (Secrets, PathBuf) {
        let dir = std::env::temp_dir()
            .join(format!("gmx-secrets-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        (Secrets::open(&dir).unwrap(), dir)
    }

    #[test]
    fn a_secret_goes_in_and_comes_back() {
        let (secrets, dir) = store();
        secrets.set("ndi", "password", "hunter2").unwrap();
        assert_eq!(secrets.get("ndi", "password").as_deref(), Some("hunter2"));
        assert!(secrets.has("ndi", "password"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_plaintext_is_not_on_the_disk() {
        let (secrets, dir) = store();
        secrets.set("srt", "passphrase", "a-very-distinctive-string").unwrap();
        let on_disk = std::fs::read_to_string(dir.join("store.json")).unwrap();
        assert!(
            !on_disk.contains("a-very-distinctive-string"),
            "the store held the plaintext: {on_disk}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn it_survives_a_restart() {
        let (secrets, dir) = store();
        secrets.set("ndi", "password", "hunter2").unwrap();
        drop(secrets);
        let again = Secrets::open(&dir).unwrap();
        assert_eq!(again.get("ndi", "password").as_deref(), Some("hunter2"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_secret_moved_to_another_field_does_not_open() {
        let (secrets, dir) = store();
        secrets.set("ndi", "password", "hunter2").unwrap();
        let sealed = secrets.values.lock().get("ndi.password").cloned().unwrap();
        secrets.values.lock().insert("ndi.other".into(), sealed);
        assert_eq!(
            secrets.get("ndi", "other"),
            None,
            "the field name is the associated data, so this must not open"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removing_a_plugin_forgets_its_secrets() {
        let (secrets, dir) = store();
        secrets.set("ndi", "password", "a").unwrap();
        secrets.set("ndi", "other", "b").unwrap();
        secrets.set("srt", "passphrase", "c").unwrap();
        assert_eq!(secrets.forget("ndi"), 2);
        assert!(secrets.get("srt", "passphrase").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_schema_says_which_fields_are_secret() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "password": { "type": "string", "format": "secret" }
            }
        });
        assert_eq!(secret_fields(&schema), vec!["password".to_string()]);
        assert!(secret_fields(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn a_form_round_trip_does_not_lose_the_value() {
        let fields = vec!["password".to_string()];
        let mut to_surface: crate::config::Params =
            toml::from_str("name = \"CAM 1\"\npassword = \"hunter2\"").unwrap();
        hide(&mut to_surface, &fields, |_| true);
        assert_eq!(to_surface["password"].as_str(), Some(SENTINEL));
        // The surface sends back what it was given.
        let mut coming_back = to_surface.clone();
        let writes = take(&mut coming_back, &fields);
        assert!(writes.is_empty(), "the sentinel means leave it alone");
        assert!(!coming_back.contains_key("password"), "and it never reaches the plugin as itself");
    }

    #[test]
    fn a_new_value_from_a_form_is_stored() {
        let fields = vec!["password".to_string()];
        let mut params: crate::config::Params = toml::from_str("password = \"new one\"").unwrap();
        let writes = take(&mut params, &fields);
        assert_eq!(writes, vec![("password".to_string(), "new one".to_string())]);
    }

    #[test]
    fn a_field_with_no_secret_stored_is_absent_rather_than_blank() {
        let fields = vec!["password".to_string()];
        let mut params: crate::config::Params = toml::from_str("password = \"\"").unwrap();
        hide(&mut params, &fields, |_| false);
        assert!(!params.contains_key("password"));
    }
}
