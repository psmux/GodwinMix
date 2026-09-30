//! Whether an SRT caller is let in, decided in libsrt's listen callback,
//! before the handshake finishes.
//!
//! The key can come two ways. In the stream id (`?psk=<key>`), in which case
//! the table checks it exactly as it does for RTMP and the connection is not
//! encrypted. Or as the SRT passphrase, which is how most SRT encoders are
//! set up: then the listener cannot see the key at all, only whether the
//! caller's passphrase matches the one it was given for this connection. So
//! the callback picks the channel's key (the one `u=` names, or else the
//! channel's first key) and sets it as this connection's passphrase, and
//! libsrt refuses a caller whose passphrase is anything else.

use crate::channels::{Admit, Protocol, Table};
use crate::hub::Hub;

use super::streamid::Route;

/// SRT's rejection codes for access control, from `access_control.h`.
pub const UNAUTHORIZED: i32 = 1401;
pub const FORBIDDEN: i32 = 1403;
pub const NOT_FOUND: i32 = 1404;
pub const CONFLICT: i32 = 1409;
pub const BAD_REQUEST: i32 = 1400;

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Let in as `admit`, with this passphrase set on the connection when
    /// the key is to be the passphrase.
    Take { admit: Admit, passphrase: Option<String> },
    Refuse { code: i32, channel: String, stream: String, why: String },
    /// A player (`m=request`): send it `admit`'s stream, with this
    /// passphrase set when the key is to be the passphrase.
    Play { admit: Admit, passphrase: Option<String> },
}

pub fn decide(table: &Table, hub: &Hub, route: &Route) -> Decision {
    let refuse = |code, why: String| Decision::Refuse {
        code,
        channel: route.app.clone(),
        stream: route.name().to_string(),
        why,
    };
    if !route.publish {
        return super::play::decide(table, hub, route);
    }
    let channel = table.channels.iter().find(|c| c.app == route.app);
    let by_query = route.has_key() || channel.is_some_and(|c| c.key_in_name);
    let decided = if by_query || channel.is_none() {
        table
            .admit_via(Protocol::Srt, &route.app, &route.stream)
            .map(|admit| (admit, None))
            .map_err(|r| (if channel.is_none() { NOT_FOUND } else { UNAUTHORIZED }, r.why))
    } else {
        by_passphrase(table, route)
    };
    match decided {
        Err((code, why)) => refuse(code, why),
        Ok((admit, _)) if hub.is_live(&admit.app, &admit.stream) => refuse(
            CONFLICT,
            format!("{}/{} is already live. Give this encoder another stream name, or stop the other one first.", admit.app, admit.stream),
        ),
        Ok((admit, passphrase)) => Decision::Take { admit, passphrase },
    }
}

/// The key as the SRT passphrase: which key, and whether the channel may
/// take this caller at all. The passphrase itself is checked by libsrt.
pub(super) fn by_passphrase(table: &Table, route: &Route) -> Result<(Admit, Option<String>), (i32, String)> {
    let app = &route.app;
    let channel = table.channels.iter().find(|c| &c.app == app).ok_or_else(|| (NOT_FOUND, String::new()))?;
    if !channel.enabled {
        let why = format!("the channel '{app}' is switched off. Switch it on in the mixer's Channels page and publish again.");
        return Err((FORBIDDEN, why));
    }
    if !channel.takes(Protocol::Srt) {
        return Err((FORBIDDEN, Protocol::Srt.not_taken(app)));
    }
    let chosen = match &route.user {
        Some(user) => channel.keys.iter().find(|(id, _)| id == user),
        None => channel.keys.first(),
    };
    let Some((key, secret)) = chosen else {
        let why = match &route.user {
            Some(user) => format!("the channel '{app}' has no key called '{user}'. Put the id of one of its keys in u=, as the Channels page shows it."),
            None => format!("the channel '{app}' has no keys, so nobody can publish to it yet. Make one on the mixer's Channels page."),
        };
        return Err((UNAUTHORIZED, why));
    };
    if route.name().is_empty() {
        return Err((BAD_REQUEST, "the stream id has no stream name. Use <channel>/main.".into()));
    }
    let admit = Admit { channel: channel.id.clone(), app: app.clone(), stream: route.name().to_string(), key: key.clone() };
    Ok((admit, Some(secret.clone())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::srt::streamid::parse;
    use serde_json::json;

    fn table() -> Table {
        Table::from_params(&json!({"channels": [
            {"id": "church", "app": "church", "protocols": ["rtmp", "srt"],
             "keys": [{"id": "obs", "secret": "first-key-0123"}, {"id": "phone", "secret": "second-key-4567"}]},
            {"id": "hall", "app": "hall", "protocols": ["rtmp"], "keys": [{"id": "a", "secret": "hall-key-0123"}]},
        ]}))
    }

    fn decide_on(id: &str) -> Decision {
        decide(&table(), &Hub::new(), &parse(id).unwrap())
    }

    #[test]
    fn with_no_key_in_the_id_the_first_key_is_the_passphrase() {
        let Decision::Take { admit, passphrase } = decide_on("church/cam2") else { panic!("let in") };
        assert_eq!((admit.stream.as_str(), admit.key.as_str()), ("cam2", "obs"));
        assert_eq!(passphrase.as_deref(), Some("first-key-0123"));
    }

    #[test]
    fn u_names_which_key_is_the_passphrase() {
        let Decision::Take { admit, passphrase } = decide_on("#!::r=church/main,m=publish,u=phone") else { panic!() };
        assert_eq!(admit.key, "phone");
        assert_eq!(passphrase.as_deref(), Some("second-key-4567"));
        assert!(matches!(decide_on("#!::r=church/main,u=nobody"), Decision::Refuse { code: UNAUTHORIZED, .. }));
    }

    #[test]
    fn a_key_in_the_id_is_checked_here_and_needs_no_passphrase() {
        let Decision::Take { admit, passphrase } = decide_on("church/main?psk=second-key-4567") else { panic!() };
        assert_eq!((admit.key.as_str(), passphrase), ("phone", None));
        assert!(matches!(decide_on("church/main?psk=wrong"), Decision::Refuse { code: UNAUTHORIZED, .. }));
    }

    #[test]
    fn refusals_say_why_with_the_code_srt_callers_understand() {
        let Decision::Refuse { code, why, .. } = decide_on("hall/main") else { panic!() };
        assert_eq!(code, FORBIDDEN);
        assert!(why.contains("does not take SRT"), "{why}");
        assert!(matches!(decide_on("nowhere/main"), Decision::Refuse { code: NOT_FOUND, .. }));
        // A player asking for a stream that is not on air.
        assert!(matches!(decide_on("#!::r=church/main,m=request"), Decision::Refuse { code: NOT_FOUND, .. }));
    }

    #[test]
    fn a_name_already_live_is_a_conflict() {
        let hub = Hub::new();
        let _live = hub.publish("church", "main", "x", None).unwrap();
        let d = decide(&table(), &hub, &parse("church/main").unwrap());
        assert!(matches!(d, Decision::Refuse { code: CONFLICT, .. }), "{d:?}");
    }
}
