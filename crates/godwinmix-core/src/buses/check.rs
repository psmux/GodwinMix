//! What the audio desk refuses, and the words it refuses with.
//!
//! Every refusal says what is wrong and what to do instead, and carries enough
//! for the control plane to build an error `data` object a caller can act on.

use godwinmix_protocol::buses::{AudioConfig, BusConfig, MAX_LEVEL};
use std::fmt;

/// Why an audio call did nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// No bus or source with this id. `known` lists the ones there are.
    NotFound { kind: &'static str, id: String, known: Vec<String> },
    /// A value out of range, or a bus that contradicts itself. `field` names
    /// the key at fault.
    Invalid { field: &'static str, message: String },
    /// Something else depends on it: an output reading a bus that is being
    /// removed. `by` names what.
    InUse { message: String, by: Vec<String> },
    /// The pipeline would not take the change. Nothing was changed.
    Failed(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::NotFound { kind, id, known } => {
                let have = if known.is_empty() { "none".to_string() } else { known.join(", ") };
                write!(f, "no {kind} '{id}'. The {kind}s here are: {have}.")?;
                if *kind == "bus" {
                    write!(f, " Add one with audio.bus.add.")?;
                }
                Ok(())
            }
            Refusal::Invalid { message, .. } => f.write_str(message),
            Refusal::InUse { message, .. } => f.write_str(message),
            Refusal::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Refusal {}

/// The ids a bus may not take, because `/pcm/` and `/opus/` already mean
/// something by them.
const RESERVED: &[&str] = &["program", "programme"];

/// A bus id: lower case letters, digits and single dashes, at most 48 long.
pub fn bus_id(id: &str) -> Result<(), Refusal> {
    let ok = !id.is_empty()
        && id.len() <= 48
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
        && !id.ends_with('-')
        && !id.contains("--");
    if !ok {
        return Err(Refusal::Invalid {
            field: "id",
            message: format!(
                "'{id}' is not a bus id. Use lower case letters, digits and dashes, such as \
                 monitor or guest-return."
            ),
        });
    }
    if RESERVED.contains(&id) {
        return Err(Refusal::Invalid {
            field: "id",
            message: format!("'{id}' already means the programme mix. Pick another id, such as monitor."),
        });
    }
    Ok(())
}

/// A fader position: a number from 0.0 to 10.0.
pub fn level(field: &'static str, v: f64) -> Result<f64, Refusal> {
    if v.is_finite() && (0.0..=MAX_LEVEL).contains(&v) {
        return Ok(v);
    }
    Err(Refusal::Invalid {
        field,
        message: format!(
            "{field} {v} is out of range. Give a number from 0.0 (silent) through 1.0 (unity) \
             to {MAX_LEVEL}."
        ),
    })
}

/// A whole bus, before it is built or saved.
pub fn bus(cfg: &BusConfig) -> Result<(), Refusal> {
    bus_id(&cfg.id)?;
    level("gain", cfg.gain)?;
    for v in cfg.sends.values() {
        level("level", *v)?;
    }
    if cfg.cue && cfg.minus.is_some() {
        return Err(Refusal::Invalid {
            field: "minus",
            message: format!(
                "bus {} cannot be both the cue bus and a mix minus. Drop `minus` for headphones, \
                 or `cue` for a guest's return.",
                cfg.id
            ),
        });
    }
    if cfg.minus.as_deref() == Some("") {
        return Err(Refusal::Invalid {
            field: "minus",
            message: "minus is the id of the source to leave out, such as guest-1.".into(),
        });
    }
    Ok(())
}

/// The `[audio]` table at load: every bus well formed, no id twice, and every
/// output that names a bus naming one that is there.
pub fn config(audio: &AudioConfig, outputs: &[crate::config::OutputConfig]) -> anyhow::Result<()> {
    level("audio.master.gain", audio.master.gain).map_err(anyhow::Error::new)?;
    let mut seen = std::collections::HashSet::new();
    for b in &audio.buses {
        bus(b).map_err(|e| anyhow::anyhow!("[[audio.bus]] {}: {e}", b.id))?;
        anyhow::ensure!(seen.insert(&b.id), "two [[audio.bus]] tables are called {}; give each its own id", b.id);
    }
    for o in outputs {
        if let Some(name) = &o.audio {
            anyhow::ensure!(
                seen.contains(name),
                "output {} takes its sound from bus {name}, and there is no [[audio.bus]] with that id. \
                 Add the bus, or leave `audio` out for the programme mix",
                o.id
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_id_says_what_an_id_looks_like() {
        for id in ["", "Monitor", "a b", "-x", "x-", "a--b", "program"] {
            let e = bus_id(id).unwrap_err();
            assert!(matches!(e, Refusal::Invalid { field: "id", .. }), "{id:?} was taken");
        }
        bus_id("guest-return").unwrap();
    }

    #[test]
    fn a_level_out_of_range_names_the_range() {
        let e = level("level", 11.0).unwrap_err().to_string();
        assert!(e.contains("0.0") && e.contains("10"), "{e}");
        assert!(level("gain", f64::NAN).is_err());
        assert_eq!(level("gain", 0.5).unwrap(), 0.5);
    }

    #[test]
    fn a_bus_that_is_cue_and_minus_is_refused() {
        let mut cfg = BusConfig::named("hp");
        cfg.cue = true;
        cfg.minus = Some("guest-1".into());
        assert!(bus(&cfg).unwrap_err().to_string().contains("Drop"));
    }
}
