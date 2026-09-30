//! The page's menu bar as the native one: File, Edit, View, Sources, Scenes,
//! Outputs and Help, read from `shell/menus.json` on the core the window is
//! showing, so there is one definition and the desktop app never has its own
//! idea of what a menu holds.
//!
//! A click runs the item's command in the page through `gmxMenu.run`, the
//! same registry the page's own menu, palette and keyboard use. Nothing is
//! granted to the page for this: the shell calls in, the page never calls
//! out, which keeps the rule that a page served by the core has no IPC.
//!
//! Save project as goes the other way, by the one channel a page has into
//! the shell, a `godwinmix://` navigation: the shell asks where with the
//! system's own save dialog and fetches the file from the core itself.

use serde::Deserialize;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, Submenu, SubmenuBuilder};
use tauri::{AppHandle, Manager, Url, Wry};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use crate::core_link::Target;
use crate::Shell;

/// Where every page item's id starts, so a click can be told from the
/// shell's own items.
const PREFIX: &str = "page|";

#[derive(Deserialize)]
struct Definition {
    menus: Vec<PageMenu>,
}

#[derive(Deserialize)]
struct PageMenu {
    title: String,
    items: Vec<Item>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Item {
    command: Option<String>,
    arg: Option<serde_json::Value>,
    title: String,
    separator: bool,
}

/// Fetch the definition from the core and put the menu up. Any failure
/// leaves the shell's own menu as it was and the page's menu bar on screen.
pub async fn install(app: AppHandle, target: Target) {
    let http = app.state::<Shell>().http.clone();
    let url = format!("{}/shell/menus.json", target.base);
    let Ok(reply) = http.get(url).send().await else { return };
    let Ok(definition) = reply.json::<Definition>().await else { return };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || match build(&handle, &definition) {
        Ok(menu) => {
            if handle.set_menu(menu).is_ok() {
                *handle.state::<Shell>().native_menu.lock().unwrap() = true;
                mark_page(&handle);
            }
        }
        Err(e) => eprintln!("[desktop] the page's menu could not be built: {e}"),
    });
}

/// Tell the page its menu bar is the native one, so it draws none of its own.
pub fn mark_page(app: &AppHandle) {
    if !*app.state::<Shell>().native_menu.lock().unwrap() {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval("document.documentElement.setAttribute('data-native-menu', '')");
    }
}

fn build(app: &AppHandle, definition: &Definition) -> tauri::Result<Menu<Wry>> {
    let mut menu = MenuBuilder::new(app).item(&crate::ui::app_submenu(app)?);
    for page in &definition.menus {
        menu = menu.item(&submenu(app, page)?);
    }
    let window = SubmenuBuilder::new(app, "Window")
        .items(&[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ])
        .build()?;
    menu.item(&window).build()
}

fn submenu(app: &AppHandle, page: &PageMenu) -> tauri::Result<Submenu<Wry>> {
    let mut sub = SubmenuBuilder::new(app, &page.title);
    for item in &page.items {
        if item.separator {
            sub = sub.separator();
            continue;
        }
        let Some(command) = &item.command else { continue };
        let arg = item.arg.as_ref().map(|a| a.to_string()).unwrap_or_else(|| "null".into());
        let mut built = MenuItemBuilder::with_id(format!("{PREFIX}{command}|{arg}"), &item.title);
        // Only the chords no text field needs. Undo, Delete and the number
        // keys stay the page's, so typing in a box still types.
        if let Some(chord) = accelerator(command) {
            built = built.accelerator(chord);
        }
        sub = sub.item(&built.build(app)?);
    }
    // Without these, Cmd+C and Cmd+V do nothing in a text field on macOS.
    if page.title == "Edit" {
        sub = sub
            .separator()
            .item(&PredefinedMenuItem::cut(app, None)?)
            .item(&PredefinedMenuItem::copy(app, None)?)
            .item(&PredefinedMenuItem::paste(app, None)?)
            .item(&PredefinedMenuItem::select_all(app, None)?);
    }
    sub.build()
}

fn accelerator(command: &str) -> Option<&'static str> {
    Some(match command {
        "project.open" => "CmdOrCtrl+O",
        "project.save" => "CmdOrCtrl+S",
        "shell.palette" => "CmdOrCtrl+K",
        _ => return None,
    })
}

/// A page item was clicked: run its command in the page. Answers false for
/// an id that is not a page item.
pub fn on_menu(app: &AppHandle, id: &str) -> bool {
    let Some(rest) = id.strip_prefix(PREFIX) else { return false };
    let (command, arg) = rest.split_once('|').unwrap_or((rest, "null"));
    let command = serde_json::to_string(command).unwrap_or_default();
    // `arg` is JSON the shell wrote itself from the definition.
    let script = format!("window.gmxMenu && window.gmxMenu.run({command}, {arg})");
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval(&script);
    }
    true
}

/// `godwinmix://save-project?name=..&secrets=0&media=0#<page state>`: ask
/// where, then have the core write the file there.
pub fn save_project(app: &AppHandle, url: &Url) {
    let query = |k: &str| url.query_pairs().find(|(key, _)| key == k).map(|(_, v)| v.to_string());
    let name = query("name").unwrap_or_else(|| "GodwinMix project".into());
    let params = serde_json::json!({
        "name": name,
        "include_secrets": query("secrets").as_deref() == Some("1"),
        "include_media": query("media").as_deref() == Some("1"),
        "page": url.fragment().and_then(page_state).unwrap_or(serde_json::Value::Null),
    });
    let file_name = format!("{}.gmxproject", name.replace(['/', '\\', ':'], "-"));
    let handle = app.clone();
    app.dialog()
        .file()
        .add_filter("GodwinMix project", &["gmxproject"])
        .set_file_name(file_name)
        .save_file(move |path| {
            let Some(path) = path.and_then(|p| p.into_path().ok()) else { return };
            tauri::async_runtime::spawn(async move { write_project(handle, path, params).await });
        });
}

/// The page state rode as base64 of its JSON, which is what `btoa` writes.
fn page_state(fragment: &str) -> Option<serde_json::Value> {
    let bytes = base64_decode(fragment)?;
    serde_json::from_slice(&bytes).ok()
}

async fn write_project(app: AppHandle, path: std::path::PathBuf, params: serde_json::Value) {
    let target = app.state::<Shell>().target.lock().unwrap().clone();
    let Some(target) = target else { return };
    let http = app.state::<Shell>().http.clone();
    let mut req = http.post(format!("{}/api/v1/project/export", target.base)).json(&params);
    if !target.token.is_empty() {
        req = req.bearer_auth(&target.token);
    }
    let outcome = async {
        let reply = req.send().await.map_err(|e| e.to_string())?;
        let status = reply.status();
        let body: serde_json::Value = reply.json().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(body["error"]["message"].as_str().or(body["message"].as_str()).unwrap_or("the mixer refused").to_string());
        }
        let text = serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }
    .await;
    if let Err(why) = outcome {
        crate::ui::tell(&app, "The project was not saved", &why, MessageDialogKind::Error);
    }
}

/// Standard base64, as `btoa` writes it. Here rather than a crate: it is
/// the one place the shell reads any.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let text = text.replace("%2B", "+").replace("%2F", "/").replace("%3D", "=");
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes().filter(|c| *c != b'=') {
        let v = ALPHABET.iter().position(|a| *a == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_btoa_writes_reads_back() {
        assert_eq!(base64_decode("eyJhIjoxfQ==").unwrap(), br#"{"a":1}"#);
        assert!(base64_decode("not*base64").is_none());
    }

    #[test]
    fn the_shared_definition_parses() {
        let text = include_str!("../../ui/shell/menus.json");
        let d: Definition = serde_json::from_str(text).unwrap();
        let titles: Vec<&str> = d.menus.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(titles, ["File", "Edit", "View", "Sources", "Scenes", "Outputs", "Help"]);
    }
}
