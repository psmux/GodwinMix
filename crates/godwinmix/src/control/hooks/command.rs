//! `mode = "command"`: run a command with the event as JSON on its stdin.
//!
//! The mode that needs nothing but a shell script. It is how a GPI box gets
//! pulsed, how a compliance log gets a line, and how somebody tries a hook out
//! before writing a plugin.
//!
//! Two ways to refuse a `take.before`: print `{"allow": false, "reason": ".."}`
//! on stdout, or exit 2. The exit code is the one Claude Code uses and the one
//! 03 section 8 names, and it is what a three line shell script will reach for.

use anyhow::Context;
use godwinmix_core::hooks::{blocks, Decision, Hook};
use serde_json::Value;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

/// Spawn one hook command and read what it says.
///
/// The child is killed when the caller's timeout fires, because
/// `tokio::process::Command` with `kill_on_drop` is what stops a `take.before`
/// script that sleeps from outliving the show.
pub async fn run(argv: &[String], hook: &Hook, body: Value) -> anyhow::Result<Decision> {
    let (program, args) = argv.split_first().context("a hook command with no program in it")?;
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .env("GMX_HOOK", &hook.event)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("running the hook command `{program}`"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let line = format!("{body}\n");
        // A hook that never reads its stdin closes the pipe; that is not an
        // error, it just means the hook did not want the event.
        let _ = stdin.write_all(line.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }
    let output = child.wait_with_output().await.context("waiting for the hook command")?;
    if !blocks(&hook.event) {
        // Nobody waited on this one. A non zero exit is still worth saying,
        // because a hook that has been failing all show is a thing an operator
        // wants to know before they need it.
        if !output.status.success() {
            anyhow::bail!(
                "the hook command `{program}` exited {}: {}",
                output.status.code().map(|c| c.to_string()).unwrap_or_else(|| "on a signal".into()),
                first_line(&output.stderr)
            );
        }
        return Ok(Decision::Allow);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let said = stdout.trim();
    if !said.is_empty() {
        if let Ok(value) = serde_json::from_str::<Value>(said) {
            let decision = Decision::parse(&value);
            if decision.is_refusal() {
                return Ok(decision);
            }
        }
    }
    Ok(Decision::from_exit(output.status.code(), &String::from_utf8_lossy(&output.stderr)))
}

fn first_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).lines().next().unwrap_or("").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_core::hooks::HookConfig;

    fn hook(command: &str, event: &str) -> Hook {
        HookConfig { event: event.into(), command: Some(command.into()), ..HookConfig::default() }
            .build(0)
            .expect("a valid hook")
    }

    async fn decide(command: &str, event: &str, body: Value) -> anyhow::Result<Decision> {
        let hook = hook(command, event);
        let godwinmix_core::hooks::Mode::Command(argv) = hook.mode.clone() else {
            unreachable!("built from a command")
        };
        run(&argv, &hook, body).await
    }

    /// A script on disk rather than a `sh -c` one liner: the thing being
    /// tested is what arrives on stdin, and three layers of shell quoting in
    /// a Rust string literal tests the quoting instead.
    fn script(name: &str, body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gmx-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp directory");
        let path = dir.join("hook.sh");
        std::fs::write(&path, body).expect("writing the hook script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    #[tokio::test]
    #[cfg_attr(windows, ignore = "the test scripts are sh")]
    async fn the_event_arrives_on_stdin_and_a_json_refusal_stops_the_take() {
        let path = script(
            "hook-stdin",
            "#!/bin/sh\nread line\ncase \"$line\" in\n  *cam3*) echo '{\"allow\": false, \"reason\": \"no audio on cam3\"}' ;;\n  *) echo '{\"allow\": true}' ;;\nesac\n",
        );
        let decision = decide(
            &format!("sh {}", path.display()),
            "take.before",
            serde_json::json!({ "hook": "take.before", "payload": { "source": "cam3" } }),
        )
        .await
        .expect("ran");
        assert_eq!(decision, Decision::Refuse { reason: "no audio on cam3".into() });

        // The same script, a source it does not object to.
        let allowed = decide(
            &format!("sh {}", path.display()),
            "take.before",
            serde_json::json!({ "hook": "take.before", "payload": { "source": "cam1" } }),
        )
        .await
        .expect("ran");
        assert_eq!(allowed, Decision::Allow);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    #[cfg_attr(windows, ignore = "the test scripts are sh")]
    async fn exit_two_blocks_and_exit_one_does_not() {
        let blocked = decide("sh -c 'echo why 1>&2; exit 2'", "take.before", Value::Null)
            .await
            .expect("ran");
        assert_eq!(blocked, Decision::Refuse { reason: "why".into() });

        let broken = decide("sh -c 'exit 1'", "take.before", Value::Null).await.expect("ran");
        assert_eq!(broken, Decision::Allow, "only exit 2 blocks");
    }

    #[tokio::test]
    #[cfg_attr(windows, ignore = "the test scripts are sh")]
    async fn a_command_that_is_not_there_is_an_error_not_a_refusal() {
        let e = decide("definitely-not-a-program-xyz", "take.after", Value::Null).await.unwrap_err();
        assert!(format!("{e:#}").contains("definitely-not-a-program-xyz"));
    }

    #[tokio::test]
    #[cfg_attr(windows, ignore = "the test scripts are sh")]
    async fn a_failing_non_blocking_hook_is_reported_with_its_first_line() {
        let e = decide("sh -c 'echo the tally box is unplugged 1>&2; exit 3'", "take.after", Value::Null)
            .await
            .unwrap_err();
        let message = format!("{e}");
        assert!(message.contains("exited 3"), "{message}");
        assert!(message.contains("the tally box is unplugged"), "{message}");
    }
}
