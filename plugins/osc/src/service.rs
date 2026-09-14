//! The socket, the event stream, and the loop between them.
//!
//! Two tasks on one current thread runtime. One reads datagrams and turns each
//! into a call on the core. The other reads the core's event stream and turns
//! tally and programme changes into datagrams. Neither blocks the other and
//! neither can stall the programme: this process is not on the media path at
//! all, and the worst a wedged OSC surface can do is fill a UDP receive buffer
//! that the kernel then drops from.

use std::net::SocketAddr;
use std::sync::Arc;

use godwinmix_client::{Client, Event};
use serde_json::json;
use tokio::net::UdpSocket;
use tokio::sync::watch;

use crate::map::{self, Action, Refusal};
use crate::osc::{self, Arg, Message};
use crate::settings::Settings;

/// Where log lines go. The sidecar writes them to the core over stderr; a
/// plugin run by hand writes them to its own stderr. One trait rather than two
/// copies of the loop.
pub trait Log: Send + Sync + 'static {
    fn info(&self, message: &str);
    fn warn(&self, message: &str);
}

/// The logger a plugin run by hand uses.
pub struct Stderr;

impl Log for Stderr {
    fn info(&self, message: &str) {
        eprintln!("gmx-osc: {message}");
    }
    fn warn(&self, message: &str) {
        eprintln!("gmx-osc: {message}");
    }
}

/// Everything the loop needs that is not settings.
pub struct Wiring {
    pub url: String,
    pub token: Option<String>,
    pub log: Arc<dyn Log>,
}

/// Run until the process is told to stop.
///
/// `settings` is a watch channel rather than a value: `configure` arrives on
/// the core's stdio channel while this is running, and rebinding the socket on
/// a new port must not mean restarting the process.
pub async fn run(wiring: Wiring, mut settings: watch::Receiver<Settings>) {
    loop {
        let current = settings.borrow().clone();
        let client = match Client::connect(&wiring.url, wiring.token.as_deref()).await {
            Ok(client) => client,
            Err(error) => {
                wiring
                    .log
                    .warn(&format!("cannot reach the core at {}: {error}. Trying again in two seconds.", wiring.url));
                if wait_or_changed(&mut settings, 2_000).await {
                    continue;
                }
                continue;
            }
        };
        wiring.log.info(&format!("connected to {}", wiring.url));

        // Tally is an `ext` stream: nothing runs in the core unless a client
        // asks, and this is the asking.
        if let Err(error) = client
            .subscribe(&["program.*", "source.state", "output.state", "tally", "flush"], json!({"tally": current.send_tally}))
            .await
        {
            wiring.log.warn(&format!("the core refused core.subscribe: {error}"));
        }

        let reason = serve(&wiring, &client, &current, &mut settings).await;
        client.close();
        wiring.log.info(&format!("{reason}; reconnecting"));
    }
}

/// One connection's worth of work. Returns when the settings change or the
/// core goes away, with the reason in words.
async fn serve(
    wiring: &Wiring,
    client: &Client,
    settings: &Settings,
    changes: &mut watch::Receiver<Settings>,
) -> String {
    // Two sockets rather than one. Sending a datagram to a port nobody is
    // listening on gets an ICMP port unreachable back, and the kernel reports
    // it on the *next* operation on that socket, which on one socket means a
    // tally message to a tablet that is asleep kills the listener that takes
    // sources. A surface being off must never stop the mixer being driven.
    let socket = match UdpSocket::bind(&settings.listen).await {
        Ok(socket) => Arc::new(socket),
        Err(error) => {
            wiring.log.warn(&format!(
                "cannot listen on {}: {error}. Another program has the port, or the address is \
                 not one of this machine's. Change `listen` and the plugin picks it up without \
                 a restart.",
                settings.listen
            ));
            // Nothing to do but wait for someone to change the setting.
            changes.changed().await.ok();
            return "the listen address changed".into();
        }
    };
    let out = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(out) => out,
        Err(error) => {
            wiring
                .log
                .warn(&format!("cannot open a socket to send from: {error}"));
            changes.changed().await.ok();
            return "the settings changed".into();
        }
    };
    wiring
        .log
        .info(&format!("listening for OSC on {}", settings.listen));

    let mut events = client.events();
    let mut buffer = vec![0u8; 8192];

    loop {
        tokio::select! {
            received = socket.recv_from(&mut buffer) => match received {
                Ok((length, peer)) => {
                    handle_packet(wiring, client, settings, &buffer[..length], peer).await;
                }
                // A refused or reset connection here is the echo of a datagram
                // this process sent somewhere nobody was listening. It says
                // nothing about the listener, so carry on.
                Err(error) if transient(&error) => {}
                Err(error) => return format!("the OSC socket failed: {error}"),
            },
            event = events.recv() => match event {
                Ok(event) => send_out(wiring, &out, settings, &event).await,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    wiring.log.warn(&format!("fell {n} events behind; the next tally message is the truth"));
                }
                Err(_) => return "the core closed the event stream".into(),
            },
            _ = changes.changed() => return "the settings changed".into(),
        }
    }
}

/// One datagram in.
async fn handle_packet(
    wiring: &Wiring,
    client: &Client,
    settings: &Settings,
    bytes: &[u8],
    peer: SocketAddr,
) {
    if !settings.allows(&peer) {
        wiring.log.warn(&format!(
            "ignored a packet from {peer}, which is not in allow_from. Add its address to the \
             list, or empty the list to accept anything on the show LAN."
        ));
        return;
    }
    let messages = match osc::decode(bytes) {
        Ok(messages) => messages,
        Err(error) => {
            wiring.log.warn(&format!("{peer} sent {error}"));
            return;
        }
    };
    for message in messages {
        match map::action_for(&message) {
            Ok(action) => apply(wiring, client, &message, action).await,
            Err(Refusal::Released) => {}
            Err(Refusal::Unknown(why)) => wiring.log.warn(&format!("{peer}: {why}")),
        }
    }
}

/// One action becomes one call. Every failure is logged with the core's own
/// message, which already names the state and the next step.
async fn apply(wiring: &Wiring, client: &Client, message: &Message, action: Action) {
    let (method, params) = match &action {
        Action::Take(Some(id)) => ("program.take", json!({"source": id})),
        Action::Take(None) => ("program.take", json!({"source": null})),
        Action::TakeScene(name) => ("program.take", json!({"scene": name})),
        Action::Revert => ("program.revert", json!({})),
        Action::Gain { id, gain } => ("source.audio.set", json!({"id": id, "gain": gain})),
        Action::Mute { id, muted } => ("source.audio.set", json!({"id": id, "muted": muted})),
        Action::OutputReconnect(id) => ("output.reconnect", json!({"id": id})),
        Action::OutputRemove(id) => ("output.remove", json!({"id": id})),
    };
    match client.call_value(method, params).await {
        Ok(_) => wiring
            .log
            .info(&format!("{} -> {method}", message.address)),
        Err(error) => wiring
            .log
            .warn(&format!("{} -> {method} refused: {error}", message.address)),
    }
}

/// One event out, as OSC, to every configured target.
async fn send_out(wiring: &Wiring, socket: &UdpSocket, settings: &Settings, event: &Event) {
    if settings.send_to.is_empty() {
        return;
    }
    let mut out: Vec<Message> = Vec::new();
    match event {
        Event::Tally(tally) if settings.send_tally => {
            for (id, state) in &tally.sources {
                let Some(state) = state.as_str() else { continue };
                out.push(Message::new(
                    settings.address(&format!("/tally/{id}")),
                    vec![Arg::Int(tally_number(state)), Arg::Str(state.to_string())],
                ));
            }
        }
        Event::ProgramTook(took) if settings.send_program => {
            out.push(Message::new(
                settings.address("/program"),
                vec![Arg::Str(took.source.clone().unwrap_or_default())],
            ));
        }
        Event::SourceState(event) if settings.send_program => {
            let (Some(id), Some(state)) = (&event.source, &event.state) else {
                return;
            };
            out.push(Message::new(
                settings.address(&format!("/source/{id}/state")),
                vec![Arg::Str(state.clone())],
            ));
        }
        Event::OutputState(event) if settings.send_program => {
            let (Some(id), Some(state)) = (&event.output, &event.state) else {
                return;
            };
            out.push(Message::new(
                settings.address(&format!("/output/{id}/state")),
                vec![Arg::Str(state.clone())],
            ));
        }
        _ => return,
    }
    for message in out {
        let bytes = message.encode();
        for target in &settings.send_to {
            match socket.send_to(&bytes, target).await {
                Ok(_) => {}
                // An ICMP port unreachable from an earlier datagram is
                // delivered on the next send, so the first message after a
                // surface comes back would otherwise be the one that is lost.
                // One retry clears it. A surface that is still off fails again
                // and that is the ordinary case, not an incident.
                Err(error) if transient(&error) => {
                    let _ = socket.send_to(&bytes, target).await;
                }
                Err(error) => wiring
                    .log
                    .warn(&format!("could not send {} to {target}: {error}", message.address)),
            }
        }
    }
}

/// Errors that mean "the far end was not there", which on a connectionless
/// socket is news about the far end and not about this one.
fn transient(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::HostUnreachable
            | std::io::ErrorKind::NetworkUnreachable
            | std::io::ErrorKind::Interrupted
    )
}

/// The number a lamp on an OSC surface wants: 0 off, 1 programme, 2 preview.
/// The string goes out beside it so a surface that would rather read words can.
pub fn tally_number(state: &str) -> i32 {
    match state {
        "program" => 1,
        "preview" => 2,
        _ => 0,
    }
}

/// Sleep, unless the settings change first. `true` means they changed.
async fn wait_or_changed(changes: &mut watch::Receiver<Settings>, ms: u64) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(std::time::Duration::from_millis(ms)) => false,
        _ = changes.changed() => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tally_state_becomes_the_number_a_lamp_wants() {
        assert_eq!(tally_number("program"), 1);
        assert_eq!(tally_number("preview"), 2);
        assert_eq!(tally_number("off"), 0);
        assert_eq!(tally_number("something new"), 0);
    }
}
