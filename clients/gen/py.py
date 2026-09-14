"""Python output: TypedDicts, the tables, and one coroutine per protocol method."""

import keyword

from model import Ref
from names import python_arg, snake

HEADER = '''"""Generated from protocol.json by clients/gen/generate.py. Do not edit.

Every type, method and event the core describes in `core.api`, as Python. A
method added to the core reaches this file by running:

    python3 clients/gen/generate.py

The drift test in tests/test_generated.py fails if this file and protocol.json
have parted company.

The types here are TypedDicts, which is to say they are the shape of the JSON
on the wire and nothing more: no validation happens at runtime and a dict with
extra keys is still the right type. `total=False` throughout, because a core
one api_level ahead may leave out a field this build thinks is required.
"""

from __future__ import annotations

from typing import Any, Dict, List, Literal, Optional, TypedDict, Union
'''

SCALARS = {"string": "str", "integer": "int", "number": "float", "boolean": "bool"}


def py_type(model, schema, inline_object="Dict[str, Any]"):
    if not isinstance(schema, dict) or not schema:
        return "Any"
    if "$ref" in schema:
        return schema["$ref"].split("/")[-1]
    if "enum" in schema:
        return _literal_union(schema["enum"])
    if "const" in schema:
        return _literal_union([schema["const"]])
    for key in ("oneOf", "anyOf"):
        if key in schema:
            parts = [py_type(model, s, inline_object) for s in schema[key]]
            consts = [s.get("const") for s in schema[key] if "const" in s]
            if consts and len(consts) == len(schema[key]):
                return _literal_union(consts)
            return _union(parts)
    kinds = schema.get("type")
    if kinds is None:
        return "Any"
    if isinstance(kinds, str):
        kinds = [kinds]
    nullable = "null" in kinds
    solid = [k for k in kinds if k != "null"]
    if not solid:
        return "None"
    mapped = [_py_kind(model, schema, k, inline_object) for k in solid]
    text = _union(mapped)
    return f"Optional[{text}]" if nullable else text


def _py_kind(model, schema, kind, inline_object):
    if kind in SCALARS:
        return SCALARS[kind]
    if kind == "array":
        return f"List[{py_type(model, schema.get('items') or {}, inline_object)}]"
    if kind == "object":
        return inline_object
    return "Any"


def _union(parts):
    out = list(dict.fromkeys(p for p in parts if p != "Any"))
    if not out:
        return "Any"
    if len(out) == 1:
        return out[0]
    # `Union[...]` rather than `a | b`: a type alias is an assignment, not an
    # annotation, so `from __future__ import annotations` does not defer it and
    # 3.9 would evaluate the `|` and fail.
    return "Union[" + ", ".join(out) + "]"


def _literal_union(values):
    inner = ", ".join(repr(v) for v in values)
    return f"Literal[{inner}]"


def render(model):
    out = [HEADER]
    out.append(f"API_LEVEL = {model.api_level}")
    out.append(f"API_COMPATIBLE = {model.api_compatible}")
    out.append("")
    out += _types(model)
    out += _tables(model)
    out += _methods(model)
    return "\n".join(out).rstrip() + "\n"


def _types(model):
    out = []
    for name, schema in model.types.items():
        kinds = schema.get("type")
        is_object = kinds == "object" or (isinstance(kinds, list) and "object" in kinds)
        props = schema.get("properties") or {}
        if is_object and props:
            # A key that is not an identifier, or is a Python keyword (the
            # `plugin.update` result has a `from`), cannot sit in a class body.
            if not all(k.isidentifier() and not keyword.iskeyword(k) for k in props):
                out.append(f'{name} = TypedDict("{name}", {{')
                for key, sub in props.items():
                    out.append(f'    "{key}": {py_type(model, sub)},')
                out.append("}, total=False)")
                out.append("")
                continue
            out.append(f"class {name}(TypedDict, total=False):")
            desc = schema.get("description")
            if desc:
                out.append(f'    """{_one_line(desc)}"""')
                out.append("")
            for key, sub in props.items():
                # A JSON Schema may be the bare `true`, which means "anything".
                # `Override.params` is one: it holds whatever the referenced
                # graphic takes.
                hint = sub.get("description") if isinstance(sub, dict) else None
                out.append(f"    {key}: {py_type(model, sub)}")
                if hint:
                    out.append(f"    # {_one_line(hint)}")
            out.append("")
        else:
            desc = schema.get("description")
            if desc:
                out.append(f"# {_one_line(desc)}")
            out.append(f"{name} = {py_type(model, schema)}")
            out.append("")
    return out


def _one_line(text):
    return " ".join(str(text).split())


def _type_of(model, slot, fallback="Dict[str, Any]"):
    if slot is None:
        return fallback
    if isinstance(slot, Ref):
        return slot.name
    return py_type(model, slot)


def _tables(model):
    out = ["METHODS = ("]
    for m in model.methods:
        rest = "None"
        if m["rest"]:
            rest = f'("{m["rest"]["method"]}", "{m["rest"]["path"]}")'
        out.append(
            f'    {{"name": "{m["name"]}", "scope": "{m["scope"]}", '
            f'"mutating": {m["mutating"]}, "destructive": {m["destructive"]}, '
            f'"rest": {rest}, "summary": {_one_line(m["summary"])!r}}},'
        )
    out += [")", ""]
    out.append("EVENT_NAMES = (")
    for e in model.events:
        out.append(f'    "{e["pattern"]}",')
    out += [")", ""]
    out.append("EXT_KEYS = {")
    for e in model.ext:
        out.append(f'    "{e["key"]}": {{"value": {e["value"]!r}, "implemented": {e["implemented"]}}},')
    out += ["}", ""]
    return out


def _arguments(model, method):
    """The keyword arguments one method takes, from its params schema."""
    schema = model.resolve(method["params_schema"]) if method["params_schema"] else {}
    props = schema.get("properties") or {}
    required = list(schema.get("required") or [])
    rows = []
    for wire, sub in props.items():
        rows.append((wire, python_arg(wire), py_type(model, sub), wire in required))
    rows.sort(key=lambda r: (not r[3],))
    extra = schema.get("additionalProperties") is True
    return rows, extra


def _methods(model):
    out = [
        "class GeneratedMethods:",
        '    """One coroutine per protocol method, over whatever transport the subclass has.',
        "",
        "    `Client` inherits this and provides `_call`. Nothing here knows how the",
        "    call travels, so the same generated file serves the WebSocket client and",
        "    anything else that can answer a JSON-RPC request.",
        '    """',
        "",
        "    async def _call(self, method: str, params: Dict[str, Any]) -> Any:",
        "        raise NotImplementedError(",
        '            "this object has no transport: build it with godwinmix.connect()"',
        "        )",
        "",
    ]
    for m in model.methods:
        out += _one_method(model, m)
    return out


def _one_method(model, m):
    ident = snake(m["name"])
    rows, extra = _arguments(model, m)
    result = _type_of(model, m["result"])
    sig = ["self"]
    seen_star = False
    for wire, arg, hint, required in rows:
        if required:
            sig.append(f"{arg}: {hint}")
        else:
            if not seen_star:
                sig.append("*")
                seen_star = True
            sig.append(f"{arg}: {_optional(hint)} = None")
    if extra:
        if not seen_star:
            sig.append("*")
            seen_star = True
        sig.append("**extra: Any")
    out = [f"    async def {ident}(", *[f"        {part}," for part in sig], f"    ) -> {result}:"]
    out.append(f'        """{_one_line(m["summary"])}"""')
    out.append("        params: Dict[str, Any] = {}")
    for wire, arg, _hint, required in rows:
        if required:
            out.append(f'        params["{wire}"] = {arg}')
        else:
            out.append(f"        if {arg} is not None:")
            out.append(f'            params["{wire}"] = {arg}')
    if extra:
        out.append("        params.update(extra)")
    out.append(f'        return await self._call("{m["name"]}", params)')
    out.append("")
    return out


def _optional(hint):
    return hint if hint.startswith("Optional[") or hint == "Any" else f"Optional[{hint}]"
