//! What `scene.create_from` is given, turned into what a layout takes: a
//! source id for each picture, and a key colour.

use crate::control::call::Call;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::requests::AddSourceRequest;

/// A source id for what the caller named: the source itself, or a source
/// made for a media file or a path. A file already on the desk as a source
/// is not added twice.
pub async fn source_for(
    call: &Call,
    known: &[String],
    named: &str,
    field: &str,
    added: &mut Vec<String>,
) -> Result<String, RpcError> {
    let named = named.trim();
    if known.iter().any(|k| k == named) {
        return Ok(named.to_string());
    }
    let uri = match call.app.library.resolve(named) {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(_) if named.contains("://") || std::path::Path::new(named).is_file() => named.to_string(),
        Err(_) => {
            return Err(RpcError::not_found(field, named, known)
                .with("field", field)
                .with("next", "Give a source id from source.list, a file name from media.list, \
                               or upload the picture with media.upload first."))
        }
    };
    // The configs rather than the status: a listing shortens a path.
    let configs = call.app.mixer.configs().await.map_err(|e| call.mixer_error(e))?;
    if let Some(same) = configs.sources.iter().find(|s| same_file(&s.uri, &uri)) {
        return Ok(same.id.clone());
    }
    let stem = std::path::Path::new(&uri).file_stem().map(|s| s.to_string_lossy().into_owned());
    let req = AddSourceRequest { id: None, name: stem, uri, kind: None, superimpose: None, params: Default::default() };
    let id = crate::control::add_source_now(&call.app, req).await.map_err(|e| call.mixer_error(e))?;
    added.push(id.clone());
    Ok(id)
}

/// The key colour to write, and where it came from.
pub async fn key_for(call: &Call, presenter: &str, asked: Option<&str>) -> Result<(String, &'static str), RpcError> {
    match asked.map(str::trim).filter(|k| !k.is_empty() && !k.eq_ignore_ascii_case("auto")) {
        Some(hex) => match godwinmix_core::plugin::filters::chroma::parse_hex(hex) {
            Some(rgb) => Ok((godwinmix_core::plugin::filters::chroma::to_hex(rgb), "given")),
            None => Err(RpcError::invalid_params(format!(
                "key is {hex:?}; give \"auto\" or a colour like \"#30b050\"."
            ))
            .with("field", "key")),
        },
        None => Ok(match super::key_color::guess(call, presenter).await {
            Some(found) => (found.color, "guessed"),
            None => ("auto".to_string(), "auto"),
        }),
    }
}

/// The presenter's size and place, each inside its range.
pub fn check_ranges(scale: Option<f64>, x: Option<f64>) -> Result<(), RpcError> {
    if let Some(s) = scale.filter(|s| !(0.3..=1.0).contains(s)) {
        return Err(RpcError::invalid_params(format!(
            "presenter_scale is {s}; it is the presenter's size as a fraction of the canvas, from 0.3 to 1."
        ))
        .with("field", "presenter_scale"));
    }
    if let Some(x) = x.filter(|x| !(0.0..=1.0).contains(x)) {
        return Err(RpcError::invalid_params(format!(
            "presenter_x is {x}; it is where the presenter stands, 0 at the left to 1 at the right."
        ))
        .with("field", "presenter_x"));
    }
    Ok(())
}

/// Two addresses for one file: equal, or the same file once a `file://` is
/// taken off and the paths are made canonical.
fn same_file(a: &str, b: &str) -> bool {
    let real = |s: &str| std::fs::canonicalize(s.strip_prefix("file://").unwrap_or(s)).ok();
    a == b || real(a).is_some_and(|p| Some(p) == real(b))
}
