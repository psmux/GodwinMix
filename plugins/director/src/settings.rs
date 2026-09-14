//! What the operator set, read out of the validated settings object.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Seconds between decisions. Clamped to at least 1: nothing in the state
    /// changes faster than the frame rate.
    pub interval_secs: f64,
    /// Seconds a shot is held before another take is allowed. The single most
    /// important number here: a programme that changes shot every few seconds
    /// is unwatchable.
    pub min_hold_secs: f64,
    /// Seconds after which the programme moves on anyway, so a service does
    /// not sit on one camera for an hour. 0 turns it off.
    pub slow_look_secs: f64,
    /// How much more a source has to be moving than the programme before that
    /// is a reason to cut.
    pub motion_delta: f64,
    /// A picture that has not moved for this long is not a shot.
    pub idle_ms: u64,
    /// The sources this director may take. Empty means all of them.
    pub sources: Vec<String>,
    /// What the programme should show, in plain words. Only read when a model
    /// is configured.
    pub goal: String,
    /// A command to consult for each decision. Empty is the rule based mode,
    /// which is the default and is the whole of this plugin without it.
    pub llm: String,
    /// How long to wait for that command before deciding without it.
    pub llm_timeout_ms: u64,
    /// Decide and log, take nothing.
    pub dry_run: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            interval_secs: 2.0,
            min_hold_secs: 8.0,
            slow_look_secs: 45.0,
            motion_delta: 0.15,
            idle_ms: 2_000,
            sources: Vec::new(),
            goal: String::new(),
            llm: String::new(),
            llm_timeout_ms: 10_000,
            dry_run: false,
        }
    }
}

impl Settings {
    pub fn from_value(value: &Value) -> Settings {
        let mut settings = Settings::default();
        if let Some(n) = value.get("interval_secs").and_then(Value::as_f64) {
            settings.interval_secs = n.max(1.0);
        }
        if let Some(n) = value.get("min_hold_secs").and_then(Value::as_f64) {
            settings.min_hold_secs = n.max(0.0);
        }
        if let Some(n) = value.get("slow_look_secs").and_then(Value::as_f64) {
            settings.slow_look_secs = if n <= 0.0 { f64::INFINITY } else { n };
        }
        if let Some(n) = value.get("motion_delta").and_then(Value::as_f64) {
            settings.motion_delta = n.clamp(0.0, 1.0);
        }
        if let Some(n) = value.get("idle_ms").and_then(Value::as_u64) {
            settings.idle_ms = n;
        }
        if let Some(list) = value.get("sources").and_then(Value::as_array) {
            settings.sources = list
                .iter()
                .filter_map(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        }
        for (key, slot) in [("goal", &mut settings.goal), ("llm", &mut settings.llm)] {
            if let Some(text) = value.get(key).and_then(Value::as_str) {
                *slot = text.trim().to_string();
            }
        }
        if let Some(n) = value.get("llm_timeout_ms").and_then(Value::as_u64) {
            settings.llm_timeout_ms = n.clamp(500, 120_000);
        }
        if let Some(on) = value.get("dry_run").and_then(Value::as_bool) {
            settings.dry_run = on;
        }
        settings
    }

    /// Whether this director is allowed to take this source.
    pub fn may_take(&self, id: &str) -> bool {
        self.sources.is_empty() || self.sources.iter().any(|s| s == id)
    }

    /// Whether a model is being consulted at all.
    pub fn uses_a_model(&self) -> bool {
        !self.llm.is_empty()
    }

    /// The command split into a program and its arguments, the way a shell
    /// would split it, without a shell.
    pub fn llm_argv(&self) -> Vec<String> {
        split_words(&self.llm)
    }
}

/// Words, with single and double quotes honoured. Enough for the command lines
/// people actually write in a settings field, and no shell involved, so a
/// setting cannot become an injection.
pub fn split_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut any = false;
    for c in text.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => word.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                any = true;
            }
            None if c.is_whitespace() => {
                if any || !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                    any = false;
                }
            }
            None => word.push(c),
        }
    }
    if any || !word.is_empty() {
        words.push(word);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_defaults_are_the_ones_in_the_schema() {
        let settings = Settings::from_value(&json!({}));
        assert_eq!(settings, Settings::default());
        assert_eq!(settings.min_hold_secs, 8.0);
        assert!(!settings.uses_a_model(), "rule based unless told otherwise");
    }

    #[test]
    fn the_interval_is_never_faster_than_a_second() {
        assert_eq!(Settings::from_value(&json!({"interval_secs": 0.1})).interval_secs, 1.0);
    }

    #[test]
    fn a_slow_look_of_zero_turns_it_off_rather_than_firing_every_cycle() {
        let settings = Settings::from_value(&json!({"slow_look_secs": 0}));
        assert!(settings.slow_look_secs.is_infinite());
    }

    #[test]
    fn an_empty_source_list_allows_every_source() {
        let settings = Settings::default();
        assert!(settings.may_take("cam1") && settings.may_take("anything"));
    }

    #[test]
    fn a_source_list_allows_only_what_is_in_it() {
        let settings = Settings::from_value(&json!({"sources": ["cam1", "cam2", ""]}));
        assert_eq!(settings.sources, vec!["cam1", "cam2"]);
        assert!(settings.may_take("cam1"));
        assert!(!settings.may_take("cam3"));
    }

    #[test]
    fn a_model_command_is_split_without_a_shell() {
        let settings = Settings::from_value(&json!({"llm": "claude -p --output-format text"}));
        assert!(settings.uses_a_model());
        assert_eq!(settings.llm_argv(), vec!["claude", "-p", "--output-format", "text"]);
    }

    #[test]
    fn quotes_hold_a_word_together() {
        assert_eq!(split_words("a 'b c' d"), vec!["a", "b c", "d"]);
        assert_eq!(split_words(r#"a "b c""#), vec!["a", "b c"]);
        assert_eq!(split_words("  "), Vec::<String>::new());
        assert_eq!(split_words("x ''"), vec!["x", ""]);
    }

    #[test]
    fn the_motion_threshold_stays_inside_the_range_motion_is_measured_in() {
        assert_eq!(Settings::from_value(&json!({"motion_delta": 9.0})).motion_delta, 1.0);
        assert_eq!(Settings::from_value(&json!({"motion_delta": -1.0})).motion_delta, 0.0);
    }

    #[test]
    fn a_silly_model_timeout_is_brought_back_into_range() {
        assert_eq!(Settings::from_value(&json!({"llm_timeout_ms": 1})).llm_timeout_ms, 500);
        assert_eq!(
            Settings::from_value(&json!({"llm_timeout_ms": 999_999})).llm_timeout_ms,
            120_000
        );
    }
}
