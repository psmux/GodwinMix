//! `gallery.list`: every item, or the ones some words find.

use super::{blocking, dir, placed};
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_core::gallery::{self, Entry};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::gallery::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("gallery.list", Scope::Read, "The Graphics gallery: every lower third, background, ticker, bug, title card, page, clip and virtual set, made here or shipped, with what each is and where it goes.", handler(list))
            .params(schema_of::<GalleryListRequest>)
            .result(schema_of::<GalleryList>)
            .tool(
                "list_graphics",
                Tier::Search,
                "List the graphics gallery: lower thirds, backgrounds, tickers, bugs, title cards, HTML \
                 graphics, clips and virtual sets. Give `query` words such as \"red lower third\" to search. \
                 Each item has an `id` for preview_graphic and place_graphic, its kind, zone, fields and \
                 whether it moves or is transparent.",
            ),
    );
}

async fn list(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GalleryListRequest = if params.is_null() { Default::default() } else { call.params(&params)? };
    let kind = match req.kind.as_deref().map(str::trim).filter(|k| !k.is_empty() && *k != "all") {
        Some(k) => Some(GalleryKind::parse(k).ok_or_else(|| {
            RpcError::invalid_params(format!(
                "kind {k:?} is not one the gallery has. Use one of template, image, clip, html, ograf, ticker, text, set, transition, effect, or leave it out."
            ))
            .with("field", "kind")
        })?),
        None => None,
    };
    let (mut entries, errors) = blocking("reading the gallery", || gallery::store::list(&dir())).await?;
    entries.retain(|e| kind.is_none_or(|k| e.item.kind == k));
    if let Some(q) = req.query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        rank(&mut entries, q);
    }
    let total = entries.len();
    entries.truncate(req.limit.unwrap_or(50).clamp(1, 500));
    placed(&call, &mut entries).await;
    body(GalleryList { items: entries.into_iter().map(|e| e.item).collect(), dir: dir().display().to_string(), total, errors })
}

/// Keep the items any word finds, the best first. A word scores more in
/// the name and the kind than in the tags, and more in the tags than in the
/// description.
fn rank(entries: &mut Vec<Entry>, query: &str) {
    let words: Vec<String> = query.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 1).map(synonym).collect();
    let mut scored: Vec<(usize, Entry)> = entries.drain(..).map(|e| (score(&e, &words), e)).filter(|(s, _)| *s > 0).collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    entries.extend(scored.into_iter().map(|(_, e)| e));
}

/// What people say for what the gallery calls it.
fn synonym(w: &str) -> String {
    match w {
        "l3" | "strap" | "straps" | "lower" | "third" | "thirds" => "lower-third",
        "bg" | "backdrop" | "backgrounds" | "plate" => "background",
        "crawl" | "scroller" | "tickers" => "ticker",
        "logo" | "corner" | "bugs" | "watermark" => "bug",
        "card" | "cards" | "title" | "titles" => "title",
        "studio" | "sets" | "virtual" => "set",
        other => other,
    }
    .to_string()
}

fn score(e: &Entry, words: &[String]) -> usize {
    let i = &e.item;
    let name = i.name.to_lowercase();
    let head = format!("{} {} {} {}", i.id, name, i.kind.as_str(), i.zone.as_str());
    let tags = i.tags.join(" ").to_lowercase();
    let full = matches!(i.zone, Zone::Full).then_some("background").unwrap_or_default();
    let desc = format!("{} {full}", i.description.to_lowercase());
    words.iter().map(|w| if head.contains(w.as_str()) { 4 } else if tags.contains(w.as_str()) { 2 } else if desc.contains(w.as_str()) { 1 } else { 0 }).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_words_find_the_pack_by_what_people_call_it() {
        let (mut all, _) = gallery::store::list(&std::env::temp_dir().join("gmx-gallery-rank"));
        rank(&mut all, "lower third");
        assert_eq!(all.first().map(|e| e.item.id.as_str()), Some("news-lower-third"));
        let (mut all, _) = gallery::store::list(&std::env::temp_dir().join("gmx-gallery-rank"));
        rank(&mut all, "a blue background");
        assert_eq!(all.first().map(|e| e.item.id.as_str()), Some("blue-gradient"));
    }
}
