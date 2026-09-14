//! The method table: data, not a match arm.
//!
//! Every method the core implements is a `MethodDef` in a `Registry`. The
//! registry is what `/rpc` dispatches through, what `/api/v1` is generated
//! from, what `protocol.json` is written from and what the MCP tool list is
//! built from. One entry, four surfaces, which is the only way they stay in
//! step.
//!
//! Another module adds methods by calling `Registry::register`. See
//! `README.md` in this crate.

use crate::error::RpcError;
use crate::scope::Scope;
use schemars::{JsonSchema, SchemaGenerator};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// A future a handler hands back. Boxed because the handlers live in a table
/// and a table wants one type.
pub type BoxFut<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// What a method does. The context carries the mixer, the calling token and
/// the call's own metadata; it is cloned per call, so keep it cheap.
pub type Handler<C> = Arc<dyn Fn(C, Value) -> BoxFut<Result<Value, RpcError>> + Send + Sync>;

/// A JSON Schema for one side of a method, built against the shared generator
/// so that shared types land in `$defs` once rather than being inlined
/// everywhere.
pub type SchemaFn = fn(&mut SchemaGenerator) -> Value;

/// The schema of a type that derives `JsonSchema`, as a `$ref` into `$defs`.
pub fn schema_of<T: JsonSchema>(g: &mut SchemaGenerator) -> Value {
    g.subschema_for::<T>().to_value()
}

/// A method that takes nothing, or answers nothing worth describing.
pub fn no_params(_g: &mut SchemaGenerator) -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

/// A result whose shape is whatever the thing being listed looks like.
pub fn any_object(_g: &mut SchemaGenerator) -> Value {
    json!({ "type": "object" })
}

/// Which MCP profile a method's tool appears in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// In both profiles. Five of these, no more.
    Minimal,
    /// In `standard` only. Twelve hot tools in total, counting the minimal five.
    Standard,
    /// Reachable by name and through `search_tools`, never in the hot list.
    Search,
}

/// How a method appears to an agent.
#[derive(Debug, Clone)]
pub struct McpBinding {
    /// Tool name. Snake case, because that is what every MCP client shows.
    pub tool: &'static str,
    pub tier: Tier,
    /// The agent's only manual for this tool: what it does, when to reach for
    /// it, and what comes back.
    pub description: &'static str,
}

/// Where a method sits on the REST layer. Produced by `rest_transform` and
/// checked against it by a test, so the table and the rule cannot drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rest {
    pub http: &'static str,
    /// With `{id}` where an id goes.
    pub path: String,
}

/// One method, as every surface sees it.
#[derive(Clone)]
pub struct MethodDef<C> {
    pub name: &'static str,
    /// The `api_level` this method first appeared in, as a version string.
    pub since: &'static str,
    pub scope: Scope,
    /// Needs a confirm token on a token whose policy is `required`, and
    /// accepts `dry_run`.
    pub destructive: bool,
    /// Changes state, so it accepts `idempotency_key` and answers with the
    /// full resulting object.
    pub mutating: bool,
    /// Calling it twice with the same arguments leaves the same state. True
    /// for a take (the same source ends up on air) and for a removal (it is
    /// still gone); false for an upload.
    pub idempotent: bool,
    /// One line, for `protocol.md` and for the tool list.
    pub summary: &'static str,
    pub params: SchemaFn,
    pub result: SchemaFn,
    pub rest: Option<Rest>,
    pub mcp: Option<McpBinding>,
    pub handler: Handler<C>,
}

impl<C> MethodDef<C> {
    /// A read only method. The REST binding comes from the transform rule, so
    /// no call site chooses a path.
    pub fn new(
        name: &'static str,
        scope: Scope,
        summary: &'static str,
        handler: Handler<C>,
    ) -> Self {
        Self {
            name,
            since: "1",
            scope,
            destructive: false,
            mutating: !matches!(scope, Scope::Read),
            idempotent: true,
            summary,
            params: no_params,
            result: any_object,
            rest: rest_transform(name),
            mcp: None,
            handler,
        }
    }

    pub fn params(mut self, f: SchemaFn) -> Self {
        self.params = f;
        self
    }

    pub fn result(mut self, f: SchemaFn) -> Self {
        self.result = f;
        self
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self.mutating = true;
        self
    }

    pub fn not_idempotent(mut self) -> Self {
        self.idempotent = false;
        self
    }

    /// Mark a read only method that still changes something, or a mutating
    /// one that does not.
    pub fn mutating(mut self, yes: bool) -> Self {
        self.mutating = yes;
        self
    }

    pub fn tool(mut self, tool: &'static str, tier: Tier, description: &'static str) -> Self {
        self.mcp = Some(McpBinding {
            tool,
            tier,
            description,
        });
        self
    }

    /// Override the generated REST path. Used only where a route predates the
    /// transform rule and has to keep its shape.
    pub fn rest_at(mut self, http: &'static str, path: &str) -> Self {
        self.rest = Some(Rest {
            http,
            path: path.to_string(),
        });
        self
    }

    /// Take the method off the REST layer entirely: `core.subscribe` is a
    /// WebSocket thing and has no useful HTTP shape.
    pub fn no_rest(mut self) -> Self {
        self.rest = None;
        self
    }
}

/// Every method this core answers.
pub struct Registry<C> {
    methods: BTreeMap<&'static str, MethodDef<C>>,
}

impl<C> Default for Registry<C> {
    fn default() -> Self {
        Self {
            methods: BTreeMap::new(),
        }
    }
}

impl<C> Registry<C> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a method. A second entry under the same name replaces the first,
    /// which is how a build with a plugin loaded overrides a stub.
    pub fn register(&mut self, def: MethodDef<C>) {
        self.methods.insert(def.name, def);
    }

    pub fn get(&self, name: &str) -> Option<&MethodDef<C>> {
        self.methods.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &MethodDef<C>> {
        self.methods.values()
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.methods.keys().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.methods.len()
    }

    pub fn is_empty(&self) -> bool {
        self.methods.is_empty()
    }

    /// The methods nearest to a misspelling, for a `-32601` that names the
    /// next step rather than just saying no.
    pub fn nearest(&self, name: &str) -> Vec<&'static str> {
        let noun = name.split('.').next().unwrap_or(name);
        let mut near: Vec<&'static str> = self
            .methods
            .keys()
            .copied()
            .filter(|m| m.starts_with(noun))
            .collect();
        if near.is_empty() {
            near = self
                .methods
                .keys()
                .copied()
                .filter(|m| m.contains(noun))
                .collect();
        }
        near.truncate(8);
        near
    }
}

/// Collection nouns: `source.list` is `GET /api/v1/sources`. Everything else
/// is a singleton, where `program.take` is `POST /api/v1/program/take`.
const COLLECTIONS: &[&str] = &[
    "source", "output", "filter", "media", "plugin", "node", "scene", "codec",
];

/// The plural a collection noun takes in a path.
fn plural(noun: &str) -> String {
    match noun {
        // Uncountable: `/api/v1/media`, never `/api/v1/medias`.
        "media" => "media".to_string(),
        other => format!("{other}s"),
    }
}

/// The REST transform from 03 section 6, as code.
///
/// `noun.verb` becomes `/api/v1/<noun>/<verb>`, except that `list`, `get`,
/// `add` and `remove` map onto the collection, and a middle segment becomes a
/// sub resource: `source.audio.set` is `POST /api/v1/sources/{id}/audio`.
pub fn rest_transform(method: &str) -> Option<Rest> {
    let parts: Vec<&str> = method.split('.').collect();
    let (noun, verb) = (*parts.first()?, *parts.last()?);
    if parts.len() < 2 {
        return None;
    }
    let middle = &parts[1..parts.len() - 1];
    if COLLECTIONS.contains(&noun) {
        let base = format!("/api/v1/{}", plural(noun));
        return Some(match (verb, middle.is_empty()) {
            ("list", true) => Rest {
                http: "GET",
                path: base,
            },
            ("add", true) => Rest {
                http: "POST",
                path: base,
            },
            ("get", true) => Rest {
                http: "GET",
                path: format!("{base}/{{id}}"),
            },
            ("remove", true) => Rest {
                http: "DELETE",
                path: format!("{base}/{{id}}"),
            },
            (verb, true) => Rest {
                http: "POST",
                path: format!("{base}/{{id}}/{verb}"),
            },
            (_, false) => Rest {
                http: "POST",
                path: format!("{base}/{{id}}/{}", middle.join("/")),
            },
        });
    }
    // A singleton. `program.get` is the programme itself, not a sub path.
    let path = if verb == "get" && middle.is_empty() {
        format!("/api/v1/{noun}")
    } else {
        format!("/api/v1/{noun}/{}", parts[1..].join("/"))
    };
    // A read is a GET so that a browser address bar and a `<img>` tag reach
    // it; everything else is a POST.
    let http = if matches!(
        verb,
        "get"
            | "list"
            | "info"
            | "api"
            | "state"
            | "history"
            | "status"
            | "describe"
            | "stats"
            // The observability reads. Every one of them answers a question
            // and changes nothing, so a browser address bar reaches them.
            | "levels"
            | "dot"
            | "latency"
            | "queues"
            | "clock"
            | "doctor"
            | "startup_report"
            | "session_log"
    ) {
        "GET"
    } else {
        "POST"
    };
    Some(Rest { http, path })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule in 03 section 6, worked through every shape today's routes
    /// take. The table in `protocol.md` is generated from this, so a change
    /// here changes the published reference.
    #[test]
    fn the_rest_transform_is_the_rule_in_the_protocol_document() {
        let at = |m: &str| {
            let r = rest_transform(m).unwrap();
            format!("{} {}", r.http, r.path)
        };
        assert_eq!(at("source.list"), "GET /api/v1/sources");
        assert_eq!(at("source.get"), "GET /api/v1/sources/{id}");
        assert_eq!(at("source.add"), "POST /api/v1/sources");
        assert_eq!(at("source.remove"), "DELETE /api/v1/sources/{id}");
        assert_eq!(at("source.audio.set"), "POST /api/v1/sources/{id}/audio");
        assert_eq!(at("source.seek"), "POST /api/v1/sources/{id}/seek");
        assert_eq!(
            at("output.reconnect"),
            "POST /api/v1/outputs/{id}/reconnect"
        );
        assert_eq!(at("program.take"), "POST /api/v1/program/take");
        assert_eq!(at("program.get"), "GET /api/v1/program");
        assert_eq!(at("program.history"), "GET /api/v1/program/history");
        assert_eq!(at("core.info"), "GET /api/v1/core/info");
        assert_eq!(at("core.status"), "GET /api/v1/core/status");
        assert_eq!(at("agent.state"), "GET /api/v1/agent/state");
        assert_eq!(at("adbreak.start"), "POST /api/v1/adbreak/start");
        // The observability methods. A read is a GET, changing a log level is
        // not, and both sit where ``observe::routes` in the `godwinmix` crate` already answers.
        assert_eq!(at("log.set"), "POST /api/v1/log/set");
        assert_eq!(at("log.gst"), "POST /api/v1/log/gst");
        assert_eq!(at("log.levels"), "GET /api/v1/log/levels");
        assert_eq!(at("pipeline.dot"), "GET /api/v1/pipeline/dot");
        assert_eq!(at("pipeline.list"), "GET /api/v1/pipeline/list");
        assert_eq!(at("core.doctor"), "GET /api/v1/core/doctor");
        assert_eq!(at("core.startup_report"), "GET /api/v1/core/startup_report");
        assert_eq!(at("core.session_log"), "GET /api/v1/core/session_log");
        // Uncountable nouns keep their spelling.
        assert_eq!(at("media.list"), "GET /api/v1/media");
        assert_eq!(at("codec.list"), "GET /api/v1/codecs");
        assert_eq!(at("media.remove"), "DELETE /api/v1/media/{id}");
        // A bare word is not a method and gets no route.
        assert_eq!(rest_transform("ping"), None);
    }

    #[test]
    fn a_registry_replaces_a_method_rather_than_holding_two() {
        let handler: Handler<()> = Arc::new(|_, _| Box::pin(async { Ok(json!({})) }));
        let mut r: Registry<()> = Registry::new();
        r.register(MethodDef::new(
            "source.list",
            Scope::Read,
            "first",
            handler.clone(),
        ));
        r.register(MethodDef::new(
            "source.list",
            Scope::Read,
            "second",
            handler.clone(),
        ));
        assert_eq!(r.len(), 1);
        assert_eq!(r.get("source.list").unwrap().summary, "second");

        // A misspelling is answered with the methods on the same noun.
        r.register(MethodDef::new("source.add", Scope::Operate, "add", handler));
        let near = r.nearest("source.destroy");
        assert!(
            near.contains(&"source.add") && near.contains(&"source.list"),
            "{near:?}"
        );
    }
}
