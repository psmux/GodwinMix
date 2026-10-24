//! The headend profile's own tools: the GPU and the machine, hot, and a
//! Flussonic config made into shows, one search away. None of them picks a
//! show: the station answers them for the whole machine.

use super::*;

fn server() -> Server {
    Server::new("http://127.0.0.1:1", None, Profile::Headend)
}

fn tool(s: &Server, name: &str) -> Value {
    s.tools().into_iter().find(|t| t["name"] == name).unwrap_or_else(|| panic!("{name} is not in the headend list"))
}

#[test]
fn the_machine_tools_are_hot_and_read_only() {
    let s = server();
    for name in ["gpu_status", "machine_status"] {
        let t = tool(&s, name);
        assert_eq!(t["annotations"]["readOnlyHint"], true, "{name}");
        assert!(t["inputSchema"]["properties"].get("show").is_none(), "{name} is the station's, not a show's");
    }
    let gpu = tool(&s, "gpu_status");
    let since = serde_json::to_string(&gpu["inputSchema"]["properties"]["since_ms"]["type"]).unwrap();
    assert!(since.contains("integer"), "since_ms is a number of milliseconds: {since}");
}

#[test]
fn the_import_takes_text_or_a_path_and_a_dry_run_and_a_search_finds_it() {
    let s = server();
    let found = mcp_tools::search(&s.registry, "import a flussonic config", 5);
    assert_eq!(found.first().map(|t| t["name"].clone()), Some(json!("import_flussonic")), "{found:?}");
    let t = mcp_tools::all_tools(&s.registry).into_iter().find(|t| t["name"] == "import_flussonic").expect("a tool");
    let props = &t["inputSchema"]["properties"];
    for p in ["text", "path", "dry_run"] {
        assert!(props.get(p).is_some(), "import_flussonic has no {p}: {props}");
    }
    assert!(props.get("show").is_none());
    assert_eq!(t["annotations"]["readOnlyHint"], false, "it makes shows");
    let manual = t["description"].as_str().unwrap();
    assert!(manual.contains("dry_run true (the default)"), "the manual says to look first");
}

#[test]
fn the_headend_tools_plan_to_the_station_routes() {
    let s = server();
    let at = |tool: &str, args: Value| {
        let p = s.plan(tool, &args).unwrap();
        format!("{} {}", p.verb, p.path)
    };
    assert_eq!(at("gpu_status", json!({})), "GET /api/v1/gpu/status");
    assert_eq!(at("machine_status", json!({})), "GET /api/v1/machine/status");
    assert_eq!(at("import_flussonic", json!({ "path": "/etc/flussonic/flussonic.conf" })), "POST /api/v1/headend/import");
    assert_eq!(at("headend_status", json!({})), "GET /api/v1/headend/status");
}

/// What set_show_output's manual promises is what the request takes, so an
/// agent that follows it is never refused for a field that does not exist.
#[test]
fn set_show_output_names_the_hls_params_and_the_encode_wants_it_takes() {
    let s = server();
    let t = tool(&s, "set_show_output");
    let text = serde_json::to_string(&t["inputSchema"]).unwrap();
    for field in ["segments", "access", "segment_ms", "window"] {
        assert!(text.contains(&format!("\"{field}\"")), "params has no {field}");
        assert!(t["description"].as_str().unwrap().contains(field), "the manual leaves out {field}");
    }
    let want: godwinmix_protocol::rendition::RenditionRequest = serde_json::from_value(json!({
        "video": {"codec": "h264", "keyframe_frames": 50, "fit": "pad", "pad_color": "#000000",
                  "deinterlace": "auto", "preset": "p1", "bframes": 0},
        "audio": {"codec": "aac", "bitrate_kbps": 128, "tracks": "all"}
    }))
    .expect("every want the manual names is a field of the request");
    assert_eq!(want.video.unwrap().keyframe_frames, Some(50));
}

/// A headend set up from nothing by an agent: every tool for it is one
/// search away, and takes what its manual says.
#[test]
fn an_agent_can_set_a_headend_up_with_no_flussonic_file() {
    let s = server();
    let all = mcp_tools::all_tools(&s.registry);
    let schema = |name: &str| all.iter().find(|t| t["name"] == name).unwrap_or_else(|| panic!("no tool {name}"))["inputSchema"].clone();
    for (query, name) in [
        ("add headend channels", "add_headend_channels"),
        ("list the headend channels and profiles", "headend_channels"),
        ("scan a multicast feed for its programs", "scan_feed"),
        ("transcode profile for many channels", "save_transcode_profile"),
    ] {
        let found = mcp_tools::search(&s.registry, query, 5);
        assert!(found.iter().any(|t| t["name"] == name), "{query}: {found:?}");
        assert!(schema(name)["properties"].get("show").is_none(), "{name} is the station's");
    }
    assert!(schema("add_headend_channels")["properties"].get("channels").is_some());
    assert_eq!(schema("scan_feed")["required"], json!(["uri"]));
}
