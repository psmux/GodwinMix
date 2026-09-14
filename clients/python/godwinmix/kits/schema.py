"""The UI schema layer, and the ranked renderer registry.

A port of `ui/kits/schema/ui-schema.js` and `registry.js`. The data schema
reader is **not** ported again: :mod:`godwinmix.schema` already has it, and it
is re-exported from here (:func:`describe_form`, :func:`apply_conditions`,
:func:`read_form`, :func:`missing`, :class:`Field`, :class:`Form`) so a caller
needs one import rather than two.

JSON Schema says presentation is out of scope, and it is right: a number
between 0 and 1 might be a slider, a spin box or a dial, and no amount of
staring at the data schema tells you which. So a plugin ships a second, small
document (JSON Forms' split) and :func:`layout_for` merges the two.

The vocabulary is fifteen typed selectors plus a `template.*` tier the host
provides, because Blender, Grafana and VS Code all needed that escape hatch.
Containers are `vertical`, `horizontal`, `group` and `tabs`. A leaf is a
`control` with a `scope` pointing at a property of the data schema. A `rule`
shows, hides, enables or disables an element from another property's value.

Anything a client does not know is rendered by whatever it does know: an
unknown control falls back to the data schema's own default widget, and an
unknown container is laid out vertically. A UI schema from a newer plugin
therefore renders on an older client, which is the whole reason it is data.
"""

from __future__ import annotations

from typing import Any, Callable, Dict, List, NamedTuple, Optional, Tuple

from ..schema import Field, Form, apply_conditions, describe_form, missing, read_form

__all__ = [
    "BASE",
    "CONTROLS",
    "HOST",
    "Field",
    "Form",
    "Node",
    "Picked",
    "Renderer",
    "Renderers",
    "SPECIFIC",
    "apply_conditions",
    "apply_rules",
    "control_for",
    "controls_of",
    "default_layout",
    "describe_form",
    "field_of_scope",
    "layout_for",
    "missing",
    "read_form",
    "register_table",
    "values_of",
]

#: Every control name this vocabulary defines.
CONTROLS = [
    "text",
    "textarea",
    "number",
    "slider",
    "integer",
    "boolean",
    "select",
    "multiselect",
    "radio",
    "colour",
    "file",
    "font",
    "source",
    "unit",
    "json",
    "template.colour",
    "template.curve",
    "template.chroma",
    "template.meter",
]

#: What a field gets when nothing declares a control for it.
BY_KIND = {
    "text": "text",
    "secret": "text",
    "url": "text",
    "number": "number",
    "integer": "integer",
    "boolean": "boolean",
    "choice": "select",
    "lines": "textarea",
    "json": "json",
}


class Node:
    """One element of a layout: a container, or a control with its field.

    `kind` is `"group"`, `"row"`, `"tabs"` or `"control"`. A control carries
    the :class:`~godwinmix.schema.Field` it edits, the control name it asked
    for and whatever `options` the UI schema put beside it. `visible` and
    `enabled` are filled in by :func:`apply_rules` and read by a renderer.
    """

    def __init__(
        self,
        kind: str,
        label: str = "",
        elements: Optional[List["Node"]] = None,
        rule: Optional[Dict[str, Any]] = None,
        field: Optional[Field] = None,
        control: Optional[str] = None,
        options: Optional[Dict[str, Any]] = None,
    ):
        self.kind = kind
        self.label = label
        self.elements: List[Node] = elements if elements is not None else []
        self.rule = rule
        self.field = field
        self.control = control
        self.options: Dict[str, Any] = options or {}
        self.visible = True
        self.enabled = True

    def __repr__(self) -> str:
        if self.kind == "control":
            return f"<Node control {self.field.name if self.field else '?'} as {self.control}>"
        return f"<Node {self.kind} {self.label!r} with {len(self.elements)}>"


def control_for(field: Field, declared: Optional[str] = None) -> str:
    """The control a field ends up with: what was declared, or what fits."""
    if declared and declared in CONTROLS:
        return declared
    return BY_KIND.get(field.kind, "text")


def field_of_scope(scope: Optional[str]) -> Optional[str]:
    """`#/properties/gain` names the field `gain`. A bare name is accepted too."""
    if not scope:
        return None
    text = str(scope)
    at = text.rfind("/")
    return text[at + 1 :] if at >= 0 else text


def layout_for(form: Form, ui: Optional[Dict[str, Any]] = None) -> Node:
    """Merge a data schema description with a UI schema into a layout tree.

    Every field of the data schema appears exactly once. A field the UI schema
    forgot is appended in a section of its own rather than dropped, because a
    plugin author who adds a property and forgets the UI schema should end up
    with an ugly form, never an unreachable setting.
    """
    if not isinstance(ui, dict):
        return default_layout(form)
    used: set = set()
    root = _node(ui, form, used)
    left = [f for f in form.fields if f.name not in used]
    if left:
        label = "More" if root.elements else ""
        root.elements = list(root.elements) + [
            Node("group", label=label, elements=[_leaf(f, None, None) for f in left])
        ]
    return root


def default_layout(form: Form) -> Node:
    """A layout from the data schema alone: the groups it declared, in order."""
    elements: List[Node] = []
    for group in form.groups:
        fields = [f for f in form.fields if (f.group or "") == group]
        if not fields:
            continue
        elements.append(Node("group", label=group, elements=[_leaf(f, None, None) for f in fields]))
    return Node("group", label="", elements=elements)


def _node(spec: Dict[str, Any], form: Form, used: set) -> Node:
    default_type = "control" if (spec.get("control") or spec.get("scope")) else "vertical"
    node_type = str(spec.get("type") or default_type)
    if node_type == "control" or spec.get("scope"):
        name = field_of_scope(spec.get("scope"))
        field = form.field(name) if name else None
        if field is not None:
            used.add(field.name)
        else:
            # A scope pointing at nothing still gets a control. The plugin said
            # it wanted one, and a text box beats a silently missing setting.
            field = Field(name=name or "", label=name or "", kind="text")
        return _leaf(field, spec.get("control"), spec)
    kind = "row" if node_type == "horizontal" else ("tabs" if node_type == "tabs" else "group")
    return Node(
        kind,
        label=str(spec["label"]) if spec.get("label") else "",
        rule=_rule_of(spec),
        elements=[_node(child, form, used) for child in (spec.get("elements") or [])],
    )


def _leaf(field: Field, control: Optional[str], spec: Optional[Dict[str, Any]]) -> Node:
    declared = control or (spec.get("control") if spec else None)
    label = (spec.get("label") if spec else None) or field.label or field.name
    return Node(
        "control",
        label=label,
        field=field,
        control=control_for(field, declared),
        options=(spec.get("options") if spec else None) or {},
        rule=_rule_of(spec),
    )


def _rule_of(spec: Optional[Dict[str, Any]]) -> Optional[Dict[str, Any]]:
    rule = (spec or {}).get("rule")
    if not rule or not rule.get("condition"):
        return None
    condition = rule["condition"]
    out: Dict[str, Any] = {
        "effect": str(rule.get("effect") or "show").lower(),
        "field": field_of_scope(condition.get("scope")),
    }
    if "equals" in condition:
        out["equals"] = condition["equals"]
    elif "const" in condition:
        out["equals"] = condition["const"]
    for key in ("oneOf", "enum"):
        if isinstance(condition.get(key), list):
            out["one_of"] = condition[key]
            break
    return out


def apply_rules(layout: Node, values: Dict[str, Any]) -> Node:
    """Mark every node visible and enabled, from the rules and the `if`/`then`.

    Nothing is rebuilt and nothing is thrown away: a renderer walks the same
    tree again and shows or hides what changed, which is how a form keeps the
    focus it had.
    """

    def visit(node: Node) -> None:
        visible, enabled = _holds(node.rule, values)
        by_field = node.kind != "control" or node.field is None or node.field.visible is not False
        node.visible = visible and by_field
        node.enabled = enabled
        for child in node.elements:
            visit(child)

    visit(layout)
    return layout


def _same(a: Any, b: Any) -> bool:
    """`===` rather than `==`: True is not 1, and 1 is not True."""
    if isinstance(a, bool) != isinstance(b, bool):
        return False
    return a == b


def _holds(rule: Optional[Dict[str, Any]], values: Dict[str, Any]) -> Tuple[bool, bool]:
    if not rule:
        return (True, True)
    value = values.get(rule.get("field"))
    if "one_of" in rule:
        met = any(_same(value, choice) for choice in rule["one_of"])
    elif "equals" in rule:
        met = _same(value, rule["equals"])
    else:
        met = value is not None and value != ""
    effect = rule.get("effect")
    if effect == "hide":
        return (not met, True)
    if effect == "enable":
        return (True, met)
    if effect == "disable":
        return (True, not met)
    return (met, True)


def controls_of(layout: Node) -> List[Node]:
    """Every control node in a layout, in draw order."""
    out: List[Node] = []

    def walk(node: Node) -> None:
        if node.kind == "control":
            out.append(node)
        for child in node.elements:
            walk(child)

    walk(layout)
    return out


# ---------------------------------------------------------------- registry
#
# A ranked tester registry, so a client can render a format better than the kit
# does without the plugin knowing (11 section 5). Every renderer answers one
# question: how well do you handle this control? The highest number wins, ties
# go to whoever registered last, and a renderer that answers zero or less is not
# asked again. That is JSON Forms' model, and it is why a host can add a colour
# picker, a curve editor or an audio meter for `template.*` controls that no
# plugin ships code for.
#
# Nothing here is toolkit specific: `godwinmix.kits.tkrender` registers ttk
# widgets against the same ranks and gets the same choices as the browser.

#: What the kit's own widgets rank at. Beat it to take over a control.
BASE = 10
SPECIFIC = 20
HOST = 30


class Renderer(NamedTuple):
    """One candidate: a name, a tester, and the thing that builds the widget."""

    name: str
    test: Callable[[Node, Optional[Field]], float]
    make: Callable[..., Any]


class Picked(NamedTuple):
    entry: Renderer
    rank: float


class Renderers:
    """The candidates, in registration order."""

    def __init__(self) -> None:
        self.entries: List[Renderer] = []

    def register(
        self,
        name: str,
        test: Callable[[Node, Optional[Field]], float],
        make: Callable[..., Any],
    ) -> Callable[[], None]:
        """Add one renderer. Answers with its removal, so a panel can take it away."""
        if not callable(test) or not callable(make):
            raise TypeError("a renderer needs a test and a make function")
        entry = Renderer(name, test, make)
        self.entries.append(entry)

        def off() -> None:
            if entry in self.entries:
                self.entries.remove(entry)

        return off

    def pick(self, node: Node) -> Optional[Picked]:
        """The best renderer for one control node, or None when nothing will have it."""
        best: Optional[Renderer] = None
        best_rank: float = 0
        # Later registrations win ties, so a client's own renderer overrides
        # the kit's without having to invent a higher number.
        for entry in self.entries:
            try:
                rank = float(entry.test(node, node.field) or 0)
            except (TypeError, ValueError):
                rank = 0
            if rank > 0 and rank >= best_rank:
                best_rank = rank
                best = entry
        return Picked(best, best_rank) if best else None

    def explain(self, layout: Node) -> Dict[str, Optional[str]]:
        """What would render each control, by name. For a test and for a doctor."""
        out: Dict[str, Optional[str]] = {}
        for node in controls_of(layout):
            picked = self.pick(node)
            out[node.field.name if node.field else ""] = picked.entry.name if picked else None
        return out


def register_table(
    registry: Renderers,
    table: Dict[str, Callable[..., Any]],
    rank: float = BASE,
) -> Callable[[], None]:
    """Register one renderer per control name, all at the same rank.

    The shape most clients want: a table from control name to widget maker.
    """
    offs: List[Callable[[], None]] = []
    for control, make in table.items():
        offs.append(
            registry.register(
                control,
                (lambda wanted: lambda node, _field: rank if node.control == wanted else 0)(control),
                make,
            )
        )

    def off() -> None:
        for one in offs:
            one()

    return off


def values_of(form: Form) -> Dict[str, Any]:
    """Every field's value, hidden ones included. What `if` is tested against.

    `Form.values()` under another name, because the JavaScript and TypeScript
    kits spell it `valuesOf` and a reader moving between them should not have
    to hunt.
    """
    return form.values()
