//! Where a passphrase sits in an input: the address's query or `params`, of
//! the main input or of its backup.

use godwinmix_protocol::shows::{BackupInput, InputSpec};
use serde_json::{Map, Value};

/// The one field that is a secret, in an address's query or in `params`.
const FIELD: &str = "passphrase";

/// The places a passphrase can be: (slot in the store, is it the backup,
/// is it in the address rather than params).
pub const PLACES: [(&str, bool, bool); 4] =
    [("uri", false, true), ("params", false, false), ("backup.uri", true, true), ("backup.params", true, false)];

/// One place's value: what the address's query or `params` holds.
pub fn read(input: &InputSpec, backup: bool, in_uri: bool) -> Option<String> {
    let (uri, params) = match (backup, &input.backup) {
        (false, _) => (&input.uri, &input.params),
        (true, Some(b)) => (&b.uri, &b.params),
        (true, None) => return None,
    };
    match in_uri {
        true => query(uri),
        false => params.as_ref()?.get(FIELD)?.as_str().map(str::to_string),
    }
}

/// Put `value` in one place, or take the field out with None.
pub fn write(input: &mut InputSpec, backup: bool, in_uri: bool, value: Option<&str>) {
    let (uri, params) = match (backup, input.backup.as_mut()) {
        (false, _) => (&mut input.uri, &mut input.params),
        (true, Some(BackupInput { uri, params, .. })) => (uri, params),
        (true, None) => return,
    };
    match in_uri {
        true => *uri = with_query(uri, value),
        false => {
            let map = params.get_or_insert_with(Map::new);
            match value {
                Some(v) => map.insert(FIELD.into(), Value::from(v)),
                None => map.remove(FIELD),
            };
            if map.is_empty() {
                *params = None;
            }
        }
    }
}

/// The passphrase in an address's query, as written.
pub fn query(uri: &str) -> Option<String> {
    let (_, q) = uri.split_once('?')?;
    q.split('&').find_map(|kv| kv.strip_prefix("passphrase=")).map(str::to_string)
}

/// The address with its passphrase replaced, or taken out with None.
pub fn with_query(uri: &str, value: Option<&str>) -> String {
    let Some((base, q)) = uri.split_once('?') else { return uri.to_string() };
    let pairs: Vec<String> = q
        .split('&')
        .filter_map(|kv| match kv.strip_prefix("passphrase=") {
            Some(_) => value.map(|v| format!("{FIELD}={v}")),
            None => Some(kv.to_string()),
        })
        .collect();
    match pairs.is_empty() {
        true => base.to_string(),
        false => format!("{base}?{}", pairs.join("&")),
    }
}
