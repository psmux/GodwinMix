//! The headend profile's own tools: the GPU, the machine, and a Flussonic
//! config made into shows. An agent running a box of many channels needs
//! all of them without a search, and none of them picks a show.

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
    assert_eq!(gpu["inputSchema"]["properties"]["since_ms"]["type"], "integer");
}

#[test]
fn the_import_takes_text_or_a_path_and_a_dry_run() {
    let s = server();
    let t = tool(&s, "import_flussonic");
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
