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
///
/// A single space between words is allowed, because Livebox allowed one and
/// a channel moved from it keeps the address its encoders already have. An
/// encoder carries it as `%20` (see `in_url`), and the listener decodes
/// that before it compares.
pub fn check_app(app: &str) -> Result<(), RpcError> {
    let ok = !app.is_empty()
        && app.len() <= 64
        && app.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ')
        && !app.contains("  ")
        && app.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && !app.ends_with(' ');
    if ok {
        return Ok(());
    }
    Err(RpcError::invalid_params(format!(
        "'{app}' cannot be an RTMP application name. Use letters, digits, dashes, \
         underscores and single spaces, up to 64 of them, starting with a letter or digit: \
         Church, sunday-service, Youth Hall."
    ))
    .with("field", "app")
    .with("max_len", 64))
}

/// Two application names are one if they differ only in case. Encoders are
/// set up by hand, and `Church` typed as `church` should still arrive; the
/// listener matches the same way, so two channels may not differ only so.
pub fn same_app(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// An application name as an encoder must send it. OBS and ffmpeg end the
/// address at a raw space and read the rest as options (ffmpeg, given
/// `rtmp://host/Youth Hall/main`, asks for the application `Youth`), so an
/// address shown to a person carries the space as `%20`, which the listener
/// decodes.
pub fn in_url(app: &str) -> String {
    app.replace(' ', "%20")
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
        assert!(check_app("Church").is_ok(), "capitals are kept as the encoders type them");
        assert!(check_app("Youth Hall").is_ok(), "one space, as Livebox allowed");
        let err = check_app("a/b").unwrap_err();
        assert!(err.message.contains("sunday-service"), "{}", err.message);
        assert!(check_app("").is_err());
        assert!(check_app("two  spaces").is_err());
        assert!(check_app(" lead").is_err());
    }

    #[test]
    fn application_names_that_differ_only_in_case_are_the_same() {
        assert!(same_app("Church", "church"));
        assert!(same_app("Youth Hall", "YOUTH HALL"));
        assert!(!same_app("church", "church-2"));
    }

    #[test]
    fn a_hint_is_the_last_four_characters_and_never_more() {
        assert_eq!(hint("abcdefgh"), "efgh");
        assert_eq!(hint("ab"), "ab");
    }
}
