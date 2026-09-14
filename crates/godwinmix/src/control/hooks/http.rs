//! `mode = "http"`: POST the event and read the answer.
//!
//! The mode for a hook that has no plugin behind it. An operator writes
//!
//! ```toml
//! [[hooks]]
//! event = "take.after"
//! http = "https://tally.example/on-take"
//! ```
//!
//! and every take arrives there as a JSON body. `docs/how-to/hooks.md` has a
//! ten line receiver to try it against.

use anyhow::Context;
use godwinmix_core::hooks::{blocks, Decision, Hook};
use serde_json::Value;

/// POST one hook body.
///
/// The request carries its own timeout as well as the caller's, because a
/// hook that cannot delay a decision is not awaited by anybody and a POST to a
/// host that never answers would otherwise hold a connection for the life of
/// the show.
///
/// The body is the envelope from `godwinmix_core::hooks`: `{hook, ts,
/// payload}`. The answer only matters for `take.before`; a receiver of any
/// other hook can return anything at all, including nothing.
pub async fn post(
    client: &reqwest::Client,
    url: &str,
    hook: &Hook,
    body: Value,
) -> anyhow::Result<Decision> {
    // A blocking hook's deadline belongs to the caller: `Hooks::take_before`
    // abandons this future at `timeout_ms` and reports a timeout, which is a
    // better message than the client's own "operation timed out". The client
    // timeout here is only the ceiling that stops a wedged POST outliving the
    // show, which is what a non blocking hook needs.
    let ceiling = if blocks(&hook.event) {
        std::time::Duration::from_millis(godwinmix_core::hooks::FIRE_AND_FORGET_TIMEOUT_MS)
    } else {
        hook.timeout
    };
    let response = client
        .post(url)
        .timeout(ceiling)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url} for hook {}", hook.event))?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!(
            "POST {url} for hook {} answered {}. The take went ahead; fix the receiver or \
             remove the hook from the config.",
            hook.event,
            status.as_u16()
        );
    }
    if !blocks(&hook.event) {
        // Nothing is waiting on this one, so the body is not read at all: a
        // receiver that answers with a megabyte of HTML should cost nothing.
        return Ok(Decision::Allow);
    }
    let text = response.text().await.unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(Decision::Allow);
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => Ok(Decision::parse(&value)),
        // Silence is consent and so is nonsense: a receiver that answers with
        // an error page must not take the programme off air.
        Err(e) => {
            tracing::warn!(url, "a take.before receiver answered something that is not JSON: {e}");
            Ok(Decision::Allow)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_core::hooks::HookConfig;
    use std::io::{BufRead, Write};

    /// A receiver on a real socket. The point of the test is the wire, so
    /// there is no mock: a thread, a TcpListener, one request each.
    fn receiver(answer: &'static str, status: &'static str) -> (String, std::thread::JoinHandle<Option<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let url = format!("http://{}/on-take", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().ok()?;
            let mut reader = std::io::BufReader::new(stream.try_clone().ok()?);
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).ok()? == 0 {
                    return None;
                }
                if let Some(n) = line.to_lowercase().strip_prefix("content-length:") {
                    length = n.trim().parse().unwrap_or(0);
                }
                if line == "\r\n" || line == "\n" {
                    break;
                }
            }
            let mut body = vec![0u8; length];
            std::io::Read::read_exact(&mut reader, &mut body).ok()?;
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                answer.len()
            );
            let _ = stream.flush();
            String::from_utf8(body).ok()
        });
        (url, handle)
    }

    fn hook(url: &str, event: &str) -> Hook {
        HookConfig { event: event.into(), http: Some(url.into()), ..HookConfig::default() }
            .build(0)
            .expect("a valid hook")
    }

    #[tokio::test]
    async fn a_receiver_gets_the_envelope_and_a_refusal_comes_back() {
        let (url, server) = receiver(r#"{"allow": false, "reason": "cam3 has no audio"}"#, "200 OK");
        let hook = hook(&url, "take.before");
        let body = godwinmix_core::hooks::envelope(
            "take.before",
            serde_json::json!({ "source": "cam3" }),
        );
        let decision = post(&reqwest::Client::new(), &url, &hook, body).await.expect("posted");
        assert_eq!(
            decision,
            Decision::Refuse { reason: "cam3 has no audio".into() }
        );
        let seen: Value = serde_json::from_str(&server.join().unwrap().expect("a body")).unwrap();
        assert_eq!(seen["hook"], "take.before");
        assert_eq!(seen["payload"]["source"], "cam3");
    }

    #[tokio::test]
    async fn a_receiver_that_says_nothing_lets_the_take_through() {
        let (url, server) = receiver("", "200 OK");
        let hook = hook(&url, "take.before");
        let decision = post(&reqwest::Client::new(), &url, &hook, Value::Null).await.expect("posted");
        assert_eq!(decision, Decision::Allow);
        let _ = server.join();
    }

    #[tokio::test]
    async fn a_receiver_that_errors_is_reported_and_does_not_stop_anything() {
        let (url, server) = receiver("no", "500 Internal Server Error");
        let hook = hook(&url, "take.after");
        let e = post(&reqwest::Client::new(), &url, &hook, Value::Null).await.unwrap_err();
        let message = format!("{e}");
        assert!(message.contains("500"), "{message}");
        assert!(message.contains("The take went ahead"), "{message}");
        let _ = server.join();
    }
}
