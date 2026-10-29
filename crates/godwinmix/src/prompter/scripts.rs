//! `prompter.script.*`: the scripts kept on the station.

use super::{check, now_ms, Prompter};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::prompter::{
    markers, rows, RowsAnswer, Script, ScriptChanged, ScriptList, ScriptSaveRequest, ScriptSummary,
};
use godwinmix_protocol::types::Event;

fn summary(script: &Script) -> ScriptSummary {
    let found = markers(&rows(&script.text, 1000));
    ScriptSummary {
        id: script.id.clone(),
        title: script.title.clone(),
        lines: script.text.split('\n').count() as u32,
        markers: found.into_iter().map(|m| m.name).collect(),
        bytes: script.text.len() as u32,
        updated_ms: script.updated_ms,
    }
}

impl Prompter {
    pub(super) fn ids(&self) -> Vec<String> {
        self.inner.lock().scripts.keys().cloned().collect()
    }

    pub fn list(&self) -> ScriptList {
        let inner = self.inner.lock();
        ScriptList { scripts: inner.scripts.values().map(summary).collect(), on: inner.on.clone() }
    }

    pub fn script(&self, id: &str) -> Result<Script, RpcError> {
        let found = self.inner.lock().scripts.get(id).cloned();
        found.ok_or_else(|| check::no_script(id, self.ids()))
    }

    /// The script on the prompter as rows, at the width every screen uses.
    pub fn rows(&self) -> RowsAnswer {
        let inner = self.inner.lock();
        let updated_ms = inner.on.as_ref().and_then(|id| inner.scripts.get(id)).map_or(0, |s| s.updated_ms);
        RowsAnswer { script: inner.on.clone(), updated_ms, line_chars: inner.look.line_chars, rows: inner.rows.clone() }
    }

    /// Make a script or replace one. Saving the one on the prompter changes
    /// the words on every screen at once and keeps the place.
    pub fn save(&self, req: ScriptSaveRequest) -> Result<Script, RpcError> {
        let id = match req.id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(id) => id.to_string(),
            None => crate::channels::keys::slug(&req.title),
        };
        check::id(&id)?;
        check::title_and_text(&req.title, &req.text)?;
        let script = Script { id: id.clone(), title: req.title.trim().to_string(), text: req.text, updated_ms: now_ms() };
        let (on_air, _) = self.change(true, |inner| {
            inner.scripts.insert(id.clone(), script.clone());
            let on_air = inner.on.as_deref() == Some(id.as_str());
            if on_air {
                inner.rebuild();
            }
            on_air
        });
        tracing::info!(script = %id, on_air, "a prompter script was saved");
        self.told(&script.id, false, script.updated_ms);
        Ok(script)
    }

    /// Forget a script. When it is the one on the prompter, the prompter is
    /// left empty and every screen says so.
    pub fn remove(&self, id: &str) -> Result<ScriptChanged, RpcError> {
        self.script(id)?;
        self.change(true, |inner| {
            inner.scripts.remove(id);
            if inner.on.as_deref() == Some(id) {
                inner.on = None;
                inner.playing = false;
                inner.position = 0.0;
                inner.rebuild();
            }
        });
        let changed = ScriptChanged { id: id.to_string(), removed: true, updated_ms: now_ms() };
        self.told(id, true, changed.updated_ms);
        Ok(changed)
    }

    fn told(&self, id: &str, removed: bool, updated_ms: u64) {
        let changed = ScriptChanged { id: id.to_string(), removed, updated_ms };
        self.bus.emit(Event::PrompterScriptChanged { changed });
    }
}
