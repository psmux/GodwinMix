//! The MCP surface for many shows: the hot list, the show tools' schemas,
//! and how a tool reaches one show through the station.

use super::*;
use godwinmix_protocol::mcp_bindings;

fn server(profile: Profile) -> Server {
    Server::new("http://127.0.0.1:1", None, profile)
}

fn names(tools: &[Value]) -> Vec<&str> {
    tools.iter().map(|t| t["name"].as_str().unwrap()).collect()
}

/// The hot lists, as snapshots, in method order on the wire so a prompt
/// cache survives a reconnect. A change here is a change to what every agent
/// is charged for on every call, so it is made on purpose or not at all.
///
/// `standard` is the live mix with its graphics and scenes, because that is
/// what people ask an agent for. The shows that serve a headend are their own
/// profile, `headend`, and `call_tool` reaches them from either.
fn hot_list(profile: Profile, rows: &[(&str, &str)]) {
    let s = server(profile);
    let registered = |method: &str| s.registry.get(method).is_some();
    let expected: Vec<&str> = rows
        .iter()
        // A row whose method has not been registered in this build is not a
        // tool yet.
        .filter(|(method, _)| registered(method))
        .map(|(_, tool)| *tool)
        .chain(["call_tool", "search_tools"])
        .collect();
    assert_eq!(names(&s.tools()), expected, "{}", profile.as_str());
}

#[test]
fn the_standard_hot_list_is_the_live_mix_and_its_graphics() {
    hot_list(Profile::Standard, &[
        ("agent.state", "agent_state"),
        ("program.revert", "revert"),
        ("program.take", "take"),
        ("scene.create_from", "create_scene_from"),
        ("scene.item.add", "add_scene_item"),
        ("scene.item.set", "set_scene_item"),
        ("snapshot.get", "snapshot"),
        ("source.add", "add_source"),
        ("source.list", "list_sources"),
        ("source.set", "set_source"),
        ("template.list", "list_templates"),
        ("template.save", "save_template"),
    ]);
}

#[test]
fn the_headend_hot_list_is_the_shows() {
    hot_list(Profile::Headend, &[
        ("agent.state", "agent_state"),
        ("program.take", "take"),
        ("show.add_many", "add_shows"),
        ("show.list", "list_shows"),
        ("show.output.set", "set_show_output"),
        ("show.set", "set_show"),
        ("show.stats", "show_stats"),
        ("source.add", "add_source"),
        ("source.list", "list_sources"),
    ]);
}

/// Every row of the binding table whose method exists is a tool with a self
/// contained object schema and the annotations its method enforces.
#[test]
fn every_show_tool_has_a_schema_an_agent_can_call() {
    let s = server(Profile::Standard);
    let all = mcp_tools::all_tools(&s.registry);
    let mut bound = 0;
    for row in mcp_bindings::table() {
        let Some(def) = s.registry.get(row.method) else { continue };
        bound += 1;
        let tool = all.iter().find(|t| t["name"] == row.tool).unwrap_or_else(|| {
            panic!("{} is registered and has no tool {}", row.method, row.tool)
        });
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "{}", row.tool);
        let text = serde_json::to_string(schema).unwrap();
        assert!(!text.contains("$ref"), "{} still has a $ref", row.tool);
        assert_eq!(tool["annotations"]["destructiveHint"], def.destructive, "{}", row.tool);
        // The station answers these for every show, so none of them takes
        // the argument that picks one.
        if !mcp_tools::addresses_one_show(row.method) {
            assert!(schema["properties"].get("show").is_none(), "{} offers `show`", row.tool);
        }
    }
    assert!(bound >= 8, "only {bound} rows of the table found their method");
}

/// A tool one show answers takes `show`, in the standard profile. The minimal
/// profile has no room for it in its budget and works on the first show.
#[test]
fn a_tool_for_one_show_takes_show_in_standard_and_not_in_minimal() {
    let find = |profile: Profile| {
        server(profile).tools().into_iter().find(|t| t["name"] == "take").unwrap()
    };
    let standard = find(Profile::Standard);
    assert_eq!(standard["inputSchema"]["properties"]["show"]["type"], "string");
    let minimal = find(Profile::Minimal);
    assert!(minimal["inputSchema"]["properties"].get("show").is_none());
}

/// `show` leaves the body and becomes `?show=`, which is how the station
/// chooses the show it passes the call to.
#[test]
fn show_becomes_the_query_the_station_routes_on() {
    let s = server(Profile::Standard);
    let p = s.plan("take", &json!({ "show": "news-1", "source": "cam1" })).unwrap();
    assert_eq!(format!("{} {}", p.verb, p.path), "POST /api/v1/program/take?show=news-1");
    assert!(p.args.get("show").is_none());
    assert_eq!(p.args["source"], "cam1");

    let p = s.plan("list_sources", &json!({ "show": "news-1" })).unwrap();
    assert_eq!(p.path, "/api/v1/sources?show=news-1");

    // Left out, or empty, is the first show, as it always was.
    assert_eq!(s.plan("take", &json!({})).unwrap().path, "/api/v1/program/take");
    assert_eq!(s.plan("take", &json!({ "show": " " })).unwrap().path, "/api/v1/program/take");
    // Anything that is not an id is refused rather than spliced into a URL.
    assert!(s.plan("take", &json!({ "show": "a&b=c" })).is_err());
}

/// The station's own tools route to the paths the station serves.
#[test]
fn show_tools_plan_to_the_station_routes() {
    let s = server(Profile::Standard);
    let at = |tool: &str, args: Value| {
        let p = s.plan(tool, &args).unwrap();
        format!("{} {}", p.verb, p.path)
    };
    assert_eq!(at("list_shows", json!({})), "GET /api/v1/shows");
    assert_eq!(at("add_show", json!({ "name": "News 1" })), "POST /api/v1/shows");
    assert_eq!(at("start_show", json!({ "id": "news-1" })), "POST /api/v1/shows/news-1/start");
    assert_eq!(at("stop_show", json!({ "id": "news-1" })), "POST /api/v1/shows/news-1/stop");
    assert_eq!(at("remove_show", json!({ "id": "news-1" })), "DELETE /api/v1/shows/news-1");
    assert_eq!(at("governor_status", json!({})), "GET /api/v1/governor/status");
    assert_eq!(at("add_channel", json!({ "name": "Studio" })), "POST /api/v1/channels");
    // The bulk methods are about the collection, and a show's outputs are a
    // sub resource of the show, as a channel's destinations are. Checked only
    // once the method is registered.
    let bound = |tool: &str, args: Value, want: &str| {
        if let Ok(p) = s.plan(tool, &args) {
            assert_eq!(format!("{} {}", p.verb, p.path), want);
        }
    };
    bound("add_shows", json!({ "shows": [] }), "POST /api/v1/shows/add_many");
    bound("remove_shows", json!({ "ids": ["a"] }), "POST /api/v1/shows/remove_many");
    bound("show_stats", json!({}), "POST /api/v1/shows/stats");
    let out = json!({ "id": "news-1", "output": "copy" });
    bound("add_show_output", json!({ "id": "news-1", "uri": "udp://239.2.2.2:5000" }), "POST /api/v1/shows/news-1/output/add");
    bound("set_show_output", out.clone(), "POST /api/v1/shows/news-1/output");
    bound("remove_show_output", out, "POST /api/v1/shows/news-1/output/remove");
    bound("set_show_output", json!({ "show": "news-1", "output": "copy" }), "POST /api/v1/shows/news-1/output");
    // A station tool keeps `show` in the body: it is not a routing hint there.
    let p = s.plan("list_shows", &json!({ "show": "x" })).unwrap();
    assert_eq!(p.path, "/api/v1/shows");
}
