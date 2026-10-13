use super::*;

fn plugin_at(dir: &std::path::Path, manifest: &str) -> Installed {
    std::fs::create_dir_all(dir.join("ui")).unwrap();
    std::fs::write(dir.join("main.py"), "").unwrap();
    std::fs::write(dir.join("gmx-plugin.toml"), manifest).unwrap();
    loader::read(dir, &Default::default())
}

const HEAD: &str = "[plugin]\nname = \"psalm\"\nversion = \"1.0.0\"\napi = 1\n\
    description = \"d\"\nlicense = \"MIT\"\nplatforms = [\"any\"]\nplacements = [\"sidecar\"]\n\
    [run]\npython = \"main.py\"\n";

#[test]
fn a_panel_provide_is_listed_from_the_versioned_folder_it_was_installed_to() {
    let dir = std::env::temp_dir().join(format!("gmx-panels-{}", std::process::id())).join("psalm/1.0.0");
    std::fs::create_dir_all(dir.join("ui")).unwrap();
    std::fs::write(dir.join("ui/verses.html"), "<p>hi</p>").unwrap();
    std::fs::write(dir.join("ui/panel.json"), r#"{"title":"Verses","height":300}"#).unwrap();
    let toml = format!(
        "{HEAD}[[provides]]\nkind = \"panel\"\nid = \"verses\"\n\
         panel = {{ kind = \"iframe\", entry = \"ui/verses.html\", slots = [\"sidebar\"] }}\n"
    );
    let plugin = plugin_at(&dir, &toml);
    assert!(plugin.problem.is_none(), "{:?}", plugin.problem);
    let found = panels_of(&plugin);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["id"], "psalm/verses");
    assert_eq!(found[0]["title"], "Verses");
    assert_eq!(found[0]["page"], "/plugins/psalm/ui/verses.html");
    assert_eq!(found[0]["tier"], "sandboxed");
    assert!(found[0]["module"].is_null());
    std::fs::remove_dir_all(dir.parent().unwrap().parent().unwrap()).ok();
}

#[test]
fn a_plugin_with_only_ui_panel_js_gets_the_folder_convention() {
    let dir = std::env::temp_dir().join(format!("gmx-panels-js-{}", std::process::id())).join("psalm/1.0.0");
    let plugin = plugin_at(&dir, HEAD);
    assert!(panels_of(&plugin).is_empty(), "nothing to show yet");
    std::fs::write(dir.join("ui/panel.js"), "export {}").unwrap();
    let found = panels_of(&plugin);
    assert_eq!(found[0]["module"], "/plugins/psalm/ui/panel.js");
    assert_eq!(found[0]["tier"], "trusted");
    assert_eq!(found[0]["id"], "psalm/panel");
    std::fs::remove_dir_all(dir.parent().unwrap().parent().unwrap()).ok();
}
