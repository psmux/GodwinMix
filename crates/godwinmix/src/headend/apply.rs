//! Making the shows match the file, through the station's own methods: a
//! new stream is `show.add_many`, a changed one is removed and made again,
//! `disabled` is `show.stop` or `show.start`, a title is `show.rename`.
//! Nothing that did not change is touched.

use super::saved::{self, Applied};
use super::state::Headend;
use crate::station::methods;
use godwinmix_flussonic::diff::diff;
use godwinmix_flussonic::import::{self, ShowPlan};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tracing::{info, warn};

async fn call(h: &Headend, method: &str, params: Value) -> Result<Value, String> {
    methods::call(&h.st, &h.token, method, params).await.map_err(|e| e.message)
}

/// Import the document in hand and apply it. `announce` logs every line
/// the import does not carry, which is for a file just read.
pub async fn apply_file(h: &Headend, announce: bool) {
    let (plans, notes) = {
        let doc = h.doc.lock();
        let i = import::import(&doc.config);
        (i.shows, i.notes)
    };
    for (scope, n) in notes.iter().filter(|_| announce) {
        info!(line = n.line, scope = %scope, kind = ?n.kind, "headend import: {}", n.what);
    }
    apply(h, plans).await;
}

pub async fn apply(h: &Headend, wanted: Vec<ShowPlan>) {
    let _one = h.applying.lock().await;
    let running: BTreeMap<String, ShowPlan> = h.running.lock().iter().map(|(n, a)| (n.clone(), a.plan.clone())).collect();
    let d = diff(&running, &wanted);
    if d.is_empty() {
        return;
    }
    info!(add = d.add.len(), remove = d.remove.len(), restart = d.restart.len(), toggle = d.toggle.len(), retitle = d.retitle.len(), "headend: applying the config");
    let gone: Vec<String> = d.remove.iter().cloned().chain(d.restart.iter().map(|p| p.name.clone())).collect();
    for name in gone {
        remove(h, &name).await;
    }
    let mut make = d.add;
    make.extend(d.restart);
    add(h, make).await;
    for p in d.toggle {
        toggle(h, &p).await;
    }
    for p in d.retitle {
        retitle(h, &p).await;
    }
    saved::save(&h.st, &h.running.lock());
}

async fn remove(h: &Headend, name: &str) {
    let Some(a) = h.running.lock().remove(name) else { return };
    if let Err(e) = call(h, "show.remove", json!({"id": a.id})).await {
        warn!(stream = name, show = %a.id, error = %e, "headend: a stream gone from the config could not be removed");
    }
}

async fn add(h: &Headend, plans: Vec<ShowPlan>) {
    if plans.is_empty() {
        return;
    }
    let shows: Vec<Value> = plans.iter().map(|p| json!({"name": p.name, "compositing": false, "input": p.input, "outputs": p.outputs})).collect();
    let answer = match call(h, "show.add_many", json!({"dry_run": false, "shows": shows})).await {
        Ok(v) => v,
        Err(e) => return warn!(error = %e, "headend: show.add_many refused the whole batch; it is tried again in 30 s"),
    };
    let refused: BTreeMap<usize, String> = answer["refused"].as_array().into_iter().flatten().map(|r| (r["index"].as_u64().unwrap_or(0) as usize, r["why"].as_str().unwrap_or_default().to_string())).collect();
    let mut added = answer["added"].as_array().cloned().unwrap_or_default().into_iter();
    for (i, p) in plans.into_iter().enumerate() {
        if let Some(why) = refused.get(&i) {
            warn!(stream = %p.name, why = %why, "headend: a stream could not be made; it is tried again in 30 s");
            continue;
        }
        let Some(id) = added.next().and_then(|v| v.as_str().map(String::from)) else { continue };
        info!(stream = %p.name, show = %id, copy = p.copy, "headend: stream running as a show");
        let name = p.name.clone();
        h.running.lock().insert(name, Applied { id, plan: ShowPlan { disabled: false, title: None, ..p.clone() } });
        if p.disabled {
            toggle(h, &p).await;
        }
        if p.title.is_some() {
            retitle(h, &p).await;
        }
    }
}

async fn toggle(h: &Headend, p: &ShowPlan) {
    let Some(id) = h.running.lock().get(&p.name).map(|a| a.id.clone()) else { return };
    let method = if p.disabled { "show.stop" } else { "show.start" };
    match call(h, method, json!({"id": id})).await {
        Ok(_) => set(h, &p.name, |a| a.plan.disabled = p.disabled),
        Err(e) => warn!(stream = %p.name, error = %e, "headend: {method} refused"),
    }
}

async fn retitle(h: &Headend, p: &ShowPlan) {
    let Some(id) = h.running.lock().get(&p.name).map(|a| a.id.clone()) else { return };
    let title = p.title.clone().unwrap_or_else(|| p.name.clone());
    match call(h, "show.rename", json!({"id": id, "name": title})).await {
        Ok(_) => set(h, &p.name, |a| a.plan.title = p.title.clone()),
        Err(e) => warn!(stream = %p.name, error = %e, "headend: show.rename refused"),
    }
}

fn set(h: &Headend, name: &str, f: impl FnOnce(&mut Applied)) {
    if let Some(a) = h.running.lock().get_mut(name) {
        f(a);
    }
}
