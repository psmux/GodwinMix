//! More setup tests: opencode's rules, the dialog's request, the folders
//! `skill install` shares with setup, and a client nothing is written for.

use super::tests::{exe, request, scratch};
use super::*;
use serde_json::json;

/// opencode's config names the rules file in `instructions`, once.
#[test]
fn opencode_reads_what_godwinmix_is_every_session() {
    let (dirs, root) = scratch("rules");
    let req = request(AgentTool::Opencode, SetupScope::User, None);
    let mut setup = plan(&req, &exe(), &dirs).unwrap();
    apply(&mut setup).unwrap();
    let config = dirs.home.join(".config").join("opencode").join("opencode.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
    let rules = dirs.home.join(".config").join("opencode").join("godwinmix.md");
    assert_eq!(v["instructions"], json!([rules.display().to_string()]));
    assert!(std::fs::read_to_string(&rules).unwrap().contains("agent_state"));
    let again = plan(&req, &exe(), &dirs).unwrap();
    assert!(again.writes.iter().all(|w| w.action == merge::Action::Unchanged), "{:?}", again.writes);
    let _ = std::fs::remove_dir_all(&root);
}

/// The request the dialog sends parses, `dry_run` included: it reaches the
/// params as well as the dispatcher, and was once refused as unknown.
#[test]
fn the_dialogs_request_parses() {
    let req: SetupRequest =
        serde_json::from_value(json!({"tool": "opencode", "scope": "project", "dir": "/p", "dry_run": true})).unwrap();
    assert!(req.dry_run && req.scope == SetupScope::Project && req.tool == AgentTool::Opencode);
    let err = serde_json::from_value::<SetupRequest>(json!({"tool": "opencode", "dri_run": true})).unwrap_err();
    assert!(err.to_string().contains("dri_run"), "{err}");
}

/// `gmx skill install --for X` and `gmx agent setup X` put the skills in the
/// same folder, for this user and for a project.
#[test]
fn skill_install_and_setup_agree_on_every_folder() {
    use crate::cli::skill::Tool;
    let dirs = Dirs::from_env().expect("a home folder");
    let cwd = std::env::current_dir().unwrap();
    let pairs = [
        (Tool::Claude, AgentTool::Claude),
        (Tool::Opencode, AgentTool::Opencode),
        (Tool::Pi, AgentTool::Pi),
        (Tool::Codex, AgentTool::Codex),
        (Tool::Gemini, AgentTool::Gemini),
        (Tool::Cursor, AgentTool::Cursor),
        (Tool::Vscode, AgentTool::Vscode),
    ];
    for (skill, agent) in pairs {
        for (project, scope) in [(false, SetupScope::User), (true, SetupScope::Project)] {
            let a = skill.directory(project).unwrap();
            let b = agent.target(scope, &dirs, &cwd).skills.unwrap();
            assert_eq!(a, b, "{agent:?} {scope:?}");
        }
    }
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
