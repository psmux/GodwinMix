"""JSON Schema draft 2020-12 to a form description.

Not a UI. A plain data structure a UI renders however it likes: Tkinter widgets
(:mod:`godwinmix.tk`), a terminal form, a Godot VBox. The same shape comes out
of `describeForm` in `@godwinmix/client`, so a plugin's settings look the same
in every surface.

A surface never hardcodes a plugin's settings. It asks `plugin.describe` for the
schema and renders what comes back, which is what makes a plugin usable from a
UI its author has never seen.

Covered: objects, scalars, enums, arrays of scalars, `if`/`then` visibility,
`format: "secret"`, `x-gmx-unit` (a suffix beside the control) and
`x-gmx-group` (a section). Not covered, on purpose: `$ref` beyond `#/$defs/…`,
`oneOf` discrimination, tuple arrays. A plugin needing those ships its own
editor.
"""

from __future__ import annotations

import json
from typing import Any, Dict, Iterable, List, Optional, Set

#: What a control is. The same set the TypeScript reader uses.
KINDS = ("text", "secret", "url", "number", "integer", "boolean", "choice", "lines", "json")


class Field:
    """One control, described rather than drawn."""

    def __init__(self, **kw: Any):
        self.name: str = kw["name"]
        self.label: str = kw["label"]
        self.kind: str = kw["kind"]
        self.description: Optional[str] = kw.get("description")
        self.unit: Optional[str] = kw.get("unit")
        self.group: str = kw.get("group") or ""
        self.required: bool = kw.get("required", False)
        self.value: Any = kw.get("value")
        self.choices: Optional[List[Any]] = kw.get("choices")
        self.minimum: Optional[float] = kw.get("minimum")
        self.maximum: Optional[float] = kw.get("maximum")
        self.step: Optional[float] = kw.get("step")
        self.placeholder: Optional[str] = kw.get("placeholder")
        self.item_kind: Optional[str] = kw.get("item_kind")
        self.show_when: List[Dict[str, Any]] = kw.get("show_when") or []
        self.visible: bool = True

    def __repr__(self) -> str:
        return f"<Field {self.name} {self.kind}{'' if self.visible else ' hidden'}>"


class Form:
    """A schema, read. `fields` in schema order, `groups` in first use order."""

    def __init__(self, title: Optional[str], description: Optional[str], fields: List[Field]):
        self.title = title
        self.description = description
        self.fields = fields
        seen: List[str] = []
        for field in fields:
            if field.group not in seen:
                seen.append(field.group)
        self.groups = sorted(seen, key=lambda g: (g != "",))

    def field(self, name: str) -> Optional[Field]:
        for f in self.fields:
            if f.name == name:
                return f
        return None

    def values(self) -> Dict[str, Any]:
        """Every field's value, hidden ones included. What `if` is tested against."""
        return {f.name: f.value for f in self.fields if f.value is not None}


def describe_form(schema: Dict[str, Any], value: Optional[Dict[str, Any]] = None) -> Form:
    """Read a schema and the current settings into something a UI can lay out."""
    schema = schema or {}
    value = value or {}
    required: Set[str] = set(schema.get("required") or [])
    fields: List[Field] = []
    for name, raw in (schema.get("properties") or {}).items():
        sub = _resolve(schema, raw)
        current = value[name] if name in value else sub.get("default")
        examples = sub.get("examples") or []
        fields.append(
            Field(
                name=name,
                label=str(sub.get("title") or name),
                kind=_kind_of(sub),
                description=sub.get("description"),
                unit=sub.get("x-gmx-unit"),
                group=str(sub.get("x-gmx-group") or ""),
                required=name in required,
                value=current,
                choices=_choices_of(sub),
                minimum=sub.get("minimum"),
                maximum=sub.get("maximum"),
                step=1 if sub.get("type") == "integer" else sub.get("multipleOf"),
                placeholder=str(examples[0]) if examples else None,
                item_kind=_kind_of(_resolve(schema, sub.get("items") or {})) if sub.get("type") == "array" else None,
            )
        )
    form = Form(schema.get("title"), schema.get("description"), fields)
    _attach_conditions(schema, form)
    apply_conditions(form, form.values())
    return form


def apply_conditions(form: Form, values: Dict[str, Any]) -> Form:
    """Work out which fields apply, given what is filled in now.

    Called once by :func:`describe_form` and again by the UI on every edit,
    which is how "codec: h264" makes the h264 fields appear.
    """
    for field in form.fields:
        if not field.show_when:
            continue
        field.visible = all(_holds(cond, values) for cond in field.show_when)
    return form


def read_form(form: Form, values: Dict[str, Any], touched_secrets: Optional[Iterable[str]] = None) -> Dict[str, Any]:
    """The object to send to the core.

    Hidden fields are left out, empty strings are left out rather than sent as
    "", and a secret the operator did not retype is left out rather than
    blanked.
    """
    touched = set(touched_secrets or ())
    out: Dict[str, Any] = {}
    for field in form.fields:
        if not field.visible:
            continue
        if field.kind == "secret" and field.name not in touched:
            continue
        coerced = _coerce(values.get(field.name), field)
        if coerced is None:
            continue
        out[field.name] = coerced
    return out


def missing(form: Form, values: Dict[str, Any]) -> List[str]:
    """Required fields that are visible and still empty."""
    filled = read_form(form, values, [f.name for f in form.fields])
    return [f.name for f in form.fields if f.required and f.visible and not filled.get(f.name)]


# ---------------------------------------------------------------- internals


def _resolve(root: Dict[str, Any], node: Any) -> Dict[str, Any]:
    if isinstance(node, dict) and str(node.get("$ref", "")).startswith("#/$defs/"):
        key = node["$ref"][len("#/$defs/") :]
        merged = dict((root.get("$defs") or {}).get(key) or {})
        merged.update({k: v for k, v in node.items() if k != "$ref"})
        return merged
    return node if isinstance(node, dict) else {}


def _type_of(sub: Dict[str, Any]) -> str:
    kind = sub.get("type")
    if isinstance(kind, list):
        solid = [k for k in kind if k != "null"]
        return solid[0] if solid else "string"
    return kind or "string"


def _kind_of(sub: Dict[str, Any]) -> str:
    if sub.get("enum") is not None or sub.get("oneOf") is not None:
        return "choice"
    kind = _type_of(sub)
    if kind == "boolean":
        return "boolean"
    if kind == "integer":
        return "integer"
    if kind == "number":
        return "number"
    if kind == "array":
        return "lines"
    if kind == "object":
        return "json"
    if sub.get("format") in ("secret", "password"):
        return "secret"
    if sub.get("format") in ("uri", "url"):
        return "url"
    return "text"


def _choices_of(sub: Dict[str, Any]) -> Optional[List[Any]]:
    if isinstance(sub.get("enum"), list):
        return list(sub["enum"])
    one_of = sub.get("oneOf")
    if isinstance(one_of, list) and one_of and all("const" in o for o in one_of):
        return [o["const"] for o in one_of]
    return None


def _attach_conditions(schema: Dict[str, Any], form: Form) -> None:
    blocks = []
    if schema.get("if"):
        blocks.append(schema)
    for block in schema.get("allOf") or []:
        if isinstance(block, dict) and block.get("if"):
            blocks.append(block)

    for block in blocks:
        test: List[Dict[str, Any]] = []
        for name, cond in (block["if"].get("properties") or {}).items():
            if "const" in cond:
                test.append({"field": name, "equals": cond["const"]})
            elif isinstance(cond.get("enum"), list):
                test.append({"field": name, "one_of": cond["enum"]})
        for name in block["if"].get("required") or []:
            test.append({"field": name, "present": True})

        for name in ((block.get("then") or {}).get("properties") or {}):
            field = form.field(name)
            if field is not None:
                field.show_when = field.show_when + test
        # An `else` applies when the `if` did not match. With one test that is
        # the test negated; with several it is "not all of them", which is more
        # than this reader promises, so a multi test `else` shows its fields.
        if len(test) == 1:
            for name in ((block.get("else") or {}).get("properties") or {}):
                field = form.field(name)
                if field is not None:
                    negated = dict(test[0])
                    negated["negate"] = not negated.get("negate", False)
                    field.show_when = field.show_when + [negated]


def _holds(cond: Dict[str, Any], values: Dict[str, Any]) -> bool:
    value = values.get(cond["field"])
    if "present" in cond:
        there = value not in (None, "")
        met = there if cond["present"] else not there
    elif "one_of" in cond:
        met = value in cond["one_of"]
    elif "equals" in cond:
        met = value == cond["equals"]
    else:
        met = True
    return not met if cond.get("negate") else met


def _coerce(raw: Any, field: Field) -> Any:
    if raw is None:
        return None
    if field.kind == "boolean":
        return bool(raw)
    if field.kind in ("integer", "number"):
        if raw == "":
            return None
        try:
            return int(raw) if field.kind == "integer" else float(raw)
        except (TypeError, ValueError):
            return None
    if field.kind == "lines":
        lines = raw if isinstance(raw, list) else [s.strip() for s in str(raw).splitlines() if s.strip()]
        if field.item_kind == "integer":
            return [int(s) for s in lines]
        if field.item_kind == "number":
            return [float(s) for s in lines]
        return [str(s) for s in lines]
    if field.kind == "json":
        if isinstance(raw, (dict, list)):
            return raw
        text = str(raw).strip()
        if not text:
            return None
        try:
            return json.loads(text)
        except ValueError:
            return None
    if field.kind == "choice" and field.choices:
        for choice in field.choices:
            if str(choice) == str(raw):
                return choice
    return None if raw == "" else raw
