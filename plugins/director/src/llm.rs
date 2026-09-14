//! Consulting a model, when one is configured.
//!
//! The `llm` setting names a command, not an API key and not a vendor. The
//! director starts it, writes the prompt on its stdin, closes stdin and reads
//! one JSON object off its stdout:
//!
//! ```json
//! {"take": "cam2", "reason": "the speaker moved to the lectern"}
//! ```
//!
//! That shape is the one `examples/ai-director.py` uses, so a prompt written
//! for one works with the other. Any command that reads a prompt and writes an
//! answer fits: `claude -p`, `ollama run llama3.2`, a shell script, a Python
//! file. The reason the documentation says "MCP capable" is that a command
//! which can itself reach the mixer over MCP (`gmx mcp` on the other end of a
//! client's config) can look things up before answering, and the prompt tells
//! it so. Nothing here requires that; a command that only reads the prompt
//! works exactly as well.
//!
//! Two rules hold whatever the command is. The director never waits longer
//! than `llm_timeout_ms` for it, and it decides on the rules alone when the
//! wait runs out. And whatever the command answers goes through
//! [`crate::rules::check`] before it reaches the mixer.

use std::process::Stdio;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::rules::View;
use crate::settings::Settings;

/// What a model said.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// The source it named, or `None` for "leave it alone".
    pub take: Option<String>,
    pub reason: String,
}

/// Why there is no proposal. Every one of these is recoverable: the cycle
/// falls back to the rules and the show carries on.
#[derive(Debug, Clone, PartialEq)]
pub enum Trouble {
    /// The command did not answer in time.
    TimedOut(u64),
    /// The command could not be started, or died.
    Failed(String),
    /// It answered, but not with a decision.
    Unreadable(String),
}

impl std::fmt::Display for Trouble {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Trouble::TimedOut(ms) => write!(f, "the model did not answer inside {ms} ms"),
            Trouble::Failed(why) => write!(f, "the model command failed: {why}"),
            Trouble::Unreadable(why) => write!(f, "the model's reply was not a decision: {why}"),
        }
    }
}

/// The prompt: the goal, the rules the answer will be checked against, the
/// state, and the shape of the answer.
///
/// It is deliberately short. A director runs this every couple of seconds and
/// every token is charged to somebody; 09 section 2 puts a 1 Hz loop at
/// 3,000 tokens in on Opus 5 at $67.50 an hour, and the numbers only work if
/// the prompt is small and the picture is not sent every cycle.
pub fn prompt(view: &View, settings: &Settings) -> String {
    let mut text = String::with_capacity(512);
    text.push_str(
        "You are directing a live programme on a video mixer. Exactly one source is on air.\n",
    );
    if !settings.goal.is_empty() {
        text.push_str("The producer's goal: ");
        text.push_str(&settings.goal);
        text.push('\n');
    }
    text.push_str(&format!(
        "Rules the mixer enforces whatever you say: only a source whose state is \"live\" may \
         be taken, and the current shot is held for at least {:.0} seconds.\n",
        settings.min_hold_secs
    ));
    text.push_str("State:\n");
    text.push_str(&describe(view));
    text.push_str(
        "\nAnswer with one JSON object and nothing else:\n\
         {\"take\": \"<source id>\" or null, \"reason\": \"<one short sentence>\"}\n\
         Prefer null. A programme that changes shot every few seconds is unwatchable.\n",
    );
    text
}

/// The state as a few short lines rather than a JSON document, because this is
/// read by a model once every couple of seconds and the shape costs tokens.
pub fn describe(view: &View) -> String {
    let mut text = format!(
        "on air: {} (held {:.0}s)\n",
        view.program.as_deref().unwrap_or("slate"),
        view.held_secs
    );
    for shot in &view.sources {
        text.push_str(&format!("  {} {}", shot.id, shot.state));
        if let Some(motion) = shot.motion {
            text.push_str(&format!(" motion {motion:.2}"));
        }
        if let Some(idle) = shot.video_idle_ms {
            text.push_str(&format!(" idle {idle}ms"));
        }
        if shot.no_video {
            text.push_str(" no-video");
        }
        if shot.no_audio {
            text.push_str(" no-audio");
        }
        text.push('\n');
    }
    text
}

/// Run the command and read its answer.
pub async fn consult(settings: &Settings, prompt: &str) -> Result<Proposal, Trouble> {
    let argv = settings.llm_argv();
    let Some((program, arguments)) = argv.split_first() else {
        return Err(Trouble::Failed("the llm setting is empty".into()));
    };
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| Trouble::Failed(format!("{program}: {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        // A command that reads no input is not an error: it may take the
        // prompt some other way. Ignore the broken pipe.
        let _ = stdin.write_all(prompt.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }
    let mut stdout = child.stdout.take().ok_or_else(|| {
        Trouble::Failed("the command gave no stdout to read the decision from".into())
    })?;

    let timeout = std::time::Duration::from_millis(settings.llm_timeout_ms);
    let mut text = String::new();
    let read = tokio::time::timeout(timeout, stdout.read_to_string(&mut text)).await;
    match read {
        Err(_) => {
            let _ = child.start_kill();
            Err(Trouble::TimedOut(settings.llm_timeout_ms))
        }
        Ok(Err(error)) => Err(Trouble::Failed(error.to_string())),
        Ok(Ok(_)) => {
            let _ = child.wait().await;
            parse(&text)
        }
    }
}

/// Pull the decision out of whatever the command wrote: a bare object, an
/// object inside a code fence, or an object with prose around it.
pub fn parse(text: &str) -> Result<Proposal, Trouble> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(Trouble::Unreadable("it wrote nothing".into()));
    }
    let object = first_object(trimmed)
        .ok_or_else(|| Trouble::Unreadable(format!("no JSON object in {:?}", clip(trimmed))))?;
    let value: serde_json::Value = serde_json::from_str(object)
        .map_err(|e| Trouble::Unreadable(format!("{e} in {:?}", clip(object))))?;
    let Some(take) = value.get("take") else {
        return Err(Trouble::Unreadable(
            "the object has no \"take\" field".into(),
        ));
    };
    let take = match take {
        serde_json::Value::Null => None,
        serde_json::Value::String(id) if id.is_empty() => None,
        serde_json::Value::String(id) => Some(id.clone()),
        other => {
            return Err(Trouble::Unreadable(format!(
                "\"take\" must be a source id or null, not {other}"
            )))
        }
    };
    let reason = value
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    Ok(Proposal { take, reason })
}

/// The first balanced `{...}` in the text, ignoring braces inside strings.
fn first_object(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|b| *b == b'{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (at, byte) in bytes.iter().enumerate().skip(start) {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return text.get(start..=at);
                }
            }
            _ => {}
        }
    }
    None
}

fn clip(text: &str) -> String {
    text.chars().take(120).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::Shot;
    use serde_json::json;

    fn view() -> View {
        View {
            program: Some("cam1".into()),
            held_secs: 12.0,
            sources: vec![
                Shot {
                    id: "cam1".into(),
                    state: "live".into(),
                    motion: Some(0.03),
                    ..Shot::default()
                },
                Shot {
                    id: "cam2".into(),
                    state: "live".into(),
                    motion: Some(0.41),
                    no_audio: true,
                    ..Shot::default()
                },
            ],
            held_by_core: None,
        }
    }

    #[test]
    fn a_bare_object_is_read() {
        let decision = parse(r#"{"take": "cam2", "reason": "the speaker moved"}"#).unwrap();
        assert_eq!(decision.take.as_deref(), Some("cam2"));
        assert_eq!(decision.reason, "the speaker moved");
    }

    #[test]
    fn a_fenced_object_is_read() {
        let decision = parse("```json\n{\"take\": null, \"reason\": \"fine\"}\n```").unwrap();
        assert_eq!(decision.take, None);
    }

    #[test]
    fn an_object_with_prose_around_it_is_read() {
        let decision =
            parse("Let me think. {\"take\": \"cam2\"} That is my answer.").unwrap();
        assert_eq!(decision.take.as_deref(), Some("cam2"));
        assert_eq!(decision.reason, "", "no reason is not an error");
    }

    #[test]
    fn a_brace_inside_a_string_does_not_end_the_object() {
        let decision = parse(r#"{"take": null, "reason": "it said {take: cam2} which is odd"}"#)
            .unwrap();
        assert!(decision.reason.contains("{take: cam2}"));
    }

    #[test]
    fn an_empty_string_means_the_slate_rather_than_a_source_called_nothing() {
        assert_eq!(parse(r#"{"take": ""}"#).unwrap().take, None);
    }

    #[test]
    fn a_reply_with_no_take_field_is_refused_with_a_reason() {
        let Err(Trouble::Unreadable(why)) = parse(r#"{"reason": "I like cam2"}"#) else {
            panic!("expected a refusal");
        };
        assert!(why.contains("take"), "{why}");
    }

    #[test]
    fn a_take_that_is_not_a_string_is_refused() {
        assert!(matches!(parse(r#"{"take": 3}"#), Err(Trouble::Unreadable(_))));
    }

    #[test]
    fn nothing_at_all_is_refused_and_says_so() {
        assert!(matches!(parse("   "), Err(Trouble::Unreadable(_))));
        assert!(matches!(parse("I would rather not"), Err(Trouble::Unreadable(_))));
    }

    #[test]
    fn the_prompt_carries_the_goal_the_rules_and_the_state() {
        let settings = Settings::from_value(&json!({"goal": "follow the speaker", "min_hold_secs": 6}));
        let text = prompt(&view(), &settings);
        assert!(text.contains("follow the speaker"), "{text}");
        assert!(text.contains("at least 6 seconds"), "{text}");
        assert!(text.contains("cam2 live motion 0.41"), "{text}");
        assert!(text.contains("\"take\""), "{text}");
    }

    #[test]
    fn the_state_is_lines_rather_than_a_json_document() {
        let text = describe(&view());
        assert!(text.starts_with("on air: cam1 (held 12s)"), "{text}");
        assert!(text.contains("no-audio"), "{text}");
        assert!(!text.contains('{'), "no JSON, because it costs tokens: {text}");
    }

    /// A command on disk rather than a quoted one liner: the point of the test
    /// is what `consult` does with a child process, and a shell in the middle
    /// only adds its own quoting rules to the thing under test.
    #[cfg(unix)]
    fn script(name: &str, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("gmx-director-{name}-{}.sh", std::process::id()));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write the script");
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("make it executable");
        path
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_command_that_answers_is_read() {
        let path = script(
            "answers",
            "cat >/dev/null\nprintf '%s' '{\"take\":\"cam2\",\"reason\":\"ok\"}'",
        );
        let settings = Settings::from_value(&json!({ "llm": path.to_string_lossy() }));
        let decision = consult(&settings, "ignored").await.unwrap();
        assert_eq!(decision.take.as_deref(), Some("cam2"));
        assert_eq!(decision.reason, "ok");
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_command_that_never_answers_times_out_rather_than_stopping_the_show() {
        let path = script("slow", "sleep 30");
        let settings = Settings::from_value(&json!({
            "llm": path.to_string_lossy(),
            "llm_timeout_ms": 500
        }));
        assert_eq!(consult(&settings, "x").await, Err(Trouble::TimedOut(500)));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn a_command_that_does_not_exist_is_a_failure_and_not_a_panic() {
        let settings = Settings::from_value(&json!({"llm": "no-such-command-anywhere"}));
        assert!(matches!(consult(&settings, "x").await, Err(Trouble::Failed(_))));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_prompt_reaches_the_command_on_its_stdin() {
        let path = script(
            "stdin",
            "grep -q MARKER && printf '%s' '{\"take\":null,\"reason\":\"saw it\"}'",
        );
        let settings = Settings::from_value(&json!({ "llm": path.to_string_lossy() }));
        let decision = consult(&settings, "MARKER\n").await.unwrap();
        assert_eq!(decision.take, None);
        assert_eq!(decision.reason, "saw it");
        let _ = std::fs::remove_file(&path);
    }
}
