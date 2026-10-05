//! What `gallery.save` was given, turned into a draft: an SVG, a page, bytes
//! as base64, a path on the mixer, a ticker or text, or a set's pictures.

use super::super::entry;
use crate::control::call::Call;
use base64::Engine;
use godwinmix_core::gallery::draft::{self, Draft, Refusal};
use godwinmix_core::gallery::manifest::{to_toml_table, SetToml};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::gallery::{GalleryKind, GallerySaveRequest, SetSpec};
use serde_json::Value;

/// The refusal a caller acts on: the reason, then what to do, in `data.fix`.
pub(crate) fn refused(r: Refusal) -> RpcError {
    RpcError::invalid_params(format!("{r}. Nothing was saved.")).with("fix", r.fix)
}

/// The one thing to save, as a draft.
pub(super) async fn draft_of(call: &Call, req: &GallerySaveRequest) -> Result<Draft, RpcError> {
    let given: Vec<&str> = [
        ("svg", req.svg.is_some()),
        ("html", req.html.is_some()),
        ("data", req.data.is_some()),
        ("file", req.file.is_some()),
        ("source", req.source.is_some()),
        ("set", req.set.is_some()),
    ]
    .into_iter()
    .filter_map(|(k, on)| on.then_some(k))
    .collect();
    match given.as_slice() {
        [] => Err(RpcError::invalid_params(
            "save_graphic needs the graphic: give `svg` (an SVG document), `html` (a web page), `data` (a picture or clip as base64), \
             `file` (a path on the mixer), `source` (a ticker or text) or `set` (a virtual set). Nothing was saved.",
        )
        .with("fields", serde_json::json!(["svg", "html", "data", "file", "source", "set"]))),
        [_] => one(call, req).await,
        many => Err(RpcError::invalid_params(format!(
            "save_graphic takes one graphic at a time and was given {}. Send one of them; save the others with their own call. Nothing was saved.",
            many.join(" and ")
        ))),
    }
}

async fn one(call: &Call, req: &GallerySaveRequest) -> Result<Draft, RpcError> {
    if let Some(svg) = &req.svg {
        return draft::svg(svg).map_err(refused);
    }
    if let Some(html) = &req.html {
        let mut extra = Vec::new();
        for (name, v) in req.files.iter().flatten() {
            let text = v.as_str().ok_or_else(|| RpcError::invalid_params(format!("files.{name} is text or a data: URI")))?;
            extra.push((name.clone(), if text.starts_with("data:") { decode(text)?.1 } else { text.as_bytes().to_vec() }));
        }
        return draft::html(html, extra).map_err(refused);
    }
    if let Some(data) = &req.data {
        let (ext, bytes) = decode(data)?;
        let name = req.filename.clone().unwrap_or_else(|| format!("upload.{}", ext.unwrap_or("bin")));
        return draft::bytes(&name, bytes).map_err(refused);
    }
    if let Some(file) = &req.file {
        return from_file(call, file).await;
    }
    if let Some(source) = &req.source {
        let uri = source.get("uri").and_then(Value::as_str).unwrap_or_default();
        let params = source.get("params").and_then(Value::as_object).cloned().unwrap_or_default();
        return draft::source(uri, to_toml_table(&params)).map_err(refused);
    }
    set(call, req.set.as_ref().expect("one was given")).await
}

/// A path on the mixer, or a file in its media library.
async fn from_file(call: &Call, file: &str) -> Result<Draft, RpcError> {
    let path = call.app.library.resolve(file).unwrap_or_else(|_| std::path::PathBuf::from(file.trim()));
    let shown = file.to_string();
    let mut found = super::blocking("reading the file", move || godwinmix_core::gallery::bundle::read_path(&path)).await?;
    match found.len() {
        1 => found.remove(0).1.map_err(refused),
        n => Err(RpcError::invalid_params(format!(
            "{shown} holds {n} graphics. Import them all with import_graphics {{\"path\": {shown:?}}}, or give the path of one. Nothing was saved."
        ))),
    }
}

/// Base64, or a `data:` URI, as bytes, with the extension its type names.
pub(crate) fn decode(text: &str) -> Result<(Option<&'static str>, Vec<u8>), RpcError> {
    let (head, b64) = match text.trim().strip_prefix("data:") {
        Some(rest) => rest.split_once(',').map(|(h, d)| (h.to_ascii_lowercase(), d)).unwrap_or_default(),
        None => (String::new(), text.trim()),
    };
    let ext = ["png", "jpeg", "webp", "gif", "svg", "webm", "quicktime", "mp4", "html", "zip"]
        .into_iter()
        .find(|t| head.contains(t))
        .map(|t| match t {
            "jpeg" => "jpg",
            "quicktime" => "mov",
            t => t,
        });
    let clean: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
    // With or without its padding, and in either alphabet: models write all four.
    use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
    let loose = GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent);
    let bytes = GeneralPurpose::new(&base64::alphabet::STANDARD, loose)
        .decode(&clean)
        .or_else(|_| GeneralPurpose::new(&base64::alphabet::URL_SAFE, loose).decode(&clean))
        .map_err(|e| RpcError::invalid_params(format!("data is not base64 ({e}). Send the file's bytes as base64, or as a data: URI such as data:image/png;base64,iVBOR... Nothing was saved.")))?;
    Ok((ext, bytes))
}

/// A virtual set: its pictures copied in beside its settings.
async fn set(call: &Call, spec: &SetSpec) -> Result<Draft, RpcError> {
    let mut files = Vec::new();
    let background = picture(call, &spec.background, "plate", &mut files).await.map_err(|e| e.with("field", "set.background"))?;
    let foreground = match spec.foreground.as_deref().filter(|f| !f.trim().is_empty()) {
        Some(f) => Some(picture(call, f, "front", &mut files).await.map_err(|e| e.with("field", "set.foreground"))?),
        None => None,
    };
    let mut d = Draft { files, ..Default::default() };
    d.manifest.kind = GalleryKind::Set.as_str().into();
    d.manifest.set = Some(SetToml { background, foreground, layout: spec.layout.clone(), settings: to_toml_table(&spec.settings) });
    Ok(d)
}

/// One picture for a set, from a gallery id, a media file, a path or a
/// `data:` URI, added to `files` as `<stem>.<ext>`.
async fn picture(call: &Call, named: &str, stem: &str, files: &mut Vec<(String, Vec<u8>)>) -> Result<String, RpcError> {
    let named = named.trim();
    let bytes = if named.starts_with("data:") {
        decode(named)?.1
    } else if let Some(path) = call.app.library.resolve(named).ok().or_else(|| Some(std::path::PathBuf::from(named)).filter(|p| p.is_file())) {
        std::fs::read(&path).map_err(|e| RpcError::invalid_params(format!("{} could not be read: {e}", path.display())))?
    } else {
        let e = entry(named).await?;
        let file = e.file().filter(|f| f.is_file()).ok_or_else(|| RpcError::invalid_params(format!("{named} is a {} with no picture file; give a picture", e.item.kind.as_str())))?;
        std::fs::read(&file).map_err(|err| RpcError::internal(format!("reading {}: {err}", file.display())))?
    };
    let sniffed = godwinmix_core::gallery::detect::sniff(&bytes, named)
        .filter(|s| matches!(s.kind(), Some(GalleryKind::Image | GalleryKind::Clip)) || s.ext() == "svg")
        .ok_or_else(|| RpcError::invalid_params(format!("{named} is not a picture. A set takes a PNG, JPEG, WebP or SVG, or a WebM or MOV clip, for each layer. Nothing was saved.")))?;
    let name = format!("{stem}.{}", sniffed.ext());
    files.push((name.clone(), bytes));
    Ok(name)
}
