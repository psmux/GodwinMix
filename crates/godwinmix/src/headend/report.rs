//! `headend.import`'s report: the import's plans and notes in the
//! protocol's words, with any password in an address hidden, because the
//! report is drawn on a page and read by agents.

use godwinmix_flussonic::import::{self, redact, NoteKind, ShowPlan};
use godwinmix_protocol::headend::{HeadendImportReport, ImportNote, ImportedStream};
use godwinmix_protocol::shows::ShowAddManyResult;
use serde_json::Value;

pub fn report(source: String, dry_run: bool, made: import::Import, shows: ShowAddManyResult, following: Option<String>) -> HeadendImportReport {
    let copied = made.shows.iter().filter(|p| p.copy).count();
    let notes = made.notes.into_iter().map(|(scope, n)| ImportNote {
        scope,
        line: n.line,
        kind: match n.kind { NoteKind::Ignored => "ignored", NoteKind::NotCarried => "not_carried" }.into(),
        what: n.what,
    });
    HeadendImportReport {
        source,
        dry_run,
        converted: made.shows.len() - copied,
        copied,
        disabled: made.shows.iter().filter(|p| p.disabled).count(),
        streams: made.shows.into_iter().map(stream).collect(),
        notes: notes.collect(),
        shows,
        following,
    }
}

fn stream(p: ShowPlan) -> ImportedStream {
    ImportedStream { name: p.name, title: p.title, disabled: p.disabled, copy: p.copy, input: hidden(p.input), outputs: p.outputs.into_iter().map(hidden).collect() }
}

/// An input or an output with any password in an address as `***`.
fn hidden(mut v: Value) -> Value {
    for at in ["/uri", "/backup/uri"] {
        if let Some(node) = v.pointer_mut(at) {
            if let Some(safe) = node.as_str().map(redact) {
                *node = Value::String(safe);
            }
        }
    }
    v
}

#[cfg(test)]
mod tests;
