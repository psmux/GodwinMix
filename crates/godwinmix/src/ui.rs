//! The reference web UI, and the files a plugin adds to it.
//!
//! The UI is a directory of plain ES modules, custom elements and CSS: no
//! framework, no build step, no npm. Every file is compiled into the binary as
//! a string behind the table below, so `godwinmix` stays one file you can copy
//! onto a machine. The whole set is about 230 kB uncompressed and there is not
//! one external request in it, which is what makes the page usable on a Pi and
//! over a phone's connection.
//!
//! Three things live here rather than in `control.rs`:
//!
//! * the static routes, one per file, so no request can ask for a path that is
//!   not in the table and directory traversal is impossible by construction;
//! * a real Content Security Policy, which is only worth setting because there
//!   is no inline script left in the page to need an exception;
//! * `/plugins/<name>/ui/*`, read from disk at request time, which is what the
//!   long unread `control.ui_dir` field was pointing at.

use axum::body::Body;
use axum::extract::Path as UrlPath;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde_json::json;
use std::path::{Component, PathBuf};
use std::sync::OnceLock;

/// Every file under `ui/`, except `ui/legacy/`, which has its own route.
///
/// Written out one by one rather than pulled in with a directory macro: the
/// list is the manifest, a reviewer can see exactly what ships, and a stray
/// file left in the tree does not silently become a public URL.
const ASSETS: &[(&str, &str)] = &[
    ("boot.js", include_str!("../../../ui/boot.js")),
    ("client/errors.js", include_str!("../../../ui/client/errors.js")),
    ("client/frames.js", include_str!("../../../ui/client/frames.js")),
    ("client/index.js", include_str!("../../../ui/client/index.js")),
    ("client/kinds.js", include_str!("../../../ui/client/kinds.js")),
    ("client/rpc.js", include_str!("../../../ui/client/rpc.js")),
    ("client/sandbox-client.js", include_str!("../../../ui/client/sandbox-client.js")),
    ("client/schema-form.js", include_str!("../../../ui/client/schema-form.js")),
    ("client/store.js", include_str!("../../../ui/client/store.js")),
    ("client/transport-legacy.js", include_str!("../../../ui/client/transport-legacy.js")),
    ("client/transport-rpc.js", include_str!("../../../ui/client/transport-rpc.js")),
    ("index.html", include_str!("../../../ui/index.html")),
    ("kits/canvas/draw.js", include_str!("../../../ui/kits/canvas/draw.js")),
    ("kits/canvas/geometry.js", include_str!("../../../ui/kits/canvas/geometry.js")),
    ("kits/canvas/gizmos.js", include_str!("../../../ui/kits/canvas/gizmos.js")),
    ("kits/canvas/safe.js", include_str!("../../../ui/kits/canvas/safe.js")),
    ("kits/canvas/snap.js", include_str!("../../../ui/kits/canvas/snap.js")),
    ("kits/protocol/index.js", include_str!("../../../ui/kits/protocol/index.js")),
    ("kits/protocol/mirror.js", include_str!("../../../ui/kits/protocol/mirror.js")),
    ("kits/protocol/predict.js", include_str!("../../../ui/kits/protocol/predict.js")),
    ("kits/protocol/timing.js", include_str!("../../../ui/kits/protocol/timing.js")),
    ("kits/protocol/undo.js", include_str!("../../../ui/kits/protocol/undo.js")),
    ("kits/schema/describe.js", include_str!("../../../ui/kits/schema/describe.js")),
    ("kits/schema/index.js", include_str!("../../../ui/kits/schema/index.js")),
    ("kits/schema/registry.js", include_str!("../../../ui/kits/schema/registry.js")),
    ("kits/schema/render.js", include_str!("../../../ui/kits/schema/render.js")),
    ("kits/schema/ui-schema.js", include_str!("../../../ui/kits/schema/ui-schema.js")),
    ("panels/alerts/panel.js", include_str!("../../../ui/panels/alerts/panel.js")),
    ("panels/composer/canvas.js", include_str!("../../../ui/panels/composer/canvas.js")),
    ("panels/composer/catalogue.js", include_str!("../../../ui/panels/composer/catalogue.js")),
    ("panels/composer/composer.css", include_str!("../../../ui/panels/composer/composer.css")),
    ("panels/composer/composer.js", include_str!("../../../ui/panels/composer/composer.js")),
    ("panels/composer/inspector.js", include_str!("../../../ui/panels/composer/inspector.js")),
    ("panels/composer/ops.js", include_str!("../../../ui/panels/composer/ops.js")),
    ("panels/header/panel.js", include_str!("../../../ui/panels/header/panel.js")),
    ("panels/media/panel.js", include_str!("../../../ui/panels/media/panel.js")),
    ("panels/multiview/panel.js", include_str!("../../../ui/panels/multiview/panel.js")),
    ("panels/outputs/panel.js", include_str!("../../../ui/panels/outputs/panel.js")),
    ("panels/scenes/more.js", include_str!("../../../ui/panels/scenes/more.js")),
    ("panels/scenes/panel.js", include_str!("../../../ui/panels/scenes/panel.js")),
    ("panels/sources/local.js", include_str!("../../../ui/panels/sources/local.js")),
    ("panels/sources/panel.js", include_str!("../../../ui/panels/sources/panel.js")),
    ("panels/sources/tile.js", include_str!("../../../ui/panels/sources/tile.js")),
    ("panels/welcome/defaults.js", include_str!("../../../ui/panels/welcome/defaults.js")),
    ("panels/welcome/panel.js", include_str!("../../../ui/panels/welcome/panel.js")),
    ("panels/welcome/tiles.js", include_str!("../../../ui/panels/welcome/tiles.js")),
    ("shell/commands.js", include_str!("../../../ui/shell/commands.js")),
    ("shell/dom.js", include_str!("../../../ui/shell/dom.js")),
    ("shell/fader.js", include_str!("../../../ui/shell/fader.js")),
    ("shell/firstrun.js", include_str!("../../../ui/shell/firstrun.js")),
    ("shell/keymap.js", include_str!("../../../ui/shell/keymap.js")),
    ("shell/layout.js", include_str!("../../../ui/shell/layout.js")),
    ("shell/menu.js", include_str!("../../../ui/shell/menu.js")),
    ("shell/meter.js", include_str!("../../../ui/shell/meter.js")),
    ("shell/modal.js", include_str!("../../../ui/shell/modal.js")),
    ("shell/palette.js", include_str!("../../../ui/shell/palette.js")),
    ("shell/picker.js", include_str!("../../../ui/shell/picker.js")),
    ("shell/pointer.js", include_str!("../../../ui/shell/pointer.js")),
    ("shell/registry.js", include_str!("../../../ui/shell/registry.js")),
    ("shell/sandbox.js", include_str!("../../../ui/shell/sandbox.js")),
    ("shell/selection.js", include_str!("../../../ui/shell/selection.js")),
    ("shell/settings.js", include_str!("../../../ui/shell/settings.js")),
    ("shell/shell.js", include_str!("../../../ui/shell/shell.js")),
    ("shell/theme.js", include_str!("../../../ui/shell/theme.js")),
    ("shell/toast.js", include_str!("../../../ui/shell/toast.js")),
    ("shell/undo.js", include_str!("../../../ui/shell/undo.js")),
    ("themes/base.css", include_str!("../../../ui/themes/base.css")),
    ("themes/dark.css", include_str!("../../../ui/themes/dark.css")),
    ("themes/high-contrast.css", include_str!("../../../ui/themes/high-contrast.css")),
    ("themes/light.css", include_str!("../../../ui/themes/light.css")),
    ("themes/system.css", include_str!("../../../ui/themes/system.css")),
];

/// The page as it was before the split, kept at `/legacy` for one release so an
/// operator mid show has something to fall back to.
const LEGACY: &str = include_str!("../../../ui/legacy/index.html");

/// The DOM harness, which is a development page and not part of the product.
///
/// It is 19 kB of assertions nobody running a show ever loads, and the served
/// set has a 250,000 byte budget it was eating eight percent of. `GMX_UI_DEV=1`
/// puts it back at `/test/`, which is what a developer running the harness
/// sets and what CI sets.
const DEV_ASSETS: &[(&str, &str)] = &[
    ("test/index.html", include_str!("../../../ui/test/index.html")),
    ("test/run.js", include_str!("../../../ui/test/run.js")),
    // The designer kits' behaviour, as the reference implementation answered
    // it. The TypeScript and Python suites read the same file from the
    // repository; the browser reads it from here, because the page has no file
    // system. Written by `node dev/kit-fixtures.mjs`.
    ("test/fixtures.json", include_str!("../../../ui/kits/fixtures.json")),
    // A plugin that ships a data schema and a designer block and no code, as
    // `plugin.describe` answers for it. The Python suite reads the same file,
    // so one fixture is the acceptance for both toolkits.
    ("test/designer-fixture.json", include_str!("../../../tests/fixtures/designer/lower-third.json")),
];

/// True when this process was started with `GMX_UI_DEV=1`.
fn dev_pages() -> bool {
    std::env::var("GMX_UI_DEV").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

/// Where `/plugins/<name>/ui/` is read from, and where the UI is read from when
/// an operator wants to edit it without rebuilding.
struct Dirs {
    ui: Option<PathBuf>,
    plugins: PathBuf,
}

static DIRS: OnceLock<Dirs> = OnceLock::new();

/// Called once at startup with the `[control]` section. Safe to skip: the
/// defaults are the embedded page and `~/.godwinmix/plugins`.
pub fn configure(ui_dir: Option<&str>, plugins_dir: Option<&str>) {
    let _ = DIRS.set(Dirs {
        ui: ui_dir.filter(|s| !s.trim().is_empty()).map(PathBuf::from),
        plugins: plugins_dir
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(default_plugins_dir),
    });
}

fn dirs() -> &'static Dirs {
    DIRS.get_or_init(|| Dirs { ui: None, plugins: default_plugins_dir() })
}

fn default_plugins_dir() -> PathBuf {
    // No `dirs` crate for one path. HOME is set on every platform this runs on,
    // and USERPROFILE covers a Windows service that does not set HOME.
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".godwinmix").join("plugins")
}

/// The whole UI, ready to `.merge()` into the control router.
///
/// Generic over the router's state because it needs none of its own: every
/// route here answers from the table or from disk, so it merges into whatever
/// state `control.rs` is carrying without knowing what that is.
pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let mut router = Router::new()
        .route("/", get(|| async { asset("index.html") }))
        .route("/legacy", get(|| async { html(LEGACY) }))
        .route("/plugins/index.json", get(plugin_index))
        .route("/plugins/{name}/ui/{*path}", get(plugin_file))
        // A preset's own theme, read out of the preset it was applied from, so
        // a theme travels with the preset and needs no rebuild.
        .route("/presets/{name}/theme.css", get(preset_theme));
    for (path, _) in ASSETS {
        let p = *path;
        router = router.route(&format!("/{p}"), get(move || async move { asset(p) }));
    }
    if dev_pages() {
        // A directory URL is what a person types, so it answers rather than 404s.
        router = router.route("/test/", get(|| async { dev_asset("test/index.html") }));
        for (path, _) in DEV_ASSETS {
            let p = *path;
            router = router.route(&format!("/{p}"), get(move || async move { dev_asset(p) }));
        }
    }
    router
}

/// One of the development pages. Only routed when `GMX_UI_DEV=1`.
fn dev_asset(path: &'static str) -> Response {
    let body = DEV_ASSETS.iter().find(|(p, _)| *p == path).map(|(_, b)| *b).unwrap_or("");
    with_headers(content_type(path), Body::from(body), path)
}

/// `/presets/<name>/theme.css`.
///
/// A preset names a theme its surface resolves, and a preset that ships its own
/// stylesheet has it at `theme_css` in the manifest. Nothing else in a preset is
/// served: this is a stylesheet route, not a file server.
async fn preset_theme(UrlPath(name): UrlPath<String>) -> Response {
    if !legible_name(&name) {
        return (StatusCode::NOT_FOUND, "no such preset").into_response();
    }
    let found = match godwinmix_core::preset::resolve(&name) {
        Ok(p) => p,
        Err(e) => return (StatusCode::NOT_FOUND, format!("{e:#}")).into_response(),
    };
    let css = found.block().ok().and_then(|b| b.theme_css.clone());
    let Some(css) = css else {
        return (
            StatusCode::NOT_FOUND,
            format!(
                "the preset '{name}' ships no stylesheet. Add `theme_css = \"theme.css\"` \
                 to its [provides.preset] table and put the file beside the manifest."
            ),
        )
            .into_response();
    };
    match found.read(&css) {
        Ok(body) => with_headers("text/css; charset=utf-8", Body::from(body), "theme.css"),
        Err(e) => (StatusCode::NOT_FOUND, format!("{e:#}")).into_response(),
    }
}

/// One embedded file, or the same file from `ui_dir` when one is configured.
fn asset(path: &'static str) -> Response {
    let kind = content_type(path);
    if let Some(dir) = dirs().ui.as_ref() {
        if let Ok(body) = std::fs::read(dir.join(path)) {
            return with_headers(kind, Body::from(body), path);
        }
    }
    let body = ASSETS
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, body)| *body)
        .unwrap_or("");
    with_headers(kind, Body::from(body), path)
}

fn html(body: &'static str) -> Response {
    with_headers("text/html; charset=utf-8", Body::from(body), "index.html")
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("woff2") => "font/woff2",
        Some("map") => "application/json",
        _ => "application/octet-stream",
    }
}

/// The policy.
///
/// `script-src 'self'` already covers `/plugins/`, because a plugin's files are
/// served from this same origin; a bare path is not a legal CSP source anyway.
/// `style-src` keeps `'unsafe-inline'` because the panels set style attributes
/// on elements they build, and a nonce cannot cover a style attribute. Scripts
/// get no such exception: there is no inline script in the page at all, which
/// is the half of the policy that stops an injected string from running.
const CSP: &str = concat!(
    "default-src 'self'; ",
    "script-src 'self'; ",
    "style-src 'self' 'unsafe-inline'; ",
    "img-src 'self' data: blob:; ",
    "media-src 'self' blob:; ",
    "font-src 'self' data:; ",
    "connect-src 'self' ws: wss:; ",
    "frame-src 'self'; ",
    "frame-ancestors 'self'; ",
    "base-uri 'none'; ",
    "form-action 'none'; ",
    "object-src 'none'",
);

fn with_headers(kind: &'static str, body: Body, path: &str) -> Response {
    let cache = if path.ends_with(".html") {
        // The page names its modules by plain path, so a stale page would load
        // stale modules. It is two kilobytes; revalidating it costs nothing.
        "no-cache"
    } else {
        "public, max-age=300"
    };
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(kind)),
            (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
            (header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP)),
            (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
            (header::REFERRER_POLICY, HeaderValue::from_static("no-referrer")),
        ],
        body,
    )
        .into_response()
}

// ------------------------------------------------------------------ plugins

/// What panels are on disk, read at request time so a file dropped into
/// `~/.godwinmix/plugins/x/ui/` appears on the next reload with no restart.
///
/// A plugin may put a `panel.json` beside its code to name itself and pick its
/// slot. Without one, `panel.js` is a trusted custom element and `panel.html`
/// is a sandboxed frame, which is the smallest thing that can possibly work.
async fn plugin_index() -> Response {
    let root = &dirs().plugins;
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !legible_name(&name) {
                continue;
            }
            let ui = entry.path().join("ui");
            let has_module = ui.join("panel.js").is_file();
            let has_page = ui.join("panel.html").is_file();
            if !has_module && !has_page {
                continue;
            }
            let manifest: serde_json::Value = std::fs::read_to_string(ui.join("panel.json"))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_else(|| json!({}));
            let tier = manifest
                .get("tier")
                .and_then(|v| v.as_str())
                .unwrap_or(if has_module { "trusted" } else { "sandboxed" });
            found.push(json!({
                "name": name,
                "id": manifest.get("id").and_then(|v| v.as_str()).unwrap_or(&format!("{name}/panel")).to_string(),
                "title": manifest.get("title").and_then(|v| v.as_str()).unwrap_or(&name).to_string(),
                "slots": manifest.get("slots").cloned().unwrap_or_else(|| json!(["sidebar"])),
                "height": manifest.get("height").cloned().unwrap_or_else(|| json!(260)),
                "tier": tier,
                "module": has_module.then(|| format!("/plugins/{name}/ui/panel.js")),
                "page": has_page.then(|| format!("/plugins/{name}/ui/panel.html")),
            }));
        }
    }
    found.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let body = json!({ "plugins": found, "dir": root.display().to_string() });
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("application/json")),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP)),
        ],
        body.to_string(),
    )
        .into_response()
}

/// One file out of one plugin's `ui/` directory.
async fn plugin_file(UrlPath((name, path)): UrlPath<(String, String)>) -> Response {
    if !legible_name(&name) {
        return (StatusCode::NOT_FOUND, "no such plugin").into_response();
    }
    let Some(relative) = safe_relative(&path) else {
        return (StatusCode::NOT_FOUND, "that path is not inside the plugin's ui directory").into_response();
    };
    let file = dirs().plugins.join(&name).join("ui").join(&relative);
    match std::fs::read(&file) {
        Ok(body) => with_headers(content_type(&path), Body::from(body), &path),
        Err(_) => (
            StatusCode::NOT_FOUND,
            format!("plugin '{name}' has no ui/{path}. Put the file there and reload the page."),
        )
            .into_response(),
    }
}

/// Plugin names are slugs (principle 5), which also rules out `..` and `/`.
fn legible_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// A path with no `..`, no root and no absolute prefix. Anything else is a no.
fn safe_relative(path: &str) -> Option<PathBuf> {
    if path.is_empty() || path.len() > 256 {
        return None;
    }
    let candidate = PathBuf::from(path);
    let mut out = PathBuf::new();
    for part in candidate.components() {
        match part {
            Component::Normal(p) => out.push(p),
            // `.` is harmless but there is no reason to accept it either.
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_and_its_modules_are_all_in_the_table() {
        let names: Vec<&str> = ASSETS.iter().map(|(p, _)| *p).collect();
        for wanted in [
            "index.html",
            "boot.js",
            "client/index.js",
            "client/transport-legacy.js",
            "shell/shell.js",
            "shell/selection.js",
            "panels/sources/panel.js",
            "themes/base.css",
            "themes/dark.css",
        ] {
            assert!(names.contains(&wanted), "{wanted} is not served");
        }
    }

    #[test]
    fn nothing_in_the_table_is_empty_or_listed_twice() {
        let mut seen = std::collections::HashSet::new();
        for (path, body) in ASSETS {
            assert!(!body.is_empty(), "{path} is empty");
            assert!(seen.insert(*path), "{path} is in the table twice");
        }
    }

    /// Every file the browser fetches to put the mixer on screen.
    ///
    /// Walked rather than listed, from `boot.js` along static imports only, so
    /// a module behind an `import()` counts as lazy exactly when it really is
    /// one and there is no table to keep honest by hand. `boot.js` fetches its
    /// panels through `import()` as well, but unconditionally and before it
    /// mounts the shell, so those are seeded in: they are eager in the only
    /// sense that matters, which is what a browser waits for on a first paint.
    fn eager_set() -> std::collections::BTreeSet<&'static str> {
        let mut seen = std::collections::BTreeSet::new();
        let mut stack: Vec<&'static str> = vec!["boot.js"];
        let boot = source_of("boot.js").unwrap_or("");
        for line in boot.lines() {
            if let Some(path) = quoted(line).filter(|p| p.starts_with("./panels/")) {
                if let Some(found) = known(&path[2..]) {
                    stack.push(found);
                }
            }
        }
        while let Some(path) = stack.pop() {
            if !seen.insert(path) {
                continue;
            }
            let Some(source) = source_of(path) else { continue };
            for spec in static_imports(source) {
                if let Some(next) = resolve(path, &spec) {
                    stack.push(next);
                }
            }
        }
        seen
    }

    fn source_of(path: &str) -> Option<&'static str> {
        ASSETS.iter().find(|(p, _)| *p == path).map(|(_, body)| *body)
    }

    fn known(path: &str) -> Option<&'static str> {
        ASSETS.iter().find(|(p, _)| *p == path).map(|(p, _)| *p)
    }

    /// The first quoted string on a line, whatever the quote character.
    fn quoted(line: &str) -> Option<&str> {
        let bytes = line.as_bytes();
        let open = bytes.iter().position(|c| *c == b'"' || *c == b'\'')?;
        let quote = bytes[open];
        let rest = &line[open + 1..];
        let close = rest.find(quote as char)?;
        Some(&rest[..close])
    }

    /// The specifiers of a module's static imports. `import(` is not one.
    fn static_imports(source: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in source.lines() {
            let t = line.trim_start();
            let is_import = (t.starts_with("import ") && !t.starts_with("import(")) || t.starts_with("} from ");
            if !is_import {
                continue;
            }
            // `import x from "y"` and `import "y"` both end in the specifier,
            // so the quoted string after `from`, or the only one, is it.
            let tail = match t.find(" from ") {
                Some(at) => &t[at..],
                None => t,
            };
            if let Some(spec) = quoted(tail) {
                out.push(spec.to_string());
            }
        }
        out
    }

    /// `../../shell/dom.js` from `panels/sources/panel.js` is `shell/dom.js`.
    fn resolve(from: &str, spec: &str) -> Option<&'static str> {
        if !spec.starts_with('.') {
            return None;
        }
        let mut parts: Vec<&str> = from.split('/').collect();
        parts.pop();
        for piece in spec.split('/') {
            match piece {
                "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }
        known(&parts.join("/"))
    }

    #[test]
    fn the_page_a_volunteer_opens_stays_under_250_kb() {
        // The budget from 07 Phase 3, against the set it was written about:
        // what a browser fetches to put a usable mixer on screen.
        //
        // The rule changed here when the composer landed. Before it, `ui/` was
        // small enough that everything served and everything loaded were the
        // same number and one assertion covered both. A scene designer is tens
        // of kilobytes that most sessions never open, and counting it against
        // the page a volunteer opens would measure the wrong thing: what
        // matters is the time to a usable mixer on a phone connection. So the
        // eager set is what is measured here, the whole directory is measured
        // below, and `the_designer_is_not_in_the_eager_set` keeps the line
        // between them where it is.
        let eager = eager_set();
        let mut bytes: usize = eager.iter().filter_map(|p| source_of(p)).map(|b| b.len()).sum();
        // The page itself and the two stylesheets it links, which no module
        // imports and every browser fetches.
        for extra in ["index.html", "themes/base.css", "themes/dark.css"] {
            bytes += source_of(extra).map(|b| b.len()).unwrap_or(0);
        }
        assert!(
            bytes < 250 * 1024,
            "the page loads {} files and {bytes} bytes, over the 250 kB budget",
            eager.len()
        );
    }

    /// Everything reachable from one module by static imports, itself included.
    fn closure_of(entry: &'static str) -> std::collections::BTreeSet<&'static str> {
        let mut seen = std::collections::BTreeSet::new();
        let mut stack = vec![entry];
        while let Some(path) = stack.pop() {
            if !seen.insert(path) {
                continue;
            }
            let Some(source) = source_of(path) else { continue };
            for spec in static_imports(source) {
                if let Some(next) = resolve(path, &spec) {
                    stack.push(next);
                }
            }
        }
        seen
    }

    #[test]
    fn the_page_with_the_composer_open_stays_under_400_kb() {
        // The other half of the rule: the lazy set is not somewhere to hide
        // things. This is the heaviest thing a session can become, the page
        // plus the whole designer and the two kits only it uses, and it is the
        // number a person who actually arranges a scene pays.
        let mut everything = eager_set();
        everything.extend(closure_of("panels/composer/composer.js"));
        let mut bytes: usize = everything.iter().filter_map(|p| source_of(p)).map(|b| b.len()).sum();
        for extra in ["index.html", "themes/base.css", "themes/dark.css", "panels/composer/composer.css"] {
            bytes += source_of(extra).map(|b| b.len()).unwrap_or(0);
        }
        assert!(bytes < 400 * 1024, "the page with the composer open is {bytes} bytes, over the 400 kB budget");
    }

    #[test]
    fn nothing_served_is_unreachable() {
        // A file in the table that nothing imports, and that is not one of the
        // four entry points, is dead weight compiled into the binary. This is
        // the check that keeps the directory honest now that not everything in
        // it is fetched.
        let mut reachable = eager_set();
        reachable.extend(closure_of("panels/composer/composer.js"));
        reachable.extend(closure_of("panels/scenes/more.js"));
        reachable.extend(closure_of("client/transport-legacy.js"));
        reachable.extend(closure_of("client/schema-form.js"));
        reachable.extend(closure_of("shell/palette.js"));
        reachable.extend(closure_of("shell/sandbox.js"));
        reachable.extend(closure_of("panels/welcome/tiles.js"));
        // Not imported by this page at all: it is what a sandboxed panel's own
        // HTML imports, inside the iframe, to talk the same protocol back.
        reachable.extend(closure_of("client/sandbox-client.js"));
        for (path, _) in ASSETS {
            if path.ends_with(".css") || path.ends_with(".html") {
                continue;
            }
            assert!(reachable.contains(path), "{path} is served but nothing imports it");
        }
    }

    #[test]
    fn the_designer_is_not_in_the_eager_set() {
        // Each of these is behind an `import()`, and the comment beside it says
        // what asks for it. A change that makes one of them eager fails here
        // rather than quietly costing every page that never opens it.
        let eager = eager_set();
        for (path, who) in [
            ("panels/composer/composer.js", "a double tap on a scene tile"),
            ("panels/composer/canvas.js", "the composer"),
            ("panels/composer/inspector.js", "the composer"),
            ("kits/canvas/gizmos.js", "the composer's canvas"),
            ("kits/canvas/draw.js", "the composer's canvas"),
            ("kits/schema/index.js", "the composer's inspector"),
            ("kits/schema/render.js", "the composer's inspector"),
            ("kits/protocol/predict.js", "the composer's canvas"),
            ("panels/scenes/more.js", "a right click or Ctrl+C on a scene"),
            ("client/transport-legacy.js", "a core with no /rpc"),
            ("client/schema-form.js", "the add picker and the settings drawer"),
            ("shell/palette.js", "Ctrl+K"),
            ("shell/sandbox.js", "a sandboxed plugin panel"),
            ("panels/welcome/tiles.js", "the welcome dialog"),
        ] {
            assert!(known(path).is_some(), "{path} is not served at all");
            assert!(!eager.contains(path), "{path} is fetched at load, but only {who} needs it");
        }
    }

    #[test]
    fn the_eager_set_is_the_page_and_its_panels() {
        // The walk is only worth trusting if it finds the obvious things.
        let eager = eager_set();
        for wanted in ["boot.js", "client/index.js", "shell/shell.js", "panels/sources/panel.js", "panels/scenes/panel.js", "kits/protocol/mirror.js"] {
            assert!(eager.contains(wanted), "the import walk did not reach {wanted}");
        }
    }

    #[test]
    fn the_page_has_no_inline_script_so_the_policy_can_be_real() {
        let page = ASSETS.iter().find(|(p, _)| *p == "index.html").unwrap().1;
        // A <script> with a src is fine; a <script> with a body is not.
        for chunk in page.split("<script").skip(1) {
            let open = chunk.split('>').next().unwrap_or("");
            assert!(open.contains("src="), "index.html has an inline script: {open}");
        }
        assert!(!CSP.contains("script-src 'self' 'unsafe-inline'"));
    }

    #[test]
    fn content_types_are_what_a_browser_needs_for_modules() {
        assert_eq!(content_type("boot.js"), "text/javascript; charset=utf-8");
        assert_eq!(content_type("themes/base.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
        assert_eq!(content_type("x.bin"), "application/octet-stream");
    }

    #[test]
    fn a_plugin_name_is_a_slug_and_a_traversal_is_not_one() {
        assert!(legible_name("ndi"));
        assert!(legible_name("my-plugin_2"));
        assert!(!legible_name(".."));
        assert!(!legible_name("a/b"));
        assert!(!legible_name(""));
        assert!(!legible_name("Ndi"));
    }

    #[test]
    fn a_plugin_path_cannot_climb_out_of_its_ui_directory() {
        assert!(safe_relative("panel.js").is_some());
        assert!(safe_relative("icons/cam.svg").is_some());
        assert!(safe_relative("../../../etc/passwd").is_none());
        assert!(safe_relative("/etc/passwd").is_none());
        assert!(safe_relative("./panel.js").is_none());
        assert!(safe_relative("").is_none());
    }

    #[test]
    fn the_test_page_is_a_development_page_and_not_part_of_the_product() {
        // It is served at `/test/` when GMX_UI_DEV=1 and nowhere otherwise, so
        // its 19 kB is not in the budget a volunteer's page is measured by.
        assert!(!ASSETS.iter().any(|(p, _)| p.starts_with("test/")));
        assert!(DEV_ASSETS.iter().any(|(p, _)| *p == "test/index.html"));
        assert!(DEV_ASSETS.iter().any(|(p, _)| *p == "test/run.js"));
        assert!(DEV_ASSETS.iter().any(|(p, _)| *p == "test/fixtures.json"));
    }

    #[test]
    fn the_welcome_panel_is_served() {
        for wanted in [
            "panels/welcome/panel.js",
            "panels/welcome/tiles.js",
            "panels/welcome/defaults.js",
        ] {
            assert!(ASSETS.iter().any(|(p, _)| *p == wanted), "{wanted} is not served");
        }
        // The tiles are drawn, not fetched, and small enough to stay that way.
        let tiles = ASSETS.iter().find(|(p, _)| *p == "panels/welcome/tiles.js").unwrap().1;
        for art in tiles.split("frame(").skip(1) {
            let one = art.split(");").next().unwrap_or("");
            assert!(one.len() < 2048, "one tile is {} bytes, over the 2 kB budget", one.len());
        }
    }

    #[test]
    fn the_legacy_page_is_still_here_for_one_release() {
        assert!(LEGACY.contains("<!doctype html>"), "the old single page is still served at /legacy");
        assert!(LEGACY.len() > 50_000, "the whole page moved, not a stub of it");
    }

    #[test]
    fn the_policy_names_every_directive_the_page_depends_on() {
        for directive in [
            "default-src 'self'",
            "img-src 'self' data: blob:",
            "connect-src 'self' ws: wss:",
            "frame-src 'self'",
            "object-src 'none'",
        ] {
            assert!(CSP.contains(directive), "the policy is missing {directive}");
        }
    }

    #[test]
    fn the_plugins_directory_falls_back_to_the_home_directory() {
        let dir = default_plugins_dir();
        assert!(dir.ends_with("plugins"));
        assert!(dir.to_string_lossy().contains(".godwinmix"));
    }
}
