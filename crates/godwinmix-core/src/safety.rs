//! The rules that stand in front of a take, for every caller.
//!
//! 03 section 6, "Safety in the core, not in a plugin". A minimum hold, a rate
//! limit, a flash guard and a watchdog on the operator who made the last cut.
//! They live here rather than in a plugin or in an agent's prompt because a
//! prompt is a suggestion and a plugin can be uninstalled.
//!
//! Nothing here touches GStreamer and nothing here blocks. A check is a lock,
//! a few comparisons on a small deque, and an answer. The take handler in the
//! binary calls [`Guard::check`] before the command reaches the pipeline and
//! [`Guard::record`] after the mixer has accepted it.
//!
//! ```toml
//! [safety]
//! min_hold_ms = 8000
//! max_takes_per_minute = 12
//! flash_guard = true
//! on_operator_silence = { after_secs = 120, action = "alert" }
//! ```

use godwinmix_protocol::scope::{Token, TokenSafety};
use parking_lot::Mutex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// ITU-R BT.1702-3: a cut that changes luminance sharply over a quarter of the
/// picture is held to 360 ms from the one before it, 334 ms above 50 Hz, and
/// no more than three in any one second. Ofcom rule 2.12 gives this the force
/// of a rule in the UK, which is why it is the one hold time here that is not
/// a matter of taste.
pub const FLASH_SEPARATION_MS: u64 = 360;
pub const FLASH_SEPARATION_MS_60HZ: u64 = 334;
pub const FLASHES_PER_SECOND: usize = 3;
/// The luminance step that counts, in candela per square metre.
pub const FLASH_LUMINANCE_CD_M2: f64 = 20.0;
/// How much of the picture has to take that step.
pub const FLASH_AREA_FRACTION: f64 = 0.25;

/// `[safety]`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct SafetyConfig {
    /// A take inside this window of the last one is refused, and the refusal
    /// says how long is left. Not a standard: no standards body publishes a
    /// minimum shot length, so this is a default an operator may move.
    pub min_hold_ms: u64,
    /// Takes allowed in any rolling minute, counted per core rather than per
    /// token, because the programme only has one picture.
    pub max_takes_per_minute: u32,
    /// The BT.1702-3 hold. On by default.
    pub flash_guard: bool,
    pub on_operator_silence: OperatorSilence,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            min_hold_ms: 8_000,
            max_takes_per_minute: 12,
            flash_guard: true,
            on_operator_silence: OperatorSilence::default(),
        }
    }
}

/// What happens when whoever made the last take stops calling.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct OperatorSilence {
    pub after_secs: u64,
    pub action: SilenceAction,
}

impl Default for OperatorSilence {
    fn default() -> Self {
        // Alert, because a programme that keeps running is the safe state.
        Self { after_secs: 120, action: SilenceAction::Alert }
    }
}

/// `alert`, `hold`, `slate`, or `fallback:<source>`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SilenceAction {
    /// Raise a `critical` alert and change nothing.
    #[default]
    Alert,
    /// Raise the alert and refuse further takes until somebody calls again.
    Hold,
    /// Raise the alert and cut to the slate.
    Slate,
    /// Raise the alert and cut to this source.
    Fallback(String),
}

impl SilenceAction {
    pub fn as_str(&self) -> String {
        match self {
            Self::Alert => "alert".into(),
            Self::Hold => "hold".into(),
            Self::Slate => "slate".into(),
            Self::Fallback(id) => format!("fallback:{id}"),
        }
    }

    /// Read the config spelling. An unknown word is an error rather than a
    /// silent `alert`, because a misspelt `slat` that quietly did nothing is
    /// the kind of thing found during the show it was meant to save.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim() {
            "alert" => Ok(Self::Alert),
            "hold" => Ok(Self::Hold),
            "slate" => Ok(Self::Slate),
            rest => match rest.strip_prefix("fallback:") {
                Some(id) if !id.trim().is_empty() => Ok(Self::Fallback(id.trim().to_string())),
                _ => Err(format!(
                    "'{text}' is not an operator silence action. Write \"alert\", \"hold\", \
                     \"slate\", or \"fallback:<source id>\"."
                )),
            },
        }
    }
}

impl<'de> Deserialize<'de> for SilenceAction {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

impl Serialize for SilenceAction {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_str())
    }
}

/// The three numbers in force for one caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub min_hold_ms: u64,
    pub max_takes_per_minute: u32,
    pub flash_guard: bool,
}

impl SafetyConfig {
    /// What this token is held to.
    ///
    /// A human token's `safety = { .. }` moves any of the three, in either
    /// direction: a vision mixer cutting a concert is not going to wait eight
    /// seconds and knows it. An agent's token may only tighten, so an agent
    /// that has been told to loosen its own limits finds that it cannot.
    /// The flash guard is never turned off by an agent token.
    pub fn for_token(&self, token: &Token) -> Limits {
        let mine = Limits {
            min_hold_ms: self.min_hold_ms,
            max_takes_per_minute: self.max_takes_per_minute,
            flash_guard: self.flash_guard,
        };
        let Some(over) = token.safety.as_ref() else { return mine };
        if token.agent {
            return tighten(mine, over);
        }
        Limits {
            min_hold_ms: over.min_hold_ms.unwrap_or(mine.min_hold_ms),
            max_takes_per_minute: over
                .max_takes_per_minute
                .unwrap_or(mine.max_takes_per_minute),
            flash_guard: over.flash_guard.unwrap_or(mine.flash_guard),
        }
    }
}

/// An agent's override, taking only the half of it that makes the rule harder.
fn tighten(mine: Limits, over: &TokenSafety) -> Limits {
    Limits {
        min_hold_ms: over.min_hold_ms.map_or(mine.min_hold_ms, |v| v.max(mine.min_hold_ms)),
        max_takes_per_minute: over
            .max_takes_per_minute
            .map_or(mine.max_takes_per_minute, |v| v.min(mine.max_takes_per_minute)),
        flash_guard: mine.flash_guard || over.flash_guard.unwrap_or(false),
    }
}

/// Why a take was refused, with the number the caller needs to try again.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    /// `min_hold`, `rate_limit`, `flash_guard` or `operator_silence`. Carried
    /// in `data.rule` so a client can branch without reading English.
    pub rule: &'static str,
    pub retry_after_ms: u64,
    pub message: String,
}

/// The rules, and the little history they need.
pub struct Guard {
    cfg: SafetyConfig,
    /// The frame rate the programme runs at, which decides whether the flash
    /// separation is 360 ms or 334 ms.
    fps: u32,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// When each of the recent takes landed. Bounded by the rate limit.
    takes: VecDeque<Instant>,
    last_take: Option<Instant>,
    /// When the programme picture last took a flash sized step.
    flashes: VecDeque<Instant>,
    /// Who made the last take, and when that token last called anything.
    operator: Option<String>,
    last_call: Option<Instant>,
    /// True once the watchdog has fired and until somebody calls again.
    silent: bool,
    /// True while telemetry is measuring the programme's luminance. Without
    /// it the guard cannot tell a flash from a dissolve, so it treats every
    /// cut as one.
    luma_observed: bool,
}

impl Guard {
    pub fn new(cfg: SafetyConfig, fps: u32) -> Arc<Self> {
        Arc::new(Self { cfg, fps, state: Mutex::new(State::default()) })
    }

    pub fn config(&self) -> &SafetyConfig {
        &self.cfg
    }

    /// The flash separation at this core's frame rate.
    pub fn flash_separation_ms(&self) -> u64 {
        if self.fps >= 50 { FLASH_SEPARATION_MS_60HZ } else { FLASH_SEPARATION_MS }
    }

    /// May this token take now? The whole policy, in one call.
    pub fn check(&self, token: &Token) -> Result<(), Refusal> {
        self.check_at(token, Instant::now())
    }

    /// The same with the clock supplied, which is how the tests are written
    /// without sleeping through an eight second hold.
    pub fn check_at(&self, token: &Token, now: Instant) -> Result<(), Refusal> {
        let limits = self.cfg.for_token(token);
        let mut state = self.state.lock();
        state.forget_before(now);
        if state.silent {
            return Err(Refusal {
                rule: "operator_silence",
                retry_after_ms: 0,
                message: format!(
                    "the programme is held: '{}' made the last take and then made no call \
                     for {} seconds, so on_operator_silence took over. Any call from a \
                     token releases the hold; call program.get and then take again.",
                    state.operator.as_deref().unwrap_or("nobody"),
                    self.cfg.on_operator_silence.after_secs
                ),
            });
        }
        if let Some(last) = state.last_take {
            let held = now.saturating_duration_since(last).as_millis() as u64;
            if held < limits.min_hold_ms {
                let left = limits.min_hold_ms - held;
                return Err(Refusal {
                    rule: "min_hold",
                    retry_after_ms: left,
                    message: format!(
                        "the shot on air has been up for {held} ms and this core holds a shot \
                         for {} ms, so there are {left} ms left. Wait {left} ms and take \
                         again, or use a token whose safety.min_hold_ms is lower.",
                        limits.min_hold_ms
                    ),
                });
            }
        }
        if state.takes.len() >= limits.max_takes_per_minute as usize {
            let oldest = state.takes.front().copied().unwrap_or(now);
            let left = 60_000u64
                .saturating_sub(now.saturating_duration_since(oldest).as_millis() as u64)
                .max(1);
            return Err(Refusal {
                rule: "rate_limit",
                retry_after_ms: left,
                message: format!(
                    "{} takes have been made in the last minute and this core allows {}, \
                     so there are {left} ms left. Wait {left} ms and take again.",
                    state.takes.len(),
                    limits.max_takes_per_minute
                ),
            });
        }
        if limits.flash_guard {
            if let Some(refusal) = self.flash_refusal(&state, now) {
                return Err(refusal);
            }
        }
        Ok(())
    }

    /// BT.1702-3, both halves: a separation from the last flash, and no more
    /// than three of them in a second.
    fn flash_refusal(&self, state: &State, now: Instant) -> Option<Refusal> {
        let separation = self.flash_separation_ms();
        let last = state.flashes.back()?;
        let since = now.saturating_duration_since(*last).as_millis() as u64;
        if since < separation {
            let left = separation - since;
            return Some(Refusal {
                rule: "flash_guard",
                retry_after_ms: left,
                message: format!(
                    "the last cut changed the picture's brightness sharply, and ITU-R \
                     BT.1702-3 holds such a cut {separation} ms from the next one. There are \
                     {left} ms left. Wait {left} ms and take again."
                ),
            });
        }
        let in_last_second = state
            .flashes
            .iter()
            .filter(|f| now.saturating_duration_since(**f) < Duration::from_secs(1))
            .count();
        if in_last_second >= FLASHES_PER_SECOND {
            let oldest = state
                .flashes
                .iter()
                .find(|f| now.saturating_duration_since(**f) < Duration::from_secs(1))?;
            let left = 1_000u64
                .saturating_sub(now.saturating_duration_since(*oldest).as_millis() as u64)
                .max(1);
            return Some(Refusal {
                rule: "flash_guard",
                retry_after_ms: left,
                message: format!(
                    "there have already been {in_last_second} sharp changes of brightness in \
                     the last second, which is the most ITU-R BT.1702-3 allows. There are \
                     {left} ms left. Wait {left} ms and take again."
                ),
            });
        }
        None
    }

    /// A take the mixer accepted. Called after the cut is queued, so a refusal
    /// downstream does not start the hold.
    pub fn record(&self, token_id: &str) {
        self.record_at(token_id, Instant::now());
    }

    pub fn record_at(&self, token_id: &str, now: Instant) {
        let mut state = self.state.lock();
        state.forget_before(now);
        state.takes.push_back(now);
        state.last_take = Some(now);
        state.operator = Some(token_id.to_string());
        state.last_call = Some(now);
        state.silent = false;
        // Without telemetry running the core cannot tell a flash from any
        // other cut, so it assumes the stricter thing. With the default
        // eight second hold this never bites; it bites when somebody lowers
        // `min_hold_ms` below the flash separation and has no probes on.
        if self.cfg.flash_guard && !state.luma_observed {
            state.flashes.push_back(now);
            state.trim_flashes(now);
        }
    }

    /// Any call at all from any token, which is what the operator watchdog
    /// watches. Cheap: one lock and two stores.
    pub fn note_call(&self, token_id: &str) {
        let mut state = self.state.lock();
        if state.operator.as_deref() == Some(token_id) {
            state.last_call = Some(Instant::now());
        }
        state.silent = false;
    }

    /// Telemetry saw the programme's luminance take a BT.1702 sized step.
    pub fn note_flash(&self) {
        let now = Instant::now();
        let mut state = self.state.lock();
        state.flashes.push_back(now);
        state.trim_flashes(now);
    }

    /// Telemetry says whether it is measuring. While it is not, every cut is
    /// treated as a possible flash.
    pub fn set_luma_observed(&self, observed: bool) {
        self.state.lock().luma_observed = observed;
    }

    /// Whether the operator who made the last take has gone quiet, and who it
    /// was. `None` while somebody is still calling, or when nobody has taken.
    pub fn silent_operator(&self) -> Option<String> {
        let state = self.state.lock();
        if state.silent {
            return None;
        }
        let after = Duration::from_secs(self.cfg.on_operator_silence.after_secs);
        let last = state.last_call?;
        let who = state.operator.clone()?;
        (last.elapsed() >= after).then_some(who)
    }

    /// Mark the watchdog as having fired, so it fires once rather than every
    /// second until somebody comes back.
    pub fn arm_silence(&self, hold: bool) {
        let mut state = self.state.lock();
        state.last_call = Some(Instant::now());
        state.silent = hold;
    }

    /// For `core.info` and the tests: the numbers a token is held to.
    pub fn limits_for(&self, token: &Token) -> Limits {
        self.cfg.for_token(token)
    }
}

impl State {
    fn forget_before(&mut self, now: Instant) {
        while self
            .takes
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= Duration::from_secs(60))
        {
            self.takes.pop_front();
        }
        self.trim_flashes(now);
    }

    /// Only the last second and a bit matters, and the deque is capped anyway
    /// so a long show cannot grow it.
    fn trim_flashes(&mut self, now: Instant) {
        while self
            .flashes
            .front()
            .is_some_and(|f| now.saturating_duration_since(*f) >= Duration::from_secs(2))
        {
            self.flashes.pop_front();
        }
        while self.flashes.len() > 16 {
            self.flashes.pop_front();
        }
    }
}

/// Convert an 8 bit luma sample to display luminance in candela per square
/// metre, which is the unit BT.1702-3 states its threshold in.
///
/// BT.1886 with a 100 cd/m2 reference white and a 2.4 gamma, on studio swing
/// luma (16 is black, 235 is white). Nobody knows the viewer's actual panel,
/// and the standard is written for a reference one, so this is the reference.
pub fn luminance_cd_m2(luma8: u8) -> f64 {
    let normalised = ((luma8 as f64) - 16.0) / 219.0;
    let clamped = normalised.clamp(0.0, 1.0);
    100.0 * clamped.powf(2.4)
}

/// Whether a step between two subsampled luma grids is a flash under
/// BT.1702-3: at least 20 cd/m2 over more than a quarter of the frame.
///
/// The grids come from the telemetry probe and are the same size, one sample
/// per cell. A mismatch in length answers false rather than guessing.
pub fn is_flash(before: &[u8], after: &[u8]) -> bool {
    if before.len() != after.len() || before.is_empty() {
        return false;
    }
    let stepped = before
        .iter()
        .zip(after)
        .filter(|(a, b)| (luminance_cd_m2(**a) - luminance_cd_m2(**b)).abs() >= FLASH_LUMINANCE_CD_M2)
        .count();
    (stepped as f64 / before.len() as f64) > FLASH_AREA_FRACTION
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_protocol::scope::Token;

    fn human() -> Token {
        Token::open()
    }

    fn agent(over: TokenSafety) -> Token {
        Token { id: "studio-agent".into(), agent: true, safety: Some(over), ..Token::open() }
    }

    fn guard(cfg: SafetyConfig) -> Arc<Guard> {
        Guard::new(cfg, 30)
    }

    fn fast() -> SafetyConfig {
        SafetyConfig { min_hold_ms: 1_000, flash_guard: false, ..SafetyConfig::default() }
    }

    /// The acceptance line from 07 Phase 1: a second take inside the hold is
    /// refused, and the refusal carries the time left rather than "no".
    #[test]
    fn a_second_take_inside_the_hold_is_refused_with_the_time_left() {
        let g = guard(SafetyConfig { flash_guard: false, ..SafetyConfig::default() });
        let t0 = Instant::now();
        assert!(g.check_at(&human(), t0).is_ok(), "the first take is always allowed");
        g.record_at("desk", t0);

        let refusal = g.check_at(&human(), t0 + Duration::from_millis(1_200)).unwrap_err();
        assert_eq!(refusal.rule, "min_hold");
        assert_eq!(refusal.retry_after_ms, 6_800);
        assert!(refusal.message.contains("6800 ms"), "{}", refusal.message);

        // And it lets go on its own once the hold is served.
        assert!(g.check_at(&human(), t0 + Duration::from_millis(8_001)).is_ok());
    }

    #[test]
    fn the_rate_limit_counts_a_rolling_minute() {
        let g = guard(fast());
        let t0 = Instant::now();
        for i in 0..12 {
            let at = t0 + Duration::from_millis(i * 1_500);
            assert!(g.check_at(&human(), at).is_ok(), "take {i} should pass");
            g.record_at("desk", at);
        }
        let at = t0 + Duration::from_millis(18_000);
        let refusal = g.check_at(&human(), at).unwrap_err();
        assert_eq!(refusal.rule, "rate_limit");
        assert!(refusal.retry_after_ms > 0);
        assert!(refusal.message.contains("12"), "{}", refusal.message);

        // A minute after the first one, there is room again.
        assert!(g.check_at(&human(), t0 + Duration::from_millis(61_000)).is_ok());
    }

    #[test]
    fn the_flash_guard_holds_a_cut_to_the_separation_the_standard_names() {
        let g = guard(SafetyConfig { min_hold_ms: 0, ..SafetyConfig::default() });
        assert_eq!(g.flash_separation_ms(), 360);
        assert_eq!(Guard::new(SafetyConfig::default(), 60).flash_separation_ms(), 334);

        g.set_luma_observed(true);
        g.note_flash();
        let refusal = g.check(&human()).unwrap_err();
        assert_eq!(refusal.rule, "flash_guard");
        assert!(refusal.retry_after_ms <= 360 && refusal.retry_after_ms > 0);
        assert!(refusal.message.contains("BT.1702-3"), "{}", refusal.message);
    }

    #[test]
    fn three_flashes_in_a_second_is_the_most_the_standard_allows() {
        let g = guard(SafetyConfig { min_hold_ms: 0, ..SafetyConfig::default() });
        let t0 = Instant::now();
        {
            let mut state = g.state.lock();
            state.luma_observed = true;
            for i in 0..3 {
                state.flashes.push_back(t0 + Duration::from_millis(i * 100));
            }
        }
        // Past the separation from the last of them, but still three inside
        // the second.
        let refusal = g.check_at(&human(), t0 + Duration::from_millis(600)).unwrap_err();
        assert_eq!(refusal.rule, "flash_guard");
        assert!(refusal.message.contains("three") || refusal.message.contains('3'), "{}", refusal.message);
        // Once the first has aged out of the second, there is room again.
        assert!(g.check_at(&human(), t0 + Duration::from_millis(1_050)).is_ok());
    }

    /// Without telemetry the core cannot tell a flash from a dissolve, so it
    /// assumes the stricter thing rather than turning the rule off.
    #[test]
    fn without_luminance_every_cut_is_treated_as_a_possible_flash() {
        let g = guard(SafetyConfig { min_hold_ms: 0, ..SafetyConfig::default() });
        g.record("desk");
        assert_eq!(g.check(&human()).unwrap_err().rule, "flash_guard");

        let g = guard(SafetyConfig { min_hold_ms: 0, ..SafetyConfig::default() });
        g.set_luma_observed(true);
        g.record("desk");
        assert!(g.check(&human()).is_ok(), "with probes on, an ordinary cut is not a flash");
    }

    /// A human surface may raise the limits per token; an agent's token may
    /// only make them harder, so an agent told to loosen its own leash finds
    /// that it cannot.
    #[test]
    fn a_human_token_moves_the_limits_and_an_agent_token_only_tightens() {
        let cfg = SafetyConfig::default();
        let looser = TokenSafety {
            min_hold_ms: Some(500),
            max_takes_per_minute: Some(120),
            flash_guard: Some(false),
        };
        let vision_mixer = Token { id: "desk".into(), safety: Some(looser.clone()), ..Token::open() };
        let limits = cfg.for_token(&vision_mixer);
        assert_eq!(limits.min_hold_ms, 500);
        assert_eq!(limits.max_takes_per_minute, 120);
        assert!(!limits.flash_guard);

        let limits = cfg.for_token(&agent(looser));
        assert_eq!(limits.min_hold_ms, 8_000, "an agent cannot shorten the hold");
        assert_eq!(limits.max_takes_per_minute, 12);
        assert!(limits.flash_guard, "and cannot turn the flash guard off");

        // Tightening is allowed from either side.
        let stricter = TokenSafety {
            min_hold_ms: Some(20_000),
            max_takes_per_minute: Some(4),
            flash_guard: None,
        };
        let limits = cfg.for_token(&agent(stricter));
        assert_eq!(limits.min_hold_ms, 20_000);
        assert_eq!(limits.max_takes_per_minute, 4);

        // A token with no override is held to the config, whoever it is.
        assert_eq!(cfg.for_token(&human()).min_hold_ms, 8_000);
    }

    #[test]
    fn operator_silence_watches_whoever_made_the_last_take() {
        let cfg = SafetyConfig {
            min_hold_ms: 0,
            flash_guard: false,
            on_operator_silence: OperatorSilence { after_secs: 0, action: SilenceAction::Hold },
            ..SafetyConfig::default()
        };
        let g = guard(cfg);
        assert_eq!(g.silent_operator(), None, "nobody has taken yet");
        g.record("studio-agent");
        assert_eq!(g.silent_operator().as_deref(), Some("studio-agent"));

        // The hold refuses takes and names the way out.
        g.arm_silence(true);
        let refusal = g.check(&human()).unwrap_err();
        assert_eq!(refusal.rule, "operator_silence");
        assert!(refusal.message.contains("studio-agent"), "{}", refusal.message);
        // Any call at all releases it.
        g.note_call("someone-else");
        assert!(g.check(&human()).is_ok());
    }

    #[test]
    fn the_silence_action_reads_and_writes_the_four_spellings() {
        assert_eq!(SilenceAction::parse("alert"), Ok(SilenceAction::Alert));
        assert_eq!(SilenceAction::parse("hold"), Ok(SilenceAction::Hold));
        assert_eq!(SilenceAction::parse("slate"), Ok(SilenceAction::Slate));
        assert_eq!(
            SilenceAction::parse("fallback:cam-wide"),
            Ok(SilenceAction::Fallback("cam-wide".into()))
        );
        assert_eq!(SilenceAction::Fallback("cam-wide".into()).as_str(), "fallback:cam-wide");
        let e = SilenceAction::parse("slat").unwrap_err();
        assert!(e.contains("slate"), "the message has to name what would have worked: {e}");
        assert!(SilenceAction::parse("fallback:").is_err());
    }

    #[test]
    fn the_config_round_trips_through_toml_with_the_documented_defaults() {
        let cfg: SafetyConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.min_hold_ms, 8_000);
        assert_eq!(cfg.max_takes_per_minute, 12);
        assert!(cfg.flash_guard);
        assert_eq!(cfg.on_operator_silence.after_secs, 120);
        assert_eq!(cfg.on_operator_silence.action, SilenceAction::Alert);

        let cfg: SafetyConfig = toml::from_str(
            "min_hold_ms = 2000\non_operator_silence = { after_secs = 30, action = \"fallback:cam1\" }",
        )
        .unwrap();
        assert_eq!(cfg.min_hold_ms, 2_000);
        assert_eq!(cfg.max_takes_per_minute, 12, "an unwritten key keeps its default");
        assert_eq!(cfg.on_operator_silence.action, SilenceAction::Fallback("cam1".into()));
        let text = toml::to_string(&cfg).unwrap();
        assert_eq!(toml::from_str::<SafetyConfig>(&text).unwrap(), cfg);
    }

    /// The BT.1702-3 threshold, on the reference display the standard is
    /// written for.
    #[test]
    fn a_flash_is_a_luminance_step_over_a_quarter_of_the_picture() {
        assert_eq!(luminance_cd_m2(16), 0.0);
        assert!((luminance_cd_m2(235) - 100.0).abs() < 1e-9);
        assert!(luminance_cd_m2(0) == 0.0, "below black clamps rather than going negative");

        let black = vec![16u8; 100];
        let white = vec![235u8; 100];
        assert!(is_flash(&black, &white));
        assert!(is_flash(&white, &black), "it is the size of the step, not its direction");

        // A quarter of the frame is not more than a quarter.
        let mut quarter = black.clone();
        quarter[..25].fill(235);
        assert!(!is_flash(&black, &quarter));
        let mut just_over = black.clone();
        just_over[..26].fill(235);
        assert!(is_flash(&black, &just_over));

        // A small step everywhere is not a flash.
        let dim = vec![120u8; 100];
        let dimmer = vec![115u8; 100];
        assert!(!is_flash(&dim, &dimmer));

        // Mismatched or empty grids answer false rather than guessing.
        assert!(!is_flash(&black, &[]));
        assert!(!is_flash(&[], &[]));
    }
}
