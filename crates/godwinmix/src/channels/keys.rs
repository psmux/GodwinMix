//! Ids, names and keys.

use godwinmix_protocol::error::RpcError;

/// How long a made key is. See `secrets::random_key`.
pub const KEY_LEN: usize = 24;

/// Where a channel's keys are sealed in the secret store: one scope per
/// channel, so removing the channel forgets them all in one call.
pub fn scope(channel: &str) -> String {
    format!("channel.{channel}")
}

/// A slug: lower case letters, digits and dashes, starting with a letter.
pub fn slug(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.trim().chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    match out.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => out,
        Some(_) => format!("c-{out}"),
        None => String::new(),
    }
}

/// `base`, or `base-2`, `base-3` and so on, whichever nothing has taken.
pub fn free(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !taken(id))
        .expect("the integers run out after the ids do")
}

/// An application name must be something every encoder accepts in a URL.
pub fn check_app(app: &str) -> Result<(), RpcError> {
    let ok = !app.is_empty()
        && app.len() <= 64
        && app.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && app.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
    if ok {
        return Ok(());
    }
    Err(RpcError::invalid_params(format!(
        "'{app}' cannot be an RTMP application name. Use letters, digits, dashes and \
         underscores, up to 64 of them, starting with a letter or digit: sunday-service, \
         youth_2."
    ))
    .with("field", "app"))
}

/// The last four characters, for a person to tell keys apart by.
pub fn hint(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    chars[chars.len().saturating_sub(4)..].iter().collect()
}

pub fn now() -> String {
    godwinmix_core::observe::logs::rfc3339(&std::time::SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_becomes_a_slug_that_starts_with_a_letter() {
        assert_eq!(slug("Sunday Service"), "sunday-service");
        assert_eq!(slug("  Youth / Hall 2 "), "youth-hall-2");
        assert_eq!(slug("2024 conference"), "c-2024-conference");
        assert_eq!(slug("!!!"), "");
    }

    #[test]
    fn a_taken_id_gets_the_next_number() {
        let taken = ["main", "main-2"];
        assert_eq!(free("main", |id| taken.contains(&id)), "main-3");
        assert_eq!(free("cam", |id| taken.contains(&id)), "cam");
    }

    #[test]
    fn an_app_name_an_encoder_would_choke_on_is_refused_with_examples() {
        assert!(check_app("sunday-service").is_ok());
        assert!(check_app("youth_2").is_ok());
        let err = check_app("a b").unwrap_err();
        assert!(err.message.contains("sunday-service"), "{}", err.message);
        assert!(check_app("").is_err());
    }

    #[test]
    fn a_hint_is_the_last_four_characters_and_never_more() {
        assert_eq!(hint("abcdefgh"), "efgh");
        assert_eq!(hint("ab"), "ab");
    }
}
