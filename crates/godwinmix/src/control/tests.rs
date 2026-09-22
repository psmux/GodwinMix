//! Tests for the control plane.
//!
//! Two of these are the ones CI leans on: `protocol_json_is_current`, which
//! regenerates the committed document and fails on any difference, and
//! `every_route_is_in_the_protocol`, which reads the router's own source and
//! refuses to let a path exist that the reference does not describe.

use super::*;
use godwinmix_protocol::method::Tier;
use godwinmix_protocol::scope::{ConfirmPolicy, Profile, Scope, Token};
use godwinmix_core::config::Superimpose;

// --- the generated document -----------------------------------------------

const PROTOCOL_JSON: &str = include_str!("../../../../protocol.json");
const PROTOCOL_MD: &str = include_str!("../../../../protocol.md");
const OPENAPI_JSON: &str = include_str!("../../../../openapi.json");

/// The CI drift check. `protocol.json` is committed so that a reader of the
/// repository, and a client generator, can see the contract without building
/// anything; that is only worth having if it cannot go stale.
#[test]
fn protocol_json_is_current() {
    let generated = godwinmix_protocol::protocol::json_text(descriptor());
    assert_eq!(
        generated, PROTOCOL_JSON,
        "protocol.json is out of date. Regenerate it with:\n  \
         cargo run --quiet -- --api-info > protocol.json\n  \
         cargo run --quiet -- --api-info --markdown > protocol.md"
    );
}

#[test]
fn protocol_md_is_current() {
    let generated = godwinmix_protocol::protocol::markdown(descriptor());
    assert_eq!(
        generated, PROTOCOL_MD,
        "protocol.md is out of date. Regenerate it with:\n  \
         cargo run --quiet -- --api-info --markdown > protocol.md"
    );
}

#[test]
fn openapi_json_is_current() {
    let generated = godwinmix_protocol::openapi::json_text(openapi());
    assert_eq!(
        generated, OPENAPI_JSON,
        "openapi.json is out of date. Regenerate it with:\n  \
         cargo run --quiet -- --api-info --openapi > openapi.json"
    );
}

/// Every REST route the table describes has to appear in the OpenAPI
/// document, or a generated client is missing a call the server answers.
#[test]
fn openapi_describes_every_rest_route() {
    let doc = openapi();
    let paths = doc["paths"].as_object().unwrap();
    for m in methods::registry().iter() {
        let Some(rest) = &m.rest else { continue };
        let item = paths
            .get(&rest.path)
            .unwrap_or_else(|| panic!("{} is not in openapi.json", rest.path));
        let op = item
            .get(rest.http.to_lowercase())
            .unwrap_or_else(|| panic!("{} {} is not in openapi.json", rest.http, rest.path));
        assert_eq!(op["operationId"], m.name);
        assert_eq!(op["x-scope"], m.scope.as_str());
        assert!(op["responses"]["200"].is_object(), "{} has no success response", m.name);
    }
    // Nothing in the document points at a $defs path that OpenAPI cannot
    // resolve.
    let text = serde_json::to_string(doc).unwrap();
    assert!(!text.contains("#/$defs/"), "a schemars $ref survived into openapi.json");
}

/// The acceptance line from the roadmap: `--api-info | jq .api_level` is 1.
#[test]
fn the_document_declares_api_level_one() {
    let doc = descriptor();
    assert_eq!(doc["api_level"], 1);
    assert_eq!(doc["api_compatible"], 1);
    assert_eq!(doc["core"], "godwinmix");
    assert!(doc["methods"].as_array().unwrap().len() > 20);
    assert!(doc["events"].as_array().unwrap().len() >= 14);
    // Every method carries the four things a client needs to plan a call.
    for m in doc["methods"].as_array().unwrap() {
        assert!(m["name"].is_string());
        assert!(m["since"].is_string(), "{} has no since", m["name"]);
        assert!(m["scope"].is_string(), "{} has no scope", m["name"]);
        assert!(m["params"].is_object(), "{} has no params schema", m["name"]);
        assert!(m["result"].is_object(), "{} has no result schema", m["name"]);
    }
}

/// Every path the router registers has to be in the published reference,
/// either as a method's own REST route or as a documented legacy alias. The
/// router's source is the input, so adding a route without documenting it
/// fails here rather than silently shipping.
#[test]
fn every_route_is_in_the_protocol() {
    let doc = descriptor();
    let mut known: Vec<String> = doc["methods"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m.get("rest").map(|r| r["path"].as_str().unwrap_or("").to_string()))
        .collect();
    for list in ["legacy", "well_known"] {
        known.extend(
            doc[list]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["path"].as_str().unwrap_or("").to_string()),
        );
    }

    let sources = [
        include_str!("../control.rs"),
        include_str!("rest.rs"),
    ];
    let mut checked = 0;
    for text in sources {
        for path in routes_in(text) {
            assert!(
                known.contains(&path),
                "the router serves {path} and protocol.json does not mention it. Add it to \
                 the method table, or to LEGACY_ROUTES / WELL_KNOWN in godwinmix-protocol's protocol.rs."
            );
            checked += 1;
        }
    }
    assert!(checked >= 20, "only found {checked} routes to check, the scan must be broken");
}

/// Pull every `.route("…")` path out of a Rust source file.
fn routes_in(text: &str) -> Vec<String> {
    text.match_indices(".route(\"")
        .filter_map(|(at, _)| {
            let rest = &text[at + ".route(\"".len()..];
            rest.find('"').map(|end| rest[..end].to_string())
        })
        .collect()
}

/// The other half of the same promise: every legacy alias names a method that
/// exists, so the table cannot point at something that was renamed.
#[test]
fn every_legacy_alias_names_a_real_method() {
    let registry = methods::registry();
    for (http, path, method) in godwinmix_protocol::protocol::LEGACY_ROUTES {
        assert!(
            registry.get(method).is_some(),
            "{http} {path} says it is now {method}, and there is no such method"
        );
    }
}

// --- the method table -----------------------------------------------------

/// Every REST path in the table is the one the transform rule produces, apart
/// from the two that carry bytes and say so. Without this the rule is a
/// comment and the paths are whatever somebody typed.
#[test]
fn every_rest_path_comes_from_the_transform_rule() {
    let hand_written = ["media.upload", "snapshot.get"];
    for m in methods::registry().iter() {
        let Some(rest) = &m.rest else { continue };
        if hand_written.contains(&m.name) {
            continue;
        }
        let expected = godwinmix_protocol::method::rest_transform(m.name)
            .unwrap_or_else(|| panic!("{} has no transform", m.name));
        assert_eq!(
            (rest.http, rest.path.as_str()),
            (expected.http, expected.path.as_str()),
            "{} does not sit where the rule puts it",
            m.name
        );
    }
}

/// Scopes and destructiveness are the two flags the server enforces, so the
/// table has to agree with 03 section 6 rather than with whoever typed last.
#[test]
fn the_table_matches_the_scopes_in_the_protocol_document() {
    let reg = methods::registry();
    let scope = |name: &str| reg.get(name).unwrap_or_else(|| panic!("no {name}")).scope;
    assert_eq!(scope("core.info"), Scope::Read);
    assert_eq!(scope("core.api"), Scope::Read);
    assert_eq!(scope("core.subscribe"), Scope::Read);
    assert_eq!(scope("agent.state"), Scope::Read);
    assert_eq!(scope("source.list"), Scope::Read);
    assert_eq!(scope("program.take"), Scope::Operate);
    assert_eq!(scope("source.add"), Scope::Operate);
    assert_eq!(scope("source.remove"), Scope::Operate);
    assert_eq!(scope("core.shutdown"), Scope::Admin);
    assert_eq!(scope("core.restart"), Scope::Admin);
    assert_eq!(scope("filter.add"), Scope::Operate);
    assert_eq!(scope("filter.list"), Scope::Read);
    // The observability methods. Reading what a pipeline is doing is a read;
    // moving a log level or reading the session log is not.
    assert_eq!(scope("pipeline.dot"), Scope::Read);
    assert_eq!(scope("core.doctor"), Scope::Read);
    assert_eq!(scope("log.set"), Scope::Admin);
    assert_eq!(scope("core.session_log"), Scope::Admin);

    let destructive: Vec<&str> =
        reg.iter().filter(|m| m.destructive).map(|m| m.name).collect();
    assert_eq!(
        destructive,
        vec![
            "core.restart",
            "core.shutdown",
            "filter.remove",
            "media.remove",
            "node.remove",
            "output.remove",
            "plugin.add",
            "plugin.remove",
            "plugin.update",
            "preset.apply",
            // A scene and an item are documents: deleting one cannot be undone
            // by repeating the call, so both are confirmed like the rest.
            "scene.item.filter.remove",
            "scene.item.remove",
            "scene.remove",
            "source.remove",
        ],
        "the destructive set is the one 03 section 6 marks, plus filter.remove (taking a \
         filter out changes the picture and cannot be undone by repeating it), \
         plugin.update (it replaces a running plugin, and rolls back rather than undoes), \
         preset.apply (it rewrites the operator's configuration file), core.restart \
         (the programme goes off air until the mixer is back) and the two scene \
         removals (a deleted composition does not come back)"
    );
}

/// A read only method must not be marked mutating, or it would take an
/// idempotency key and be cached for a day.
#[test]
fn a_read_only_method_changes_nothing() {
    for m in methods::registry().iter() {
        if m.scope == Scope::Read {
            assert!(!m.mutating, "{} reads and says it mutates", m.name);
            assert!(!m.destructive, "{} reads and says it is destructive", m.name);
        }
    }
}

/// The hot tool budget is a number, not a hope. Both profiles are measured in
/// bytes, at four bytes to a token.
#[test]
fn the_mcp_profiles_stay_inside_their_budgets() {
    use godwinmix_protocol::mcp_tools::*;
    let reg = methods::registry();

    let standard = tools(&reg, Profile::Standard);
    assert!(
        standard.len() <= STANDARD_TOOLS,
        "the standard profile has {} tools, the ceiling is {STANDARD_TOOLS}",
        standard.len()
    );
    let size = wire_size(&standard);
    assert!(size < STANDARD_BYTES, "the standard tool list is {size} bytes, budget {STANDARD_BYTES}");

    let minimal = tools(&reg, Profile::Minimal);
    assert!(
        minimal.len() <= MINIMAL_TOOLS,
        "the minimal profile has {} tools, the ceiling is {MINIMAL_TOOLS}",
        minimal.len()
    );
    let size = wire_size(&minimal);
    assert!(size < MINIMAL_BYTES, "the minimal tool list is {size} bytes, budget {MINIMAL_BYTES}");

    // Both profiles end with the way out to everything else.
    for profile in [Profile::Standard, Profile::Minimal] {
        let list = tools(&reg, profile);
        assert_eq!(list.last().unwrap()["name"], SEARCH_TOOL);
    }
    // Minimal is a subset of standard, so moving a token between profiles
    // never takes a tool away that the agent was told about.
    let standard_names: Vec<&str> =
        standard.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for tool in &minimal {
        let name = tool["name"].as_str().unwrap();
        assert!(standard_names.contains(&name), "{name} is in minimal and not in standard");
    }
}

/// 09 section 5 item 2: the hot list must not change shape at runtime, or a
/// client's prompt cache is thrown away on every scene change. Registering a
/// plugin's method must not move it.
#[test]
fn adding_a_source_or_a_plugin_does_not_change_the_hot_tool_list() {
    use godwinmix_protocol::mcp_tools::tools;
    let before = tools(&methods::registry(), Profile::Standard);

    let mut reg = methods::registry();
    reg.register(
        godwinmix_protocol::method::MethodDef::new(
            "ndi.discover",
            Scope::Operate,
            "a plugin's own method, registered after startup",
            methods::handler(|_call, _params| async { Ok(serde_json::json!({})) }),
        )
        .tool("ndi_discover", Tier::Search, "a plugin tool, reachable through search_tools"),
    );
    let after = tools(&reg, Profile::Standard);

    let names = |list: &[serde_json::Value]| -> Vec<String> {
        list.iter().map(|t| t["name"].as_str().unwrap_or_default().to_string()).collect()
    };
    assert_eq!(names(&before), names(&after), "the hot list moved when a tool was added");
    // The new tool is reachable, just not hot.
    let all = godwinmix_protocol::mcp_tools::all_tools(&reg);
    assert!(all.iter().any(|t| t["name"] == "ndi_discover"));
    assert!(!names(&after).contains(&"ndi_discover".to_string()));
}

/// Annotations are enforced, not decorative: `readOnlyHint` has to mean the
/// server will refuse to change anything, and `destructiveHint` has to match
/// the flag that triggers the confirm round trip.
#[test]
fn tool_annotations_match_what_the_server_enforces() {
    let reg = methods::registry();
    for tool in godwinmix_protocol::mcp_tools::all_tools(&reg) {
        let method = tool["method"].as_str().unwrap();
        let def = reg.get(method).unwrap();
        let a = &tool["annotations"];
        assert_eq!(a["readOnlyHint"], !def.mutating, "{method} readOnlyHint");
        assert_eq!(a["destructiveHint"], def.destructive, "{method} destructiveHint");
        assert_eq!(a["idempotentHint"], def.idempotent, "{method} idempotentHint");
        // Every tool an agent can reach carries a manual worth reading.
        let description = tool["description"].as_str().unwrap();
        assert!(description.len() > 80, "{method} has a thin description");
        assert_eq!(tool["inputSchema"]["type"], "object", "{method} input schema");
        assert!(
            !serde_json::to_string(&tool["inputSchema"]).unwrap().contains("$ref"),
            "{method} input schema still has a $ref in it, which most clients do not resolve"
        );
    }
}

/// Every refusal a client can provoke has to name what to do next. This walks
/// the constructors rather than the messages, because it is the constructors
/// every handler goes through.
#[test]
fn every_error_names_a_next_step() {
    use godwinmix_protocol::error::{ErrorCode, RpcError};
    let errors = vec![
        RpcError::not_found("source", "cam9", &["cam1".into()]),
        RpcError::not_found("output", "yt", &[]),
        RpcError::scope("source.remove", "operate", &["read".into()]),
        RpcError::invalid_params("source.add could not read its params. Call core.api."),
        RpcError::new(ErrorCode::NotInState, "source cam9 is connecting. Wait for event/source.state and take it then."),
    ];
    for e in errors {
        assert!(e.message.len() > 25, "too terse to act on: {}", e.message);
        assert!(
            e.message.ends_with('.') || e.message.ends_with('!'),
            "an error message is a sentence: {}",
            e.message
        );
        assert!(e.data["retryable"].is_boolean(), "{} does not say whether to retry", e.message);
    }
}

// --- tokens, scopes and the legacy door ------------------------------------

fn headers_with(auth: Option<&str>) -> HeaderMap {
    let mut h = HeaderMap::new();
    if let Some(a) = auth {
        h.insert(header::AUTHORIZATION, a.parse().unwrap());
    }
    h
}

fn uri(s: &str) -> Uri {
    s.parse().unwrap()
}

fn presented(method: &Method, auth: Option<&str>, at: &str) -> Option<String> {
    presented_token(method, &headers_with(auth), &uri(at))
}

#[test]
fn no_token_configured_means_everything_is_open() {
    let tokens = Tokens::default();
    assert!(tokens.authenticate(None).is_ok());
    assert!(tokens.authenticate(Some("junk")).is_ok());
}

#[test]
fn a_token_in_the_header_is_read_on_every_method() {
    let tokens = Tokens::new(vec![Token::legacy("s3cret")], false);
    for method in [Method::GET, Method::POST, Method::DELETE] {
        let presented = presented(&method, Some("Bearer s3cret"), "/api/take");
        assert!(tokens.authenticate(presented.as_deref()).is_ok(), "{method} with the token");
    }
    // The scheme is case insensitive, as HTTP says it is.
    let lower = presented(&Method::POST, Some("bearer s3cret"), "/api/take");
    assert!(tokens.authenticate(lower.as_deref()).is_ok());

    let none = presented(&Method::POST, None, "/api/take");
    assert_eq!(tokens.authenticate(none.as_deref()).unwrap_err().message(), "missing token");
    for wrong in ["Bearer s3cres", "Bearer s3cre", "Bearer s3cret1"] {
        let p = presented(&Method::POST, Some(wrong), "/api/take");
        assert_eq!(tokens.authenticate(p.as_deref()).unwrap_err().message(), "wrong token");
    }
    // Another scheme is not a bearer token at all.
    let basic = presented(&Method::GET, Some("Basic czNjcmV0"), "/api/take");
    assert_eq!(tokens.authenticate(basic.as_deref()).unwrap_err().message(), "missing token");
}

#[test]
fn a_token_in_the_query_is_taken_on_get_only() {
    let tokens = Tokens::new(vec![Token::legacy("s3cret")], false);
    let ok = |p: Option<String>| tokens.authenticate(p.as_deref()).is_ok();
    assert!(ok(presented(&Method::GET, None, "/ws?token=s3cret")));
    // Percent encoded, as a browser would send it, and among other keys.
    assert!(ok(presented(&Method::GET, None, "/ws?x=1&token=s3%63ret&y=2")));
    assert!(!ok(presented(&Method::GET, None, "/ws?token=nope")));
    assert!(!ok(presented(&Method::GET, None, "/ws?token=")));
    // A POST does not get to put the token in its URL.
    assert!(!ok(presented(&Method::POST, None, "/ws?token=s3cret")));
    // A header wins over a query when both are present, wrong or not.
    assert!(!ok(presented(&Method::GET, Some("Bearer nope"), "/ws?token=s3cret")));
}

/// The table is what makes a restricted token possible. A read only token can
/// look and cannot touch, and the refusal says which scope to ask for.
#[test]
fn a_read_only_token_is_refused_before_the_handler_runs() {
    let reader = Token {
        id: "watcher".into(),
        secret: "r".into(),
        scopes: vec![Scope::Read],
        confirm: ConfirmPolicy::None,
        rehearsal: false,
        profile: Profile::Standard,
        agent: false,
        safety: None,
        plugin: None,
        node: None,
    };
    let reg = methods::registry();
    assert!(reader.has(reg.get("source.list").unwrap().scope));
    assert!(!reader.has(reg.get("program.take").unwrap().scope));
    assert!(!reader.has(reg.get("core.shutdown").unwrap().scope));

    let refusal = godwinmix_protocol::error::RpcError::scope(
        "program.take",
        reg.get("program.take").unwrap().scope.as_str(),
        &reader.scope_names(),
    );
    assert_eq!(refusal.code, -32002);
    assert!(refusal.message.contains("operate"), "{}", refusal.message);
}

/// Two doors onto one set of methods must not mean two sets of permissions.
/// Every deprecated path resolves to the method it aliases, so the scope that
/// governs `/api/v1` governs `/api` as well.
#[test]
fn the_legacy_paths_carry_the_scope_of_the_method_they_alias() {
    let routes = rest::legacy_routes();
    let registry = methods::registry();
    let scope_of = |http: Method, path: &str| {
        let (route, _) = rest::resolve(&routes, &http, path).expect("a legacy route");
        (route.method, registry.get(route.method).unwrap().scope)
    };
    assert_eq!(scope_of(Method::GET, "/api/status"), ("core.status", Scope::Read));
    assert_eq!(scope_of(Method::POST, "/api/take"), ("program.take", Scope::Operate));
    assert_eq!(scope_of(Method::POST, "/api/sources"), ("source.add", Scope::Operate));
    assert_eq!(
        scope_of(Method::DELETE, "/api/sources/cam1"),
        ("source.remove", Scope::Operate)
    );
    assert_eq!(scope_of(Method::POST, "/api/shutdown"), ("core.shutdown", Scope::Admin));
    assert_eq!(scope_of(Method::GET, "/ws"), ("core.subscribe", Scope::Read));
    // The listing and the adding sit on one path under two verbs, and they do
    // not have the same scope.
    assert_eq!(scope_of(Method::GET, "/api/outputs"), ("output.list", Scope::Read));
    assert_eq!(scope_of(Method::POST, "/api/outputs"), ("output.add", Scope::Operate));

    // A read only token is refused on every path that changes something, and
    // nowhere else.
    let reader = Token {
        id: "watcher".into(),
        secret: "r".into(),
        scopes: vec![Scope::Read],
        confirm: ConfirmPolicy::None,
        rehearsal: false,
        profile: Profile::Standard,
        agent: false,
        safety: None,
        plugin: None,
        node: None,
    };
    for (http, path) in [
        (Method::POST, "/api/take"),
        (Method::POST, "/api/sources"),
        (Method::DELETE, "/api/sources/cam1"),
        (Method::POST, "/api/shutdown"),
    ] {
        let (method, scope) = scope_of(http, path);
        assert!(!reader.has(scope), "a read only token can still reach {method}");
    }
    assert!(reader.has(scope_of(Method::GET, "/api/status").1));
    assert!(reader.has(scope_of(Method::GET, "/api/agent/state").1));
}

/// The other half of the same rule, which the deprecated door used to miss: a
/// token whose policy is `confirm = required` could remove a source, drop an
/// output or shut the mixer down through `/api`, because the guard there
/// checked the scope and the rehearsal flag and not the confirm policy. Those
/// paths have no envelope to carry a confirm token, so the answer is to send
/// the caller to the versioned route rather than to invent a round trip they
/// cannot complete.
#[test]
fn a_confirm_required_token_cannot_destroy_anything_through_the_deprecated_door() {
    let routes = rest::legacy_routes();
    let registry = methods::registry();
    let careful = Token {
        id: "studio-agent".into(),
        secret: "x".into(),
        scopes: vec![Scope::Read, Scope::Operate, Scope::Admin],
        confirm: ConfirmPolicy::Required,
        rehearsal: false,
        profile: Profile::Standard,
        agent: true,
        safety: None,
        plugin: None,
        node: None,
    };
    let easy = Token { confirm: ConfirmPolicy::None, ..careful.clone() };

    let destructive = [
        (Method::DELETE, "/api/sources/cam1", "source.remove"),
        (Method::DELETE, "/api/outputs/yt", "output.remove"),
        (Method::DELETE, "/api/media/clip.mp4", "media.remove"),
        (Method::POST, "/api/shutdown", "core.shutdown"),
    ];
    for (http, path, method) in destructive {
        let (route, _) = rest::resolve(&routes, &http, path).expect("a legacy route");
        assert_eq!(route.method, method);
        let def = registry.get(route.method).unwrap();
        assert!(def.destructive, "{method} should be marked destructive");
        assert!(careful.has(def.scope), "the scope is not what is refusing this");

        let refusal = super::legacy_refusal(&careful, def, path)
            .unwrap_or_else(|| panic!("{path} let a confirm-required token through"));
        assert!(refusal.contains("/api/v1"), "the refusal has to name the way through: {refusal}");
        assert!(refusal.contains(method), "{refusal}");
        // A token that needs no confirmation is not affected.
        assert!(super::legacy_refusal(&easy, def, path).is_none(), "{path}");
    }

    // And nothing that is not destructive is refused.
    for (http, path) in [(Method::POST, "/api/take"), (Method::GET, "/api/status")] {
        let (route, _) = rest::resolve(&routes, &http, path).unwrap();
        let def = registry.get(route.method).unwrap();
        assert!(super::legacy_refusal(&careful, def, path).is_none(), "{path}");
    }
}

// --- the pieces the legacy handlers still lean on --------------------------

/// `add_source` builds its `SourceConfig` through JSON, so the strings the
/// API accepts have to be exactly the ones serde knows.
#[test]
fn superimpose_spellings_survive_the_trip_through_json() {
    let cfg = |v: Value| -> Result<SourceConfig, _> {
        serde_json::from_value(json!({
            "id": "page", "uri": "web+https://example.com/live", "name": null,
            "superimpose": v,
        }))
    };
    assert_eq!(cfg("auto".into()).unwrap().superimpose, Superimpose::Auto);
    assert_eq!(cfg("off".into()).unwrap().superimpose, Superimpose::Off);
    assert!(cfg("on".into()).is_err());
    // Why the handler substitutes "off" rather than passing a null on: the
    // serde default fills in a missing key, not a null one.
    assert!(cfg(Value::Null).is_err());
    let bare: SourceConfig = serde_json::from_value(json!({
        "id": "page", "uri": "web+https://example.com/live",
    }))
    .unwrap();
    assert_eq!(bare.superimpose, Superimpose::Off);
}

#[test]
fn ids_are_derived_from_hosts_and_names() {
    assert_eq!(host_of("https://www.youtube.com/watch?v=x"), "youtube.com");
    assert_eq!(host_of("web+https://user:pw@host.tv:8443/live"), "host.tv");
    assert_eq!(host_of("rtmp://127.0.0.1:1935/live/cam1"), "127.0.0.1");
    assert_eq!(slug("youtube.com"), "youtube-com");
    assert_eq!(slug("  Camera #2 (wide) "), "camera-2-wide");
    assert_eq!(slug("***"), "source");
}

/// golive names a source and an output after their host. The web+ prefix and
/// the port must not leak into the id.
#[test]
fn golive_ids_come_from_the_host() {
    assert_eq!(derived_id("web+http://127.0.0.1:8090/demo.html"), "127-0-0-1");
    assert_eq!(derived_id("web+https://www.example.com/live?x=1"), "example-com");
    assert_eq!(derived_id(&godwinmix_core::input::as_web_uri("example.com/page")), "example-com");
    assert_eq!(derived_id("rtmp://a.rtmp.youtube.com/live2/KEY"), "a-rtmp-youtube-com");
    assert_eq!(derived_id("web+"), "source");
    let mut c = id_candidates("demo");
    assert_eq!(c.next().as_deref(), Some("demo"));
    assert_eq!(c.next().as_deref(), Some("demo-2"));
    assert_eq!(c.last().as_deref(), Some("demo-9"));
}

/// A slider that overshoots still moves the sound, so out of range is
/// clamped. NaN is the one value refused: it cannot arrive from JSON, but
/// `f64::clamp` would hand it straight through to a volume element that then
/// goes silent with nothing in the log to explain it.
#[test]
fn gains_are_clamped_before_they_reach_the_pipeline() {
    assert_eq!(checked_gain(0.5).unwrap(), 0.5);
    assert_eq!(checked_gain(-2.0).unwrap(), 0.0);
    assert_eq!(checked_gain(1e6).unwrap(), MAX_GAIN);
    assert_eq!(checked_gain(f64::INFINITY).unwrap(), MAX_GAIN);
    assert_eq!(checked_gain(f64::NEG_INFINITY).unwrap(), 0.0);
    assert!(checked_gain(f64::NAN).is_err());
}

/// A scrubber dragged off either end of its track should land at that end.
#[test]
fn a_position_off_the_end_of_the_track_is_clamped_not_refused() {
    assert_eq!(checked_position(0.0).unwrap(), 0);
    assert_eq!(checked_position(42_000.0).unwrap(), 42_000);
    assert_eq!(checked_position(-5_000.0).unwrap(), 0);
    // Fractions come of dividing a pixel position by a track width.
    assert_eq!(checked_position(41_999.6).unwrap(), 42_000);
    // Saturating rather than wrapping: a silly number must not land near the
    // start of the clip, which is what `as` on a float used to do.
    assert_eq!(checked_position(1e300).unwrap(), u64::MAX);
    assert!(checked_position(f64::NAN).is_err());
}

#[test]
fn the_seek_request_needs_a_position() {
    let parse = |v: Value| serde_json::from_value::<godwinmix_protocol::SeekRequest>(v);
    assert_eq!(parse(json!({ "position_ms": 42000 })).unwrap().position_ms, 42_000.0);
    // An integer and a float both arrive as the same thing, so a UI can send
    // whatever its slider gives it.
    assert_eq!(parse(json!({ "position_ms": 42000.5 })).unwrap().position_ms, 42_000.5);
    // An empty body is not a read here. There is nothing to read: the position
    // is in the status snapshot and in the position event already.
    assert!(parse(json!({})).is_err());
}

async fn body_json(r: Response) -> Value {
    let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Three answers, three statuses, on the legacy door. The 409 is the one that
/// earns its keep: a whole page source exists and takes the request happily,
/// but its sounds were mixed by Chromium and there is nothing behind the
/// balance faders.
#[tokio::test]
async fn balancing_says_which_kind_of_no_it_is() {
    let ok = audio_response(
        "page",
        AudioOutcome::Set(godwinmix_core::state::SourceAudioState {
            gain: 1.0,
            muted: false,
            page: Some(0.8),
            media: Some(vec![1.0, 0.0]),
        }),
    );
    assert_eq!(ok.status(), StatusCode::OK);
    let v = body_json(ok).await;
    assert_eq!(v["gain"], 1.0);
    assert_eq!(v["muted"], false);
    assert_eq!(v["page"], 0.8);
    assert_eq!(v["media"][1], 0.0);

    // A camera answers with the fader and the mute and nothing else.
    let camera = audio_response(
        "cam1",
        AudioOutcome::Set(godwinmix_core::state::SourceAudioState {
            gain: 0.4,
            muted: true,
            page: None,
            media: None,
        }),
    );
    assert_eq!(camera.status(), StatusCode::OK);
    let v = body_json(camera).await;
    assert_eq!(v["gain"], 0.4);
    assert_eq!(v["muted"], true);
    assert!(v.get("page").is_none(), "a camera answer must carry no balance");
    assert!(v.get("media").is_none(), "a camera answer must carry no balance");

    let missing = audio_response("cam9", AudioOutcome::NoSuchSource);
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let v = body_json(missing).await;
    assert!(v["error"].as_str().unwrap().contains("cam9"), "{v}");

    let flat = audio_response("cam1", AudioOutcome::NotSuperimposed);
    assert_eq!(flat.status(), StatusCode::CONFLICT);
    let v = body_json(flat).await;
    let msg = v["error"].as_str().unwrap();
    assert!(msg.contains("cam1") && msg.contains("superimposed"), "unclear message: {msg}");
}

/// The scrubber's three answers on the legacy door.
#[tokio::test]
async fn seeking_says_which_kind_of_no_it_is() {
    let ok = seek_response(
        "clip1",
        SeekOutcome::Moved(godwinmix_core::state::SourcePositionState {
            position_ms: 42_000,
            duration_ms: Some(154_000),
        }),
    );
    assert_eq!(ok.status(), StatusCode::OK);
    let v = body_json(ok).await;
    assert_eq!(v["position_ms"], 42_000);
    assert_eq!(v["duration_ms"], 154_000);

    let early = seek_response(
        "clip1",
        SeekOutcome::Moved(godwinmix_core::state::SourcePositionState {
            position_ms: 1_000,
            duration_ms: None,
        }),
    );
    let v = body_json(early).await;
    assert_eq!(v["position_ms"], 1_000);
    assert!(v.get("duration_ms").is_none());

    let missing = seek_response("cam9", SeekOutcome::NoSuchSource);
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let v = body_json(missing).await;
    assert!(v["error"].as_str().unwrap().contains("cam9"), "{v}");

    let live = seek_response("cam1", SeekOutcome::NotSeekable);
    assert_eq!(live.status(), StatusCode::CONFLICT);
    let v = body_json(live).await;
    let msg = v["error"].as_str().unwrap();
    assert!(msg.contains("cam1") && msg.contains("live feed"), "unclear message: {msg}");

    let refused = seek_response("clip1", SeekOutcome::Failed("demuxer refused the seek".into()));
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    let v = body_json(refused).await;
    assert!(v["error"].as_str().unwrap().contains("demuxer refused"), "{v}");
}

/// `/api/v1/snapshot/{id}` takes an id, and the legacy route takes a file
/// name. Both have to reach the same picture, or the tool that an agent calls
/// with `{"id": "program"}` answers "no such snapshot".
#[test]
fn a_snapshot_is_named_with_or_without_the_extension() {
    for spelling in ["sheet", "sheet.jpg"] {
        assert!(
            godwinmix_core::snapshot::parse_pick(&snapshot_name(spelling)).is_some(),
            "{spelling} has to name the contact sheet"
        );
    }
    assert_eq!(
        godwinmix_core::snapshot::parse_pick(&snapshot_name("cam1")),
        Some(godwinmix_core::snapshot::Pick::Source("cam1".into()))
    );
    assert_eq!(
        godwinmix_core::snapshot::parse_pick(&snapshot_name("program.jpg")),
        Some(godwinmix_core::snapshot::Pick::Program)
    );
    // An empty name is still nothing, rather than becoming ".jpg".
    assert!(godwinmix_core::snapshot::parse_pick(&snapshot_name("")).is_none());
}

/// The mosaic layout id has to be stable for as long as the cells are, and to
/// move the moment they change, or a client cannot match a late frame.
#[test]
fn the_layout_travels_with_the_cells() {
    let status = |source: &str| MultiviewStatus {
        enabled: true,
        width: 960,
        height: 540,
        cols: 2,
        rows: 1,
        cells: vec![godwinmix_core::state::CellAssignment {
            index: 1,
            source: Some(source.into()),
            x: 0,
            y: 0,
            w: 480,
            h: 270,
        }],
        fps: 8,
    };
    let a = layout_of(&status("cam1"));
    let b = layout_of(&status("cam1"));
    let c = layout_of(&status("cam2"));
    assert_eq!(a.id, b.id);
    assert_ne!(a.id, c.id);
    assert_eq!(a.width, 960);
    assert_eq!(a.cells.len(), 1);
}
