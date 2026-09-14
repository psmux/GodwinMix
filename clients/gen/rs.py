"""Rust output: serde types, the event enum, and one method per protocol method."""

from model import Ref
from names import pascal, rust_field, snake

HEADER = """// Generated from protocol.json by clients/gen/generate.py. Do not edit.
//
// Every type, method and event the core describes in `core.api`, as Rust. A
// method added to the core reaches this file by running:
//
//     python3 clients/gen/generate.py
//
// The drift test in tests/generated.rs fails if this file and protocol.json
// have parted company.
//
// String unions (a source state, an output state) are `String` aliases with a
// table of the values this api_level knows, not enums. A core one level ahead
// may send a state this build has never heard of, and a client that refuses to
// parse the whole status document because of one unknown word is worse than
// useless during a show.
#![allow(clippy::all)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::{Client, Result};
"""

SCALARS = {"string": "String", "integer": "i64", "number": "f64", "boolean": "bool"}
FORMATS = {
    "uint64": "u64",
    "uint32": "u32",
    "uint16": "u16",
    "uint8": "u8",
    "int64": "i64",
    "int32": "i32",
    "double": "f64",
    "float": "f32",
}


def rs_type(model, schema):
    if not isinstance(schema, dict) or not schema:
        return "Value"
    if "$ref" in schema:
        return schema["$ref"].split("/")[-1]
    if "enum" in schema or "const" in schema:
        return "String"
    for key in ("oneOf", "anyOf"):
        if key in schema:
            solid = [s for s in schema[key] if s.get("type") != "null"]
            if len(solid) == 1:
                inner = rs_type(model, solid[0])
                return f"Option<{inner}>" if len(solid) != len(schema[key]) else inner
            if all("const" in s or "enum" in s for s in solid):
                return "String"
            return "Value"
    kinds = schema.get("type")
    if kinds is None:
        return "Value"
    if isinstance(kinds, str):
        kinds = [kinds]
    nullable = "null" in kinds
    solid = [k for k in kinds if k != "null"]
    if not solid:
        return "Value"
    inner = _rs_kind(model, schema, solid[0]) if len(solid) == 1 else "Value"
    return f"Option<{inner}>" if nullable else inner


def _rs_kind(model, schema, kind):
    if kind in SCALARS:
        if kind in ("integer", "number"):
            return FORMATS.get(schema.get("format"), SCALARS[kind])
        return SCALARS[kind]
    if kind == "array":
        return f"Vec<{rs_type(model, schema.get('items') or {})}>"
    if kind == "object":
        if schema.get("properties"):
            return "Value"
        return "BTreeMap<String, Value>"
    return "Value"


def _doc(text, pad=""):
    if not text:
        return []
    return [f"{pad}/// {line}".rstrip() for line in str(text).strip().splitlines()]


def render(model):
    out = [HEADER]
    out.append(f"pub const API_LEVEL: u32 = {model.api_level};")
    out.append(f"pub const API_COMPATIBLE: u32 = {model.api_compatible};")
    out.append("")
    out += _types(model)
    out += _tables(model)
    out += _events(model)
    out += _methods(model)
    return "\n".join(out).rstrip() + "\n"


def _types(model):
    out = []
    for name, schema in model.types.items():
        kinds = schema.get("type")
        is_object = kinds == "object" or (isinstance(kinds, list) and "object" in kinds)
        props = schema.get("properties") or {}
        out += _doc(schema.get("description"))
        if is_object and props:
            out += _struct(model, name, schema)
        elif "enum" in schema or ("oneOf" in schema and all("const" in s or "enum" in s for s in schema["oneOf"])):
            values = schema.get("enum") or [s.get("const") for s in schema["oneOf"] if "const" in s]
            values += [v for s in schema.get("oneOf", []) for v in (s.get("enum") or [])]
            out.append(f"pub type {name} = String;")
            listed = ", ".join(f'"{v}"' for v in dict.fromkeys(values))
            out.append(f"/// The values api_level {model.api_level} knows for [`{name}`].")
            out.append(f"pub const {snake(name).upper()}_VALUES: &[&str] = &[{listed}];")
        else:
            out.append(f"pub type {name} = {rs_type(model, schema)};")
        out.append("")
    return out


def _struct(model, name, schema):
    required = set(schema.get("required") or [])
    out = [
        "#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]",
        "#[serde(default)]",
        f"pub struct {name} {{",
    ]
    for wire, sub in (schema.get("properties") or {}).items():
        ident, renamed = rust_field(wire)
        ty = rs_type(model, sub)
        if wire not in required and not ty.startswith(("Option<", "Vec<", "BTreeMap<")) and ty != "Value":
            ty = f"Option<{ty}>"
        out += _doc(sub.get("description"), "    ")
        if renamed:
            out.append(f'    #[serde(rename = "{wire}")]')
        if ty.startswith("Option<"):
            out.append('    #[serde(skip_serializing_if = "Option::is_none")]')
        out.append(f"    pub {ident}: {ty},")
    if schema.get("additionalProperties") is True:
        out.append("    /// Anything this build does not know a name for.")
        out.append("    #[serde(flatten)]")
        out.append("    pub extra: BTreeMap<String, Value>,")
    out.append("}")
    return out


def _tables(model):
    out = [
        "/// What a method is, for a surface that builds its own menu or its own REST call.",
        "#[derive(Debug, Clone, Copy)]",
        "pub struct MethodInfo {",
        "    pub name: &'static str,",
        "    pub summary: &'static str,",
        "    pub scope: &'static str,",
        "    pub mutating: bool,",
        "    pub destructive: bool,",
        "    pub rest: Option<(&'static str, &'static str)>,",
        "}",
        "",
        f"pub const METHODS: [MethodInfo; {len(model.methods)}] = [",
    ]
    for m in model.methods:
        rest = "None"
        if m["rest"]:
            rest = f'Some(("{m["rest"]["method"]}", "{m["rest"]["path"]}"))'
        summary = " ".join(m["summary"].split()).replace("\\", "\\\\").replace('"', '\\"')
        out.append(
            f'    MethodInfo {{ name: "{m["name"]}", summary: "{summary}", scope: "{m["scope"]}", '
            f'mutating: {str(m["mutating"]).lower()}, destructive: {str(m["destructive"]).lower()}, rest: {rest} }},'
        )
    out += ["];", ""]
    out.append(f"pub const EVENT_NAMES: [&str; {len(model.events)}] = [")
    for e in model.events:
        out.append(f'    "{e["pattern"]}",')
    out += ["];", ""]
    out.append(f"pub const EXT_KEYS: [&str; {len(model.ext)}] = [")
    for e in model.ext:
        out.append(f'    "{e["key"]}",')
    out += ["];", ""]
    return out


def _event_variant(e):
    return pascal(e["pattern"])


def _events(model):
    out = [
        "/// One event off the wire, already parsed into its payload.",
        "///",
        "/// `Other` is not a failure: a core a level ahead sends events this build has",
        "/// never heard of, and a client that panicked on one would be useless.",
        "#[derive(Debug, Clone, PartialEq)]",
        "pub enum Event {",
    ]
    for e in model.events:
        if e["binary"]:
            out.append("    /// 16 byte header then JPEG, decoded by [`crate::frames::parse_frame`].")
            out.append(f"    {_event_variant(e)}(crate::frames::Frame),")
            continue
        out += _doc(e["summary"], "    ")
        out.append(f"    {_event_variant(e)}({_type_of(model, e['payload'], 'Value')}),")
    out += [
        "    /// An event name this api_level does not know, with its params as they came.",
        "    Other { name: String, params: Value },",
        "}",
        "",
        "impl Event {",
        "    /// Parse one `event/...` notification. Unknown names and payloads that do",
        "    /// not fit become [`Event::Other`] rather than an error.",
        "    pub fn parse(name: &str, params: Value) -> Event {",
        '        let pattern = name.strip_prefix("event/").unwrap_or(name);',
        "        match pattern {",
    ]
    for e in model.events:
        if e["binary"]:
            continue
        variant = _event_variant(e)
        out.append(f'            "{e["pattern"]}" => match serde_json::from_value(params.clone()) {{')
        out.append(f"                Ok(payload) => Event::{variant}(payload),")
        out.append('                Err(_) => Event::Other { name: pattern.to_string(), params },')
        out.append("            },")
    out += [
        "            _ => Event::Other { name: pattern.to_string(), params },",
        "        }",
        "    }",
        "",
        "    /// The name after `event/`, whatever the variant.",
        "    pub fn name(&self) -> &str {",
        "        match self {",
    ]
    for e in model.events:
        out.append(f'            Event::{_event_variant(e)}(_) => "{e["pattern"]}",')
    out += [
        "            Event::Other { name, .. } => name,",
        "        }",
        "    }",
        "}",
        "",
    ]
    return out


def _type_of(model, slot, fallback="Value"):
    if slot is None:
        return fallback
    if isinstance(slot, Ref):
        return slot.name
    return rs_type(model, slot)


def _methods(model):
    out = [
        "/// One method per protocol method. Thin wrappers over [`Client::call`], so a",
        "/// method the core gains is one regenerated line here.",
        "impl Client {",
    ]
    for m in model.methods:
        ident = snake(m["name"])
        result = _type_of(model, m["result"])
        out += _doc(" ".join(m["summary"].split()), "    ")
        if m["params"] is None:
            out.append(f"    pub async fn {ident}(&self) -> Result<{result}> {{")
            out.append(f'        self.call("{m["name"]}", &serde_json::json!({{}})).await')
        else:
            ptype = _type_of(model, m["params"])
            out.append(f"    pub async fn {ident}(&self, params: &{ptype}) -> Result<{result}> {{")
            out.append(f'        self.call("{m["name"]}", params).await')
        out.append("    }")
        out.append("")
    out.append("}")
    return out
