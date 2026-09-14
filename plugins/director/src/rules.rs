//! The decision, with no clock, no socket and no model in it.
//!
//! Everything the director does is here: a view of the mixer goes in, a
//! [`Decision`] comes out, and the reason is a sentence a person can read in a
//! log. That is what makes the behaviour testable, and it is also what makes
//! the model optional: when a model is configured it proposes a source and
//! [`check`] runs it past exactly these rules before anything reaches the
//! mixer, which is the arrangement `examples/ai-director.py` describes and the
//! one the record in 09 section 2 argues for. An agent that believes a take is
//! safe is wrong often enough that the deterministic layer has to be the one
//! holding the programme.
//!
//! The rules, in the order they are applied:
//!
//! 1. What is on air has failed, stalled, gone away, or lost its picture.
//!    Cut now, whatever the hold says. A black programme is the one thing
//!    worse than a shot held too long.
//! 2. The shot has not been held long enough. Hold.
//! 3. Something is moving and what is on air is not. Cut to it.
//! 4. The shot has been held for the slow look time. Move on to the next
//!    live source, so a service does not sit on one camera for an hour.
//! 5. Otherwise hold.

use crate::settings::Settings;

/// One source, as `agent.state` describes it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shot {
    pub id: String,
    /// `connecting`, `live`, `stalled` or `failed`. Only `live` may be taken.
    pub state: String,
    /// 0.0 to 1.0, how much the picture changed between its last two frames.
    /// `None` when no snapshot tracker is running, which is the ordinary case
    /// on a core nobody is watching, and the rules work without it.
    pub motion: Option<f64>,
    /// How long since the last frame, present only once the picture stopped.
    pub video_idle_ms: Option<u64>,
    pub no_video: bool,
    pub no_audio: bool,
}

impl Shot {
    pub fn is_live(&self) -> bool {
        self.state == "live"
    }
}

/// What the director can see this cycle.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct View {
    /// The source on air, or `None` for the slate.
    pub program: Option<String>,
    pub sources: Vec<Shot>,
    /// How long the current shot has been held, in seconds.
    pub held_secs: f64,
    /// Present while a safety rule in the core is holding the programme. The
    /// director does not fight it.
    pub held_by_core: Option<String>,
}

impl View {
    pub fn shot(&self, id: &str) -> Option<&Shot> {
        self.sources.iter().find(|s| s.id == id)
    }

    /// The source on air, when it is still a source.
    pub fn on_air(&self) -> Option<&Shot> {
        self.program.as_deref().and_then(|id| self.shot(id))
    }
}

/// What to do about it.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Leave the programme alone. The string is why, for the log.
    Hold(String),
    /// Take this source. `None` is the slate, which happens only when nothing
    /// is live.
    Take { source: Option<String>, why: String },
}

impl Decision {
    pub fn why(&self) -> &str {
        match self {
            Decision::Hold(why) => why,
            Decision::Take { why, .. } => why,
        }
    }
}

/// Every source this director is allowed to take, in a stable order.
pub fn candidates<'a>(view: &'a View, settings: &Settings) -> Vec<&'a Shot> {
    view.sources
        .iter()
        .filter(|shot| shot.is_live() && !shot.no_video)
        .filter(|shot| settings.may_take(&shot.id))
        .filter(|shot| !is_frozen(shot, settings))
        .collect()
}

/// A picture that stopped is not a shot, however live the source says it is.
fn is_frozen(shot: &Shot, settings: &Settings) -> bool {
    shot.video_idle_ms
        .map(|idle| idle >= settings.idle_ms)
        .unwrap_or(false)
}

/// The rule based decision.
pub fn decide(view: &View, settings: &Settings) -> Decision {
    if let Some(reason) = &view.held_by_core {
        return Decision::Hold(format!("the core is holding the programme: {reason}"));
    }

    let choices = candidates(view, settings);
    if choices.is_empty() {
        return match &view.program {
            Some(id) => Decision::Hold(format!(
                "nothing else is live, so '{id}' stays on air whatever it is doing"
            )),
            None => Decision::Hold("nothing is live, so the slate stays".into()),
        };
    }

    // 1. What is on air is not usable any more.
    let broken = match view.on_air() {
        None if view.program.is_some() => Some("has gone away".to_string()),
        None => Some("the slate is up".to_string()),
        Some(shot) if !shot.is_live() => Some(format!("is {}", shot.state)),
        Some(shot) if shot.no_video => Some("has no picture".to_string()),
        Some(shot) if is_frozen(shot, settings) => Some(format!(
            "has not sent a frame for {} ms",
            shot.video_idle_ms.unwrap_or_default()
        )),
        Some(_) => None,
    };
    if let Some(why) = broken {
        let pick = best(&choices, None);
        return Decision::Take {
            source: Some(pick.id.clone()),
            why: match &view.program {
                Some(current) => format!("'{current}' {why}, so '{}' goes on", pick.id),
                None => format!("{why} and '{}' is live", pick.id),
            },
        };
    }

    // 2. Hold the shot.
    if view.held_secs < settings.min_hold_secs {
        return Decision::Hold(format!(
            "'{}' has been on for {:.0}s of {:.0}s",
            view.program.as_deref().unwrap_or("the slate"),
            view.held_secs,
            settings.min_hold_secs
        ));
    }

    let current = view.on_air();
    let current_motion = current.and_then(|s| s.motion);
    let others: Vec<&&Shot> = choices
        .iter()
        .filter(|shot| Some(shot.id.as_str()) != view.program.as_deref())
        .collect();
    if others.is_empty() {
        return Decision::Hold(format!(
            "'{}' is the only shot there is",
            view.program.as_deref().unwrap_or("the slate")
        ));
    }

    // 3. Something is moving and what is on air is not.
    if let Some(here) = current_motion {
        let mover = others
            .iter()
            .filter_map(|shot| shot.motion.map(|m| (m, *shot)))
            .filter(|(m, _)| *m - here > settings.motion_delta)
            .max_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((motion, shot)) = mover {
            return Decision::Take {
                source: Some(shot.id.clone()),
                why: format!(
                    "'{}' is moving ({motion:.2}) and '{}' is not ({here:.2})",
                    shot.id,
                    view.program.as_deref().unwrap_or("the slate")
                ),
            };
        }
    }

    // 4. The slow look: move on rather than sit on one shot forever.
    if view.held_secs >= settings.slow_look_secs {
        let pick = next_after(&choices, view.program.as_deref());
        return Decision::Take {
            source: Some(pick.id.clone()),
            why: format!(
                "'{}' has been on for {:.0}s, so the programme moves to '{}'",
                view.program.as_deref().unwrap_or("the slate"),
                view.held_secs,
                pick.id
            ),
        };
    }

    Decision::Hold(format!(
        "'{}' is doing its job",
        view.program.as_deref().unwrap_or("the slate")
    ))
}

/// Check a source a model named against the same rules, so a model can never
/// put something on air that the rules would refuse.
///
/// Returns the decision to carry out, which may be a hold with the reason the
/// proposal was refused.
pub fn check(view: &View, settings: &Settings, proposed: Option<&str>, reason: &str) -> Decision {
    let Some(id) = proposed else {
        return Decision::Hold(format!("the model kept the shot: {reason}"));
    };
    if Some(id) == view.program.as_deref() {
        return Decision::Hold(format!("the model kept '{id}': {reason}"));
    }
    let Some(shot) = view.shot(id) else {
        let known: Vec<&str> = view.sources.iter().map(|s| s.id.as_str()).collect();
        return Decision::Hold(format!(
            "the model named '{id}', which is not a source. The sources are: {}",
            known.join(", ")
        ));
    };
    if !shot.is_live() {
        return Decision::Hold(format!("the model named '{id}', which is {}", shot.state));
    }
    if !settings.may_take(id) {
        return Decision::Hold(format!(
            "the model named '{id}', which is not in this director's source list"
        ));
    }
    if is_frozen(shot, settings) {
        return Decision::Hold(format!(
            "the model named '{id}', whose picture stopped {} ms ago",
            shot.video_idle_ms.unwrap_or_default()
        ));
    }
    // The hold is the director's, not the model's. A model that wants a cut
    // every two seconds gets one every `min_hold_secs`.
    if view.held_secs < settings.min_hold_secs {
        return Decision::Hold(format!(
            "the model wanted '{id}' but '{}' has been on for only {:.0}s of {:.0}s",
            view.program.as_deref().unwrap_or("the slate"),
            view.held_secs,
            settings.min_hold_secs
        ));
    }
    Decision::Take {
        source: Some(id.to_string()),
        why: reason.to_string(),
    }
}

/// The liveliest shot, or the first one when nothing reports motion.
fn best<'a>(choices: &[&'a Shot], avoid: Option<&str>) -> &'a Shot {
    let allowed: Vec<&&Shot> = choices.iter().filter(|s| Some(s.id.as_str()) != avoid).collect();
    let pool: &[&&Shot] = if allowed.is_empty() {
        // Nothing but the one we were avoiding.
        return choices[0];
    } else {
        &allowed
    };
    pool.iter()
        .max_by(|a, b| {
            a.motion
                .unwrap_or(0.0)
                .total_cmp(&b.motion.unwrap_or(0.0))
                // A tie goes to the earlier id, so the choice is repeatable.
                .then(b.id.cmp(&a.id))
        })
        .map(|shot| **shot)
        .unwrap_or(choices[0])
}

/// The next source after this one, wrapping. A rota rather than a random pick,
/// so an operator watching can predict what happens next.
fn next_after<'a>(choices: &[&'a Shot], current: Option<&str>) -> &'a Shot {
    let at = current.and_then(|id| choices.iter().position(|s| s.id == id));
    match at {
        Some(at) => choices[(at + 1) % choices.len()],
        None => choices[0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn settings() -> Settings {
        Settings::from_value(&json!({"min_hold_secs": 5, "slow_look_secs": 30}))
    }

    fn live(id: &str, motion: Option<f64>) -> Shot {
        Shot {
            id: id.into(),
            state: "live".into(),
            motion,
            ..Shot::default()
        }
    }

    fn view(program: Option<&str>, held: f64, sources: Vec<Shot>) -> View {
        View {
            program: program.map(str::to_string),
            sources,
            held_secs: held,
            held_by_core: None,
        }
    }

    #[test]
    fn with_nothing_live_the_slate_stays() {
        let view = view(None, 99.0, vec![]);
        assert!(matches!(decide(&view, &settings()), Decision::Hold(_)));
    }

    #[test]
    fn the_first_live_source_goes_on_air_from_the_slate() {
        let view = view(None, 99.0, vec![live("cam1", None), live("cam2", None)]);
        let Decision::Take { source, why } = decide(&view, &settings()) else {
            panic!("expected a take");
        };
        assert_eq!(source.as_deref(), Some("cam1"));
        assert!(why.contains("cam1"), "{why}");
    }

    #[test]
    fn a_shot_is_held_for_the_minimum() {
        let view = view(Some("cam1"), 1.0, vec![live("cam1", Some(0.0)), live("cam2", Some(0.9))]);
        let Decision::Hold(why) = decide(&view, &settings()) else {
            panic!("expected a hold");
        };
        assert!(why.contains("1s of 5s"), "{why}");
    }

    #[test]
    fn a_source_that_fails_is_cut_away_from_at_once_whatever_the_hold_says() {
        let mut cam1 = live("cam1", Some(0.5));
        cam1.state = "failed".into();
        let view = view(Some("cam1"), 0.5, vec![cam1, live("cam2", Some(0.1))]);
        let Decision::Take { source, why } = decide(&view, &settings()) else {
            panic!("expected a take");
        };
        assert_eq!(source.as_deref(), Some("cam2"));
        assert!(why.contains("failed"), "{why}");
    }

    #[test]
    fn a_frozen_picture_is_cut_away_from_even_while_the_source_says_live() {
        let mut cam1 = live("cam1", Some(0.0));
        cam1.video_idle_ms = Some(5_000);
        let view = view(Some("cam1"), 0.5, vec![cam1, live("cam2", None)]);
        let Decision::Take { source, why } = decide(&view, &settings()) else {
            panic!("expected a take");
        };
        assert_eq!(source.as_deref(), Some("cam2"));
        assert!(why.contains("frame"), "{why}");
    }

    #[test]
    fn a_frozen_source_is_never_taken_to() {
        let mut cam2 = live("cam2", Some(0.9));
        cam2.video_idle_ms = Some(9_000);
        let view = view(Some("cam1"), 60.0, vec![live("cam1", Some(0.0)), cam2]);
        assert!(
            matches!(decide(&view, &settings()), Decision::Hold(_)),
            "the only other shot is frozen, so the programme stays"
        );
    }

    #[test]
    fn a_source_that_has_gone_away_is_cut_away_from() {
        let view = view(Some("cam1"), 0.1, vec![live("cam2", None)]);
        let Decision::Take { source, .. } = decide(&view, &settings()) else {
            panic!("expected a take");
        };
        assert_eq!(source.as_deref(), Some("cam2"));
    }

    #[test]
    fn motion_moves_the_programme_once_the_hold_is_up() {
        let view = view(Some("cam1"), 6.0, vec![live("cam1", Some(0.02)), live("cam2", Some(0.40))]);
        let Decision::Take { source, why } = decide(&view, &settings()) else {
            panic!("expected a take");
        };
        assert_eq!(source.as_deref(), Some("cam2"));
        assert!(why.contains("moving"), "{why}");
    }

    #[test]
    fn a_small_difference_in_motion_is_not_a_reason_to_cut() {
        let view = view(Some("cam1"), 6.0, vec![live("cam1", Some(0.20)), live("cam2", Some(0.25))]);
        assert!(matches!(decide(&view, &settings()), Decision::Hold(_)));
    }

    #[test]
    fn the_slow_look_moves_on_when_nothing_else_has() {
        let view = view(Some("cam1"), 31.0, vec![live("cam1", None), live("cam2", None)]);
        let Decision::Take { source, why } = decide(&view, &settings()) else {
            panic!("expected a take");
        };
        assert_eq!(source.as_deref(), Some("cam2"));
        assert!(why.contains("31s"), "{why}");
    }

    #[test]
    fn the_slow_look_goes_round_the_rota_rather_than_back_and_forth() {
        let sources = vec![live("cam1", None), live("cam2", None), live("cam3", None)];
        let after_two = view(Some("cam2"), 31.0, sources.clone());
        let Decision::Take { source, .. } = decide(&after_two, &settings()) else {
            panic!("expected a take");
        };
        assert_eq!(source.as_deref(), Some("cam3"));

        let after_three = view(Some("cam3"), 31.0, sources);
        let Decision::Take { source, .. } = decide(&after_three, &settings()) else {
            panic!("expected a take");
        };
        assert_eq!(source.as_deref(), Some("cam1"), "the rota wraps");
    }

    #[test]
    fn one_live_source_is_never_cut_away_from() {
        let view = view(Some("cam1"), 600.0, vec![live("cam1", Some(0.0))]);
        let Decision::Hold(why) = decide(&view, &settings()) else {
            panic!("expected a hold");
        };
        assert!(why.contains("only shot"), "{why}");
    }

    #[test]
    fn a_source_outside_the_list_is_never_taken() {
        let settings = Settings::from_value(&json!({"sources": ["cam1"], "slow_look_secs": 5}));
        let view = view(Some("cam1"), 60.0, vec![live("cam1", None), live("cam2", Some(0.9))]);
        assert!(matches!(decide(&view, &settings), Decision::Hold(_)));
    }

    #[test]
    fn the_core_s_own_hold_wins_over_every_rule() {
        let mut view = view(None, 99.0, vec![live("cam1", None)]);
        view.held_by_core = Some("the ad break is on air".into());
        let Decision::Hold(why) = decide(&view, &settings()) else {
            panic!("expected a hold");
        };
        assert!(why.contains("ad break"), "{why}");
    }

    #[test]
    fn a_source_with_no_picture_is_not_a_candidate() {
        let mut cam2 = live("cam2", Some(0.9));
        cam2.no_video = true;
        let view = view(Some("cam1"), 60.0, vec![live("cam1", Some(0.0)), cam2]);
        assert!(matches!(decide(&view, &settings()), Decision::Hold(_)));
    }

    // --- the model's proposals, checked against the same rules -------------

    #[test]
    fn a_model_that_names_a_live_source_gets_its_take() {
        let view = view(Some("cam1"), 60.0, vec![live("cam1", None), live("cam2", None)]);
        let Decision::Take { source, why } = check(&view, &settings(), Some("cam2"), "the speaker moved")
        else {
            panic!("expected a take");
        };
        assert_eq!(source.as_deref(), Some("cam2"));
        assert_eq!(why, "the speaker moved");
    }

    #[test]
    fn a_model_that_invents_a_source_is_refused_with_the_ones_that_exist() {
        let view = view(Some("cam1"), 60.0, vec![live("cam1", None), live("cam2", None)]);
        let Decision::Hold(why) = check(&view, &settings(), Some("cam9"), "because") else {
            panic!("expected a hold");
        };
        assert!(why.contains("cam1, cam2"), "{why}");
    }

    #[test]
    fn a_model_cannot_take_a_source_that_is_not_live() {
        let mut cam2 = live("cam2", None);
        cam2.state = "connecting".into();
        let view = view(Some("cam1"), 60.0, vec![live("cam1", None), cam2]);
        let Decision::Hold(why) = check(&view, &settings(), Some("cam2"), "because") else {
            panic!("expected a hold");
        };
        assert!(why.contains("connecting"), "{why}");
    }

    #[test]
    fn a_model_cannot_cut_faster_than_the_hold() {
        let view = view(Some("cam1"), 1.0, vec![live("cam1", None), live("cam2", None)]);
        let Decision::Hold(why) = check(&view, &settings(), Some("cam2"), "because") else {
            panic!("expected a hold");
        };
        assert!(why.contains("only 1s of 5s"), "{why}");
    }

    #[test]
    fn a_model_that_keeps_the_shot_keeps_it() {
        let view = view(Some("cam1"), 60.0, vec![live("cam1", None), live("cam2", None)]);
        assert!(matches!(check(&view, &settings(), None, "nothing changed"), Decision::Hold(_)));
        assert!(matches!(
            check(&view, &settings(), Some("cam1"), "still right"),
            Decision::Hold(_)
        ));
    }

    #[test]
    fn a_model_cannot_take_a_source_outside_the_list() {
        let settings = Settings::from_value(&json!({"sources": ["cam1"]}));
        let view = view(Some("cam1"), 60.0, vec![live("cam1", None), live("cam2", None)]);
        let Decision::Hold(why) = check(&view, &settings, Some("cam2"), "because") else {
            panic!("expected a hold");
        };
        assert!(why.contains("source list"), "{why}");
    }
}
