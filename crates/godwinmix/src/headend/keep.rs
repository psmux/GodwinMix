//! Making shows match a list of plans, through the station's own methods:
//! a new plan is `show.add_many`, a changed one is removed and made again,
//! `disabled` is `show.stop` or `show.start`, a title is `show.rename`.
//! Nothing that did not change is touched.
//!
//! Two keepers run on a headend: one for the streams of a followed
//! Flussonic file, one for the channels the station keeps itself. Each
//! holds its own plans, so neither removes the other's shows.

use super::saved::{self, Applied, Running};
use crate::station::methods;
use crate::station::state::Station;
use godwinmix_flussonic::diff::diff;
use godwinmix_flussonic::import::ShowPlan;
use godwinmix_protocol::scope::Token;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Weak};
use tracing::{info, warn};

pub struct Keeper {
    st: Weak<Station>,
    token: Token,
    file: &'static str,
    pub running: Mutex<Running>,
    /// One apply at a time.
    applying: tokio::sync::Mutex<()>,
}

/// What one apply did, by name.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Outcome {
    pub added: Vec<String>,
    pub restarted: Vec<String>,
    pub removed: Vec<String>,
    /// Plans the station would not make, with its reason.
    pub refused: Vec<(String, String)>,
}

impl Keeper {
    /// A keeper with what `file` kept, less shows that are gone.
    pub fn new(st: &Arc<Station>, file: &'static str) -> Keeper {
        Keeper { st: Arc::downgrade(st), token: Token::open(), file, running: Mutex::new(saved::load(st, file)), applying: tokio::sync::Mutex::new(()) }
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let Some(st) = self.st.upgrade() else { return Err("the station has stopped".into()) };
        methods::call(&st, &self.token, method, params).await.map_err(|e| e.message)
    }

    pub fn save(&self) {
        if let Some(st) = self.st.upgrade() {
            saved::save(&st, self.file, &self.running.lock());
        }
    }

    pub fn id_of(&self, name: &str) -> Option<String> {
        self.running.lock().get(name).map(|a| a.id.clone())
    }

    /// Hand a running show to another keeper, which then owns it.
    pub fn take(&self, name: &str) -> Option<Applied> {
        let a = self.running.lock().remove(name);
        self.save();
        a
    }

    pub fn give(&self, name: &str, a: Applied) {
        self.running.lock().insert(name.to_string(), a);
        self.save();
    }

    /// The shows that run, wanted or not.
    pub async fn apply(&self, wanted: Vec<ShowPlan>) -> Outcome {
        let _one = self.applying.lock().await;
        let running: BTreeMap<String, ShowPlan> = self.running.lock().iter().map(|(n, a)| (n.clone(), a.plan.clone())).collect();
        let d = diff(&running, &wanted);
        let mut out = Outcome::default();
        if d.is_empty() {
            return out;
        }
        info!(file = self.file, add = d.add.len(), remove = d.remove.len(), restart = d.restart.len(), toggle = d.toggle.len(), retitle = d.retitle.len(), "headend: applying");
        for name in &d.remove {
            self.remove(name).await;
        }
        for p in &d.restart {
            self.remove(&p.name).await;
        }
        out.removed = d.remove.clone();
        out.restarted = d.restart.iter().map(|p| p.name.clone()).collect();
        let mut make = d.add;
        make.extend(d.restart);
        let (added, refused) = self.add(make).await;
        out.added = added.into_iter().filter(|n| !out.restarted.contains(n)).collect();
        out.refused = refused;
        for p in d.toggle {
            self.toggle(&p).await;
        }
        for p in d.retitle {
            self.retitle(&p).await;
        }
        self.save();
        out
    }

    async fn remove(&self, name: &str) {
        let Some(a) = self.running.lock().remove(name) else { return };
        if let Err(e) = self.call("show.remove", json!({"id": a.id})).await {
            warn!(stream = name, show = %a.id, error = %e, "headend: a channel's show could not be removed");
        }
    }

    async fn add(&self, plans: Vec<ShowPlan>) -> (Vec<String>, Vec<(String, String)>) {
        if plans.is_empty() {
            return Default::default();
        }
        let shows: Vec<Value> = plans.iter().map(show_of).collect();
        let answer = match self.call("show.add_many", json!({"dry_run": false, "shows": shows})).await {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "headend: show.add_many refused the whole batch; it is tried again in 30 s");
                return (Vec::new(), plans.into_iter().map(|p| (p.name, e.clone())).collect());
            }
        };
        let refused: BTreeMap<usize, String> = answer["refused"].as_array().into_iter().flatten().map(|r| (r["index"].as_u64().unwrap_or(0) as usize, r["why"].as_str().unwrap_or_default().to_string())).collect();
        let mut ids = answer["added"].as_array().cloned().unwrap_or_default().into_iter();
        let (mut made, mut no) = (Vec::new(), Vec::new());
        for (i, p) in plans.into_iter().enumerate() {
            if let Some(why) = refused.get(&i) {
                warn!(stream = %p.name, why = %why, "headend: a channel could not be made; it is tried again in 30 s");
                no.push((p.name.clone(), why.clone()));
                continue;
            }
            let Some(id) = ids.next().and_then(|v| v.as_str().map(String::from)) else { continue };
            info!(stream = %p.name, show = %id, copy = p.copy, "headend: channel running as a show");
            self.running.lock().insert(p.name.clone(), Applied { id, plan: ShowPlan { disabled: false, title: None, ..p.clone() } });
            made.push(p.name.clone());
            if p.disabled {
                self.toggle(&p).await;
            }
            if p.title.is_some() {
                self.retitle(&p).await;
            }
        }
        (made, no)
    }

    async fn toggle(&self, p: &ShowPlan) {
        let Some(id) = self.id_of(&p.name) else { return };
        let method = if p.disabled { "show.stop" } else { "show.start" };
        match self.call(method, json!({"id": id})).await {
            Ok(_) => self.set(&p.name, |a| a.plan.disabled = p.disabled),
            Err(e) => warn!(stream = %p.name, error = %e, "headend: {method} refused"),
        }
    }

    async fn retitle(&self, p: &ShowPlan) {
        let Some(id) = self.id_of(&p.name) else { return };
        let title = p.title.clone().unwrap_or_else(|| p.name.clone());
        match self.call("show.rename", json!({"id": id, "name": title})).await {
            Ok(_) => self.set(&p.name, |a| a.plan.title = p.title.clone()),
            Err(e) => warn!(stream = %p.name, error = %e, "headend: show.rename refused"),
        }
    }

    fn set(&self, name: &str, f: impl FnOnce(&mut Applied)) {
        if let Some(a) = self.running.lock().get_mut(name) {
            f(a);
        }
    }
}

/// A plan as `show.add_many` takes one show.
pub fn show_of(p: &ShowPlan) -> Value {
    json!({"name": p.name, "compositing": false, "input": p.input, "outputs": p.outputs})
}
