"""TypeScript output: types, the event map, and one method per protocol method."""

from model import Ref
from names import camel, doc_lines

HEADER = """// Generated from protocol.json by clients/gen/generate.py. Do not edit.
//
// Every type, method and event the core describes in `core.api`, as TypeScript.
// A method added to the core reaches this file by running:
//
//     python3 clients/gen/generate.py
//
// The drift test in test/generated.test.ts fails if this file and protocol.json
// have parted company.
/* eslint-disable */
"""

SCALARS = {"string": "string", "integer": "number", "number": "number", "boolean": "boolean"}


def ts_type(model, schema, indent=0):
    """One JSON Schema node as a TypeScript type expression."""
    if not isinstance(schema, dict) or not schema:
        return "unknown"
    if "$ref" in schema:
        return schema["$ref"].split("/")[-1]
    if "enum" in schema:
        return " | ".join(_literal(v) for v in schema["enum"]) or "never"
    if "const" in schema:
        return _literal(schema["const"])
    for key in ("oneOf", "anyOf"):
        if key in schema:
            parts = [ts_type(model, s, indent) for s in schema[key]]
            seen, out = set(), []
            for p in parts:
                if p not in seen:
                    seen.add(p)
                    out.append(p)
            return " | ".join(out) or "unknown"
    kinds = schema.get("type")
    if kinds is None:
        return "unknown"
    if isinstance(kinds, str):
        kinds = [kinds]
    nullable = "null" in kinds
    solid = [k for k in kinds if k != "null"]
    if not solid:
        return "null"
    mapped = [_ts_kind(model, schema, k, indent) for k in solid]
    text = " | ".join(dict.fromkeys(mapped))
    if nullable:
        text += " | null"
    return text


def _ts_kind(model, schema, kind, indent):
    if kind in SCALARS:
        return SCALARS[kind]
    if kind == "array":
        inner = ts_type(model, schema.get("items") or {}, indent)
        return f"Array<{inner}>" if " " in inner else f"{inner}[]"
    if kind == "object":
        props = schema.get("properties") or {}
        if not props:
            return "Record<string, unknown>"
        return _inline_object(model, schema, indent)
    return "unknown"


def _inline_object(model, schema, indent):
    pad = "  " * (indent + 1)
    lines = ["{"]
    required = set(schema.get("required") or [])
    for name, sub in schema["properties"].items():
        mark = "" if name in required else "?"
        lines.append(f"{pad}{_key(name)}{mark}: {ts_type(model, sub, indent + 1)};")
    if schema.get("additionalProperties") is True:
        lines.append(f"{pad}[key: string]: unknown;")
    lines.append("  " * indent + "}")
    return "\n".join(lines)


def _key(name):
    return name if name.isidentifier() else f'"{name}"'


def _literal(value):
    if isinstance(value, str):
        return '"' + value.replace('"', '\\"') + '"'
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value)


def _doc(text, indent=0):
    lines = doc_lines(text, "")
    if not lines:
        return []
    pad = "  " * indent
    if len(lines) == 1:
        return [f"{pad}/** {lines[0].strip()} */"]
    out = [f"{pad}/**"]
    out += [f"{pad} *{line}" if line else f"{pad} *" for line in lines]
    out.append(f"{pad} */")
    return out


def render(model):
    out = [HEADER]
    out.append(f"export const API_LEVEL = {model.api_level};")
    out.append(f"export const API_COMPATIBLE = {model.api_compatible};")
    out.append("")
    out += _types(model)
    out += _maps(model)
    out += _tables(model)
    out += _methods(model)
    return "\n".join(out).rstrip() + "\n"


def _types(model):
    out = []
    for name, schema in model.types.items():
        out += _doc(schema.get("description"))
        kinds = schema.get("type")
        is_object = kinds == "object" or (isinstance(kinds, list) and "object" in kinds)
        if is_object and (schema.get("properties") or schema.get("additionalProperties") is True):
            out.append(f"export interface {name} {_inline_object(model, schema, 0) if schema.get('properties') else '{ [key: string]: unknown }'}")
        else:
            out.append(f"export type {name} = {ts_type(model, schema)};")
        out.append("")
    return out


def _type_of(model, slot, fallback="Record<string, unknown>"):
    if slot is None:
        return fallback
    if isinstance(slot, Ref):
        return slot.name
    return ts_type(model, slot)


def _maps(model):
    out = ["/** The params each method takes, by method name. */", "export interface MethodParams {"]
    for m in model.methods:
        out.append(f'  "{m["name"]}": {_type_of(model, m["params"], "Record<string, never>")};')
    out += ["}", "", "/** What each method answers with, by method name. */", "export interface MethodResults {"]
    for m in model.methods:
        out.append(f'  "{m["name"]}": {_type_of(model, m["result"])};')
    out += ["}", "", "export type MethodName = keyof MethodParams;", ""]
    out += ["/** The payload of each event, by the name after `event/`. */", "export interface EventPayloads {"]
    for e in model.events:
        if e["binary"]:
            out.append(f'  "{e["pattern"]}": Uint8Array;')
        else:
            out.append(f'  "{e["pattern"]}": {_type_of(model, e["payload"])};')
    out += ["}", "", "export type EventName = keyof EventPayloads;", ""]
    return out


def _tables(model):
    out = [
        "/** What a method is, for a UI that builds its own buttons or its own REST calls. */",
        "export interface MethodInfo {",
        "  name: MethodName;",
        "  summary: string;",
        "  scope: string;",
        "  mutating: boolean;",
        "  destructive: boolean;",
        "  rest?: { method: string; path: string };",
        "}",
        "",
        "export const METHODS: readonly MethodInfo[] = [",
    ]
    for m in model.methods:
        rest = ""
        if m["rest"]:
            rest = f', rest: {{ method: "{m["rest"]["method"]}", path: "{m["rest"]["path"]}" }}'
        summary = m["summary"].replace("\\", "\\\\").replace('"', '\\"').replace("\n", " ")
        out.append(
            f'  {{ name: "{m["name"]}", summary: "{summary}", scope: "{m["scope"]}", '
            f'mutating: {str(m["mutating"]).lower()}, destructive: {str(m["destructive"]).lower()}{rest} }},'
        )
    out += ["] as const;", ""]
    out.append("/** The `ext` keys this api_level knows, and whether the core implements them yet. */")
    out.append("export const EXT_KEYS: Readonly<Record<string, { value: string; implemented: boolean }>> = {")
    for e in model.ext:
        value = e["value"].replace("\\", "\\\\").replace('"', '\\"')
        out.append(f'  "{e["key"]}": {{ value: "{value}", implemented: {str(e["implemented"]).lower()} }},')
    out += ["};", ""]
    out.append("export const EVENT_NAMES: readonly EventName[] = [")
    for e in model.events:
        out.append(f'  "{e["pattern"]}",')
    out += ["];", ""]
    return out


def _methods(model):
    out = [
        "/**",
        " * One method per protocol method, over whatever transport the subclass has.",
        " *",
        " * `Client` extends this and replaces `_call`. Nothing else here knows how",
        " * the call travels, which is why the same generated file serves the",
        " * WebSocket client and anything else that can answer a JSON-RPC request.",
        " */",
        "export class GeneratedMethods {",
        "  /** Replaced by Client. Sends one call and answers its result. */",
        "  _call(method: string, params: Record<string, unknown>): Promise<unknown> {",
        '    return Promise.reject(new Error(`no transport for ${method}: build this object with connect()`));',
        "  }",
        "",
    ]
    for m in model.methods:
        ident = camel(m["name"])
        result = _type_of(model, m["result"])
        doc = _doc(m["summary"], 1)
        out += doc
        if m["params"] is None:
            out.append(f'  {ident}(): Promise<{result}> {{')
            out.append(f'    return this._call("{m["name"]}", {{}}) as Promise<{result}>;')
        else:
            ptype = _type_of(model, m["params"])
            required = bool((model.resolve(m["params_schema"]) or {}).get("required"))
            default = "" if required else " = {}"
            out.append(f'  {ident}(params: {ptype}{default}): Promise<{result}> {{')
            out.append(
                f'    return this._call("{m["name"]}", params as unknown as Record<string, unknown>) '
                f"as Promise<{result}>;"
            )
        out.append("  }")
        out.append("")
    out.append("}")
    return out
