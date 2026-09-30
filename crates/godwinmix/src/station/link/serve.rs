//! The station's half of the link: one task per show, holding the tickets
//! the governor granted it, dropped all at once when the show goes.

use super::{Hello, Line};
use godwinmix_govern::{Ask, Governor, Ticket};
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info, warn};

/// What the link tells the station about its shows.
pub trait Host: Send + Sync + 'static {
    /// A show said hello. False turns it away (an unknown show, a wrong
    /// secret).
    fn hello(&self, hello: &Hello) -> bool;
    fn on_air(&self, show: &str, on: bool);
    /// What the show measures of its own CPU, thousandths of a core.
    fn load(&self, show: &str, millicores: u32);
    /// Its link closed: it died, or it is stopping.
    fn gone(&self, show: &str, pid: u32);
    fn governor(&self) -> Governor;
}

/// Listen on a loopback port the system picks, and serve every show that
/// connects. Answers with the address, for the shows' command lines.
pub async fn listen(host: Arc<dyn Host>) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let host = host.clone();
                    tokio::spawn(async move { serve(stream, host).await });
                }
                Err(e) => warn!(error = %e, "the show link could not accept a connection"),
            }
        }
    });
    Ok(addr)
}

async fn serve(stream: TcpStream, host: Arc<dyn Host>) {
    let _ = stream.set_nodelay(true);
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    let Ok(Some(first)) = lines.next_line().await else { return };
    let hello = serde_json::from_str::<Line>(&first)
        .ok()
        .filter(|l| l.method.as_deref() == Some("show.hello"))
        .and_then(|l| serde_json::from_value::<Hello>(l.params).ok());
    let Some(hello) = hello.filter(|h| host.hello(h)) else {
        warn!("a connection to the show link did not say a show's hello; closed");
        return;
    };
    info!(show = %hello.show, addr = %hello.addr, "a show linked to the station");
    let governor = host.governor();
    let mut tickets: HashMap<u64, Ticket> = HashMap::new();
    let mut watch = tokio::time::interval(SHED_EVERY);
    loop {
        // The watch wakes only while this show holds something to shed.
        let holding = !tickets.is_empty();
        let reply = tokio::select! {
            line = lines.next_line() => match line {
                Ok(Some(text)) => match serde_json::from_str::<Line>(&text) {
                    Ok(line) => handle(&hello.show, line, &governor, &mut tickets, host.as_ref()),
                    Err(_) => None,
                },
                _ => break,
            },
            _ = watch.tick(), if holding => shed_for(&governor, &tickets),
        };
        if let Some(reply) = reply {
            if write.write_all(reply.text().as_bytes()).await.is_err() {
                break;
            }
        }
    }
    debug!(show = %hello.show, held = tickets.len(), "the show's link closed; its tickets go back");
    drop(tickets);
    host.gone(&hello.show, hello.pid);
}

/// How often a show holding tickets is told what to give up, while the
/// machine is over its line. Nothing is sent while it is not.
const SHED_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

/// The station's plan for the whole machine, cut down to this show's tickets.
fn shed_for(governor: &Governor, tickets: &HashMap<u64, Ticket>) -> Option<Line> {
    let steps: Vec<_> = governor.shed().into_iter().filter(|s| tickets.contains_key(&s.ticket)).collect();
    (!steps.is_empty()).then(|| Line::call(None, "governor.shed", serde_json::json!({ "steps": steps })))
}

fn handle(show: &str, line: Line, governor: &Governor, tickets: &mut HashMap<u64, Ticket>, host: &dyn Host) -> Option<Line> {
    match line.method.as_deref()? {
        "governor.admit" => {
            let id = line.id?;
            let ask: Ask = serde_json::from_value(line.params).ok()?;
            let (answer, ticket) = governor.answer(ask);
            if let Some(t) = ticket {
                tickets.insert(t.id(), t);
            }
            Some(Line::reply(id, serde_json::to_value(answer).ok()?))
        }
        "governor.release" => {
            let ticket = line.params.get("ticket").and_then(Value::as_u64)?;
            tickets.remove(&ticket);
            // It stops reporting once it holds nothing; its last word must
            // not stand for work it no longer does.
            if tickets.is_empty() {
                host.load(show, 0);
            }
            None
        }
        "show.load" => {
            let m = line.params.get("millicores").and_then(Value::as_u64).unwrap_or(0);
            host.load(show, u32::try_from(m).unwrap_or(u32::MAX));
            None
        }
        "show.on_air" => {
            host.on_air(show, line.params.get("on").and_then(Value::as_bool).unwrap_or(false));
            None
        }
        other => {
            debug!(show, method = other, "the show link does not know this; ignored");
            None
        }
    }
}
