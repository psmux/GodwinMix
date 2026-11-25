//! The loop that joins follow and lead to the core and the switchers, over
//! the public protocol: `event/program.took` in, `program.take` out.

use std::sync::Arc;
use std::time::{Duration, Instant};

use godwinmix_atem::Command;
use godwinmix_client::{Client, Event};
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use super::{follow, seen, Guard, Lead, Verdict, TRIPPED};
use crate::tools::Shared;
use crate::Log;

/// How often a leading switcher's programme is looked at.
const LOOK_EVERY: Duration = Duration::from_millis(40);

/// Run until the process ends, reconnecting to the core as needed.
pub async fn run(desk: Shared, url: String, token: Option<String>, log: Arc<dyn Log>) {
    loop {
        let client = match Client::connect(&url, token.as_deref()).await {
            Ok(c) => c,
            Err(e) => {
                log.warn(&format!("cannot reach the core at {url} for follow and lead: {e}. Trying again in two seconds"));
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        if let Err(e) = client.subscribe(&["program.*"], json!({})).await {
            log.warn(&format!("the core refused core.subscribe for program.*: {e}. Follow will not work until it allows it"));
        }
        let why = serve(&desk, &client, &*log).await;
        client.close();
        log.info(&format!("{why}; reconnecting to the core"));
    }
}

async fn serve(desk: &Shared, client: &Client, log: &dyn Log) -> String {
    let mut events = client.events();
    let mut look = tokio::time::interval(LOOK_EVERY);
    look.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let (mut lead, mut guard) = (Lead::default(), Guard::default());
    let mut mix: Option<String> = None;
    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok(Event::ProgramTook(took)) => {
                    mix = took.source.or(took.scene);
                    let Some(source) = mix.clone() else { continue };
                    let d = desk.lock().unwrap_or_else(|e| e.into_inner());
                    for (id, input) in follow(&seen(&d), &source) {
                        if !allowed(&mut guard, log) { break; }
                        let sent = d.switchers.get(&id).map_err(|e| e.to_string())
                            .and_then(|a| a.send_now(vec![Command::Program { me: 0, source: input }]).map_err(|e| e.to_string()));
                        match sent {
                            Ok(()) => log.info(&format!("follow: GodwinMix took {source}, so {id} cuts to input {input}")),
                            Err(e) => log.warn(&format!("follow: could not cut {id} to input {input}: {e}")),
                        }
                    }
                }
                Ok(_) => {}
                Err(RecvError::Lagged(n)) => log.warn(&format!("fell {n} events behind the core; follow catches up on the next take")),
                Err(RecvError::Closed) => return "the core closed the event stream".into(),
            },
            _ = look.tick() => {
                let moves = lead.changes(&seen(&desk.lock().unwrap_or_else(|e| e.into_inner())), mix.as_deref());
                for (id, source) in moves {
                    if !allowed(&mut guard, log) { break; }
                    match client.take(Some(&source)).await {
                        Ok(_) => { log.info(&format!("lead: {id} cut, so GodwinMix takes {source}")); mix = Some(source); }
                        Err(e) => log.warn(&format!("lead: {id} cut, but GodwinMix refused to take {source}: {e}")),
                    }
                }
            }
        }
    }
}

fn allowed(guard: &mut Guard, log: &dyn Log) -> bool {
    match guard.check(Instant::now()) {
        Verdict::Go => true,
        Verdict::Tripped => {
            log.warn(TRIPPED);
            false
        }
        Verdict::Paused => false,
    }
}
