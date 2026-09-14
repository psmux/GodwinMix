//! An OSC address becomes one call on the core's `/rpc`, or nothing.
//!
//! This is the whole of the plugin's behaviour and none of its plumbing, so it
//! is the part with the tests. Nothing here opens a socket or knows a core
//! exists: an address and its arguments go in, an [`Action`] comes out, and a
//! message that means nothing answers with a sentence naming the forms that
//! would have worked.
//!
//! The addresses, in full:
//!
//! ```text
//!  /program/take         "cam1"     take a source; no argument cuts to the slate
//!  /program/take/cam1    [1]        the same, for a surface that cannot send a string
//!  /program/slate                   cut to the slate
//!  /program/revert                  back to the shot before
//!  /scene/take           "wide"     take a scene by name
//!  /scene/take/wide      [1]
//!  /source/cam1/take     [1]
//!  /source/cam1/audio/gain    -6.0  decibels, 0 is unity
//!  /source/cam1/audio/fader    0.7  linear 0 to 10, which is what a fader sends
//!  /source/cam1/audio/mute       1
//!  /output/youtube/reconnect  [1]
//!  /output/youtube/remove     [1]
//! ```
//!
//! Every address whose last piece is an action takes an optional button
//! argument. A momentary button sends 1 on the way down and 0 on the way up,
//! and acting on both would take twice, so a falsey argument is ignored.

use crate::osc::Message;

/// One thing to do to the mixer.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// `program.take`. `None` cuts to the slate.
    Take(Option<String>),
    /// `program.take` with `scene`.
    TakeScene(String),
    /// `program.revert`.
    Revert,
    /// `source.audio.set` with a linear gain, 0.0 to 10.0.
    Gain { id: String, gain: f64 },
    /// `source.audio.set` with `muted`.
    Mute { id: String, muted: bool },
    /// `output.reconnect`.
    OutputReconnect(String),
    /// `output.remove`.
    OutputRemove(String),
}

/// Why a message did nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// A button on the way up. Not a mistake, and not worth logging loudly.
    Released,
    /// The address is not one this plugin answers, or its argument is wrong.
    /// The message names the next step, as every other refusal in GodwinMix
    /// does.
    Unknown(String),
}

/// Decibels to the linear gain `source.audio.set` takes.
///
/// The core's fader runs 0.0 silent to 10.0, with 1.0 unity, so 0 dB is 1.0
/// and -6 dB is about 0.5. Anything at or under -90 dB is silence rather than
/// a very small number, because that is what an operator pulling a fader to
/// the bottom means.
pub fn db_to_linear(db: f64) -> f64 {
    if db <= -90.0 || db.is_nan() {
        return 0.0;
    }
    (10f64.powf(db / 20.0)).clamp(0.0, 10.0)
}

/// Read one message. `Ok(None)` never happens: a message either means
/// something or is refused with a reason.
pub fn action_for(message: &Message) -> Result<Action, Refusal> {
    let parts = message.parts();
    let args = &message.args;

    // A trailing argument that reads as 0 is a button coming back up.
    let pressed = match args.first() {
        None => true,
        Some(arg) => arg.is_truthy() || arg.as_str().is_some() || arg.as_f64().is_none(),
    };

    match parts.as_slice() {
        ["program", "take"] => {
            if !pressed {
                return Err(Refusal::Released);
            }
            match args.first().and_then(|a| a.as_str()) {
                Some(id) if !id.is_empty() => Ok(Action::Take(Some(id.to_string()))),
                Some(_) | None => Ok(Action::Take(None)),
            }
        }
        ["program", "take", id] => button(pressed, Action::Take(Some((*id).to_string()))),
        ["program", "slate"] => button(pressed, Action::Take(None)),
        ["program", "revert"] => button(pressed, Action::Revert),
        ["scene", "take"] => {
            if !pressed {
                return Err(Refusal::Released);
            }
            match args.first().and_then(|a| a.as_str()) {
                Some(name) if !name.is_empty() => Ok(Action::TakeScene(name.to_string())),
                _ => Err(Refusal::Unknown(
                    "/scene/take wants the scene name as a string argument, or write it into \
                     the address as /scene/take/<name>."
                        .into(),
                )),
            }
        }
        ["scene", "take", name] => button(pressed, Action::TakeScene((*name).to_string())),
        ["source", id, "take"] => button(pressed, Action::Take(Some((*id).to_string()))),
        ["source", id, "audio", "gain"] => {
            let db = number(args, "/source/<id>/audio/gain", "decibels, 0 is unity")?;
            Ok(Action::Gain {
                id: (*id).to_string(),
                gain: db_to_linear(db),
            })
        }
        ["source", id, "audio", "fader"] => {
            let level = number(args, "/source/<id>/audio/fader", "0.0 to 10.0, 1.0 is unity")?;
            Ok(Action::Gain {
                id: (*id).to_string(),
                gain: level.clamp(0.0, 10.0),
            })
        }
        ["source", id, "audio", "mute"] => {
            let on = number(args, "/source/<id>/audio/mute", "1 to mute, 0 to unmute")?;
            Ok(Action::Mute {
                id: (*id).to_string(),
                muted: on != 0.0,
            })
        }
        ["output", id, "reconnect"] => {
            button(pressed, Action::OutputReconnect((*id).to_string()))
        }
        ["output", id, "remove"] => button(pressed, Action::OutputRemove((*id).to_string())),
        _ => Err(Refusal::Unknown(format!(
            "'{}' is not an address this plugin answers. It answers /program/take, \
             /program/slate, /program/revert, /scene/take, /source/<id>/take, \
             /source/<id>/audio/{{gain,fader,mute}}, /output/<id>/reconnect and \
             /output/<id>/remove.",
            message.address
        ))),
    }
}

fn button(pressed: bool, action: Action) -> Result<Action, Refusal> {
    if pressed {
        Ok(action)
    } else {
        Err(Refusal::Released)
    }
}

fn number(args: &[crate::osc::Arg], address: &str, wants: &str) -> Result<f64, Refusal> {
    match args.first().and_then(|a| a.as_f64()) {
        Some(n) => Ok(n),
        None => Err(Refusal::Unknown(format!(
            "{address} wants one number ({wants}). It was sent {} argument(s) and none of them \
             read as a number.",
            args.len()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osc::Arg;

    fn read(address: &str, args: Vec<Arg>) -> Result<Action, Refusal> {
        action_for(&Message::new(address, args))
    }

    #[test]
    fn the_address_the_documentation_leads_with_takes_a_source() {
        assert_eq!(
            read("/program/take", vec![Arg::Str("cam1".into())]),
            Ok(Action::Take(Some("cam1".into())))
        );
    }

    #[test]
    fn a_surface_that_cannot_send_strings_puts_the_id_in_the_address() {
        assert_eq!(
            read("/program/take/cam1", vec![Arg::Float(1.0)]),
            Ok(Action::Take(Some("cam1".into())))
        );
        assert_eq!(
            read("/source/cam2/take", vec![]),
            Ok(Action::Take(Some("cam2".into())))
        );
    }

    #[test]
    fn a_take_with_no_source_cuts_to_the_slate() {
        assert_eq!(read("/program/take", vec![]), Ok(Action::Take(None)));
        assert_eq!(read("/program/slate", vec![]), Ok(Action::Take(None)));
        assert_eq!(
            read("/program/take", vec![Arg::Str(String::new())]),
            Ok(Action::Take(None))
        );
    }

    #[test]
    fn a_button_coming_back_up_does_not_take_a_second_time() {
        assert_eq!(read("/program/take/cam1", vec![Arg::Int(0)]), Err(Refusal::Released));
        assert_eq!(read("/program/revert", vec![Arg::Bool(false)]), Err(Refusal::Released));
        assert_eq!(read("/source/cam1/take", vec![Arg::Float(0.0)]), Err(Refusal::Released));
    }

    #[test]
    fn a_gain_in_decibels_becomes_the_core_s_linear_fader() {
        let Ok(Action::Gain { id, gain }) = read("/source/cam1/audio/gain", vec![Arg::Float(-6.0)])
        else {
            panic!("expected a gain");
        };
        assert_eq!(id, "cam1");
        assert!((gain - 0.501).abs() < 0.001, "{gain}");

        let Ok(Action::Gain { gain, .. }) = read("/source/cam1/audio/gain", vec![Arg::Int(0)])
        else {
            panic!("expected a gain");
        };
        assert!((gain - 1.0).abs() < 1e-9, "0 dB is unity, got {gain}");
    }

    #[test]
    fn the_bottom_of_a_fader_is_silence_and_not_a_very_small_number() {
        assert_eq!(db_to_linear(-90.0), 0.0);
        assert_eq!(db_to_linear(-120.0), 0.0);
        assert_eq!(db_to_linear(f64::NEG_INFINITY), 0.0);
        assert_eq!(db_to_linear(f64::NAN), 0.0);
    }

    #[test]
    fn a_very_loud_request_is_clamped_to_what_the_core_accepts() {
        assert_eq!(db_to_linear(60.0), 10.0);
        let Ok(Action::Gain { gain, .. }) = read("/source/cam1/audio/fader", vec![Arg::Float(99.0)])
        else {
            panic!("expected a gain");
        };
        assert_eq!(gain, 10.0);
    }

    #[test]
    fn a_linear_fader_passes_straight_through() {
        let Ok(Action::Gain { gain, .. }) = read("/source/cam1/audio/fader", vec![Arg::Float(0.7)])
        else {
            panic!("expected a gain");
        };
        assert!((gain - 0.7).abs() < 1e-6);
    }

    #[test]
    fn mute_reads_whatever_a_surface_calls_true() {
        for arg in [Arg::Int(1), Arg::Float(1.0), Arg::Bool(true)] {
            assert_eq!(
                read("/source/cam1/audio/mute", vec![arg.clone()]),
                Ok(Action::Mute { id: "cam1".into(), muted: true }),
                "{arg:?}"
            );
        }
        assert_eq!(
            read("/source/cam1/audio/mute", vec![Arg::Int(0)]),
            Ok(Action::Mute { id: "cam1".into(), muted: false })
        );
    }

    #[test]
    fn a_scene_take_reads_the_name_from_either_place() {
        assert_eq!(
            read("/scene/take", vec![Arg::Str("wide".into())]),
            Ok(Action::TakeScene("wide".into()))
        );
        assert_eq!(read("/scene/take/wide", vec![]), Ok(Action::TakeScene("wide".into())));
    }

    #[test]
    fn a_scene_take_with_no_name_says_both_forms() {
        let Err(Refusal::Unknown(message)) = read("/scene/take", vec![]) else {
            panic!("expected a refusal");
        };
        assert!(message.contains("/scene/take/<name>"), "{message}");
    }

    #[test]
    fn outputs_reconnect_and_remove() {
        assert_eq!(
            read("/output/youtube/reconnect", vec![]),
            Ok(Action::OutputReconnect("youtube".into()))
        );
        assert_eq!(
            read("/output/youtube/remove", vec![Arg::Int(1)]),
            Ok(Action::OutputRemove("youtube".into()))
        );
    }

    #[test]
    fn an_unknown_address_lists_the_ones_that_work() {
        let Err(Refusal::Unknown(message)) = read("/mixer/go", vec![]) else {
            panic!("expected a refusal");
        };
        assert!(message.contains("/program/take"), "{message}");
        assert!(message.contains("/source/<id>/audio"), "{message}");
    }

    #[test]
    fn a_gain_with_no_number_says_what_it_wanted() {
        let Err(Refusal::Unknown(message)) = read("/source/cam1/audio/gain", vec![]) else {
            panic!("expected a refusal");
        };
        assert!(message.contains("decibels"), "{message}");
    }
}
