//! Every setup, written into a made up home so no real config is touched.

use super::*;
use serde_json::json;

fn scratch(name: &str) -> (Dirs, PathBuf) {
    let root = std::env::temp_dir().join(format!("gmx-agent-setup-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("project")).unwrap();
    let dirs = Dirs { home: root.join("home"), app_config: root.join("appdata") };
    (dirs, root)
}

fn request(tool: AgentTool, scope: SetupScope, dir: Option<&Path>) -> SetupRequest {
    SetupRequest {
        tool,
        scope,
        dir: dir.map(|d| d.display().to_string()),
        env: Map::new(),
        dry_run: false,
    }
}

fn exe() -> PathBuf {
    std::env::current_exe().unwrap()
}

/// The MCP file each tool reads, per scope, relative to the home or project.
#[test]
fn every_tool_writes_where_it_reads() {
    let (dirs, root) = scratch("where");
    let project = root.join("project");
    let cases: [(AgentTool, SetupScope, &[&str]); 9] = [
        (AgentTool::Claude, SetupScope::User, &["home", ".claude.json"]),
        (AgentTool::Claude, SetupScope::Project, &["project", ".mcp.json"]),
        (AgentTool::Opencode, SetupScope::User, &["home", ".config", "opencode", "opencode.json"]),
        (AgentTool::Opencode, SetupScope::Project, &["project", "opencode.json"]),
        (AgentTool::Codex, SetupScope::User, &["home", ".codex", "config.toml"]),
        (AgentTool::Gemini, SetupScope::User, &["home", ".gemini", "settings.json"]),
        (AgentTool::Cursor, SetupScope::Project, &["project", ".cursor", "mcp.json"]),
        (AgentTool::Vscode, SetupScope::User, &["appdata", "Code", "User", "mcp.json"]),
        (AgentTool::Vscode, SetupScope::Project, &["project", ".vscode", "mcp.json"]),
    ];
    for (tool, scope, parts) in cases {
        let setup = plan(&request(tool, scope, Some(&project)), &exe(), &dirs).unwrap();
        let want = parts.iter().fold(root.clone(), |p, s| p.join(s));
        assert_eq!(setup.writes[0].path, want.display().to_string(), "{tool:?} {scope:?}");
        assert_eq!(setup.writes[0].action, merge::Action::Create);
        assert_eq!(setup.writes.len(), 4, "the server and three skills for {tool:?}");
    }
    // pi has no MCP: only its skills, where it reads them.
    let pi = plan(&request(AgentTool::Pi, SetupScope::User, None), &exe(), &dirs).unwrap();
    assert_eq!(pi.writes.len(), 3);
    assert!(pi.writes[0].path.starts_with(&dirs.home.join(".agents").join("skills").display().to_string()));
    let _ = std::fs::remove_dir_all(&root);
}

/// Applying writes the entry beside what was there, copies the original
/// aside, and a second run changes nothing.
#[test]
fn applying_merges_backs_up_and_is_idempotent() {
    let (dirs, root) = scratch("apply");
    let config = dirs.home.join(".gemini").join("settings.json");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, r#"{"theme": "Dracula", "mcpServers": {"other": {"command": "x"}}}"#).unwrap();
    let req = request(AgentTool::Gemini, SetupScope::User, None);
    let mut setup = plan(&req, &exe(), &dirs).unwrap();
    assert!(!setup.applied);
    apply(&mut setup).unwrap();
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
    assert_eq!(v["theme"], "Dracula");
    assert_eq!(v["mcpServers"]["other"]["command"], "x");
    assert_eq!(v["mcpServers"]["godwinmix"]["args"], json!(["mcp"]));
    let backup = setup.writes[0].backup.clone().expect("a backup");
    assert!(std::fs::read_to_string(&backup).unwrap().contains("Dracula"));
    assert!(dirs.home.join(".gemini/skills/godwinmix-design/SKILL.md").is_file());

    let again = plan(&req, &exe(), &dirs).unwrap();
    assert!(again.writes.iter().all(|w| w.action == merge::Action::Unchanged), "{:?}", again.writes);
    let _ = std::fs::remove_dir_all(&root);
}

/// A dry run is a plan: nothing exists on disk afterwards.
#[test]
fn planning_writes_nothing() {
    let (dirs, root) = scratch("plan");
    plan(&request(AgentTool::Claude, SetupScope::User, None), &exe(), &dirs).unwrap();
    assert!(!dirs.home.exists(), "planning created {}", dirs.home.display());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_project_setup_needs_a_full_path() {
    let (dirs, root) = scratch("project");
    let err = plan(&request(AgentTool::Claude, SetupScope::Project, None), &exe(), &dirs).unwrap_err();
    assert!(err.contains("dir"), "{err}");
    let mut req = request(AgentTool::Claude, SetupScope::Project, None);
    req.dir = Some("relative/folder".into());
    assert!(plan(&req, &exe(), &dirs).unwrap_err().contains("absolute"));
    let _ = std::fs::remove_dir_all(&root);
}

/// The environment goes in under the key each tool reads it from.
#[test]
fn env_is_written_the_way_each_tool_reads_it() {
    let env: Map<String, Value> = serde_json::from_value(json!({"GODWINMIX_URL": "http://127.0.0.1:9"})).unwrap();
    assert_eq!(AgentTool::Opencode.entry("g", &env)["environment"]["GODWINMIX_URL"], "http://127.0.0.1:9");
    assert_eq!(AgentTool::Claude.entry("g", &env)["env"]["GODWINMIX_URL"], "http://127.0.0.1:9");
    assert!(AgentTool::Cursor.entry("g", &Map::new()).get("env").is_none());
}

/// The PATH note lands after the frontmatter, which is where it is read.
#[test]
fn the_path_note_follows_the_frontmatter() {
    let skill = "---\nname: x\ndescription: y\n---\n\n# Body\n";
    let out = files::with_note(skill, Some("> run C:/g.exe"));
    assert!(out.starts_with("---\nname: x\ndescription: y\n---\n"), "{out}");
    assert!(out.contains("> run C:/g.exe\n\n# Body"), "{out}");
    assert_eq!(files::with_note(skill, None), skill);
}

/// Any other client gets the entry to paste and no file.
#[test]
fn another_client_is_shown_the_entry() {
    let (dirs, root) = scratch("other");
    let setup = plan(&request(AgentTool::Other, SetupScope::User, None), &exe(), &dirs).unwrap();
    assert!(setup.writes.is_empty());
    assert_eq!(setup.entry.unwrap()["mcpServers"]["godwinmix"]["args"], json!(["mcp"]));
    let _ = std::fs::remove_dir_all(&root);
}
