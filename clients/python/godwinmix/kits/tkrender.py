"""The schema kit's widgets, in ttk.

This is the Tkinter half of :mod:`godwinmix.kits.schema`: one maker per control
name, registered into the same ranked registry the browser uses, so a panel
that wants a better colour picker registers a higher rank and nothing else
changes.

    from godwinmix.kits.tkrender import inspector

    panel = inspector(parent, schema, ui=ui_schema, value=current)
    panel.grid(row=0, column=0, sticky="nsew")
    ...
    await client.call("plugin.configure", {"id": name, "settings": panel.read()})

Importing this module pulls in `tkinter`, which is exactly why it is a file of
its own: `godwinmix.kits` is arithmetic and a headless script imports it
without loading Tk.

:mod:`godwinmix.tk` does the same job for a bare data schema, in about half the
lines. This one exists because a UI schema adds containers, rules and declared
controls, and a form that handles those does not fit inside the simple one.
"""

from __future__ import annotations

import json
import tkinter as tk
from tkinter import ttk
from typing import Any, Callable, Dict, List, Optional, Tuple

from ..schema import Form, apply_conditions, describe_form, read_form
from .schema import (
    BASE,
    SPECIFIC,
    Node,
    Renderers,
    apply_rules,
    controls_of,
    layout_for,
    register_table,
)

SECRET_KEPT = "•" * 8
HINT = "#666"


class Context:
    """What a widget maker is handed: where to build, and how to read and write.

    The browser's renderers take `{value, set, touchSecret}`. Tkinter needs one
    thing more, a parent widget, because a Tk widget is built into its parent
    rather than appended afterwards.
    """

    def __init__(self, parent: tk.Misc, node: Node, panel: "SchemaPanel"):
        self.parent = parent
        self.node = node
        self.panel = panel

    def value(self) -> Any:
        return self.panel.values.get(self.node.field.name)

    def set(self, value: Any) -> None:
        self.panel.set(self.node.field.name, value)

    def touch_secret(self) -> None:
        self.panel.touched_secrets.add(self.node.field.name)


class SchemaPanel(ttk.Frame):
    """A form built from a data schema, a UI schema, or both."""

    def __init__(
        self,
        parent: tk.Misc,
        schema: Optional[Dict[str, Any]] = None,
        ui: Optional[Dict[str, Any]] = None,
        value: Optional[Dict[str, Any]] = None,
        on_change: Optional[Callable[[Dict[str, Any], str], None]] = None,
        renderers: Optional[Renderers] = None,
    ):
        super().__init__(parent)
        self.form: Form = describe_form(schema or {}, value or {})
        self.layout: Node = layout_for(self.form, ui)
        self.renderers: Renderers = renderers or default_renderers()
        self.values: Dict[str, Any] = self.form.values()
        self.on_change = on_change
        self.touched_secrets: set = set()
        #: node -> the widget that shows it, for the show and hide pass.
        self.built: List[Tuple[Node, tk.Widget]] = []
        self.columnconfigure(0, weight=1)
        self._build()

    # ------------------------------------------------------------------ read

    def read(self) -> Dict[str, Any]:
        """The object to send: hidden fields out, untouched secrets left alone."""
        apply_conditions(self.form, self.values)
        return read_form(self.form, self.values, self.touched_secrets)

    def set(self, name: str, value: Any) -> None:
        """One control changed. Re-run the rules, then show and hide."""
        self.values[name] = value
        apply_conditions(self.form, self.values)
        apply_rules(self.layout, self.values)
        self.refresh()
        if self.on_change:
            self.on_change(self.values, name)

    # ----------------------------------------------------------------- build

    def _build(self) -> None:
        self._fold_conditions()
        apply_rules(self.layout, self.values)
        widget = self._node(self, self.layout)
        widget.grid(row=0, column=0, sticky="ew")
        self.refresh()

    def _fold_conditions(self) -> None:
        """The data schema's own `if`/`then`, folded into the layout's rules.

        `layout_for` copied the fields it found; this points every control back
        at the live field, so `apply_conditions` and `apply_rules` agree about
        what is visible.
        """
        apply_conditions(self.form, self.values)
        for node in controls_of(self.layout):
            field = self.form.field(node.field.name) if node.field else None
            if field is not None:
                node.field = field

    def _node(self, parent: tk.Misc, node: Node) -> tk.Widget:
        if node.kind == "control":
            return self._control(parent, node)
        if node.label:
            box: tk.Widget = ttk.LabelFrame(parent, text=node.label, padding=(8, 4))
        else:
            box = ttk.Frame(parent)
        row = 0
        column = 0
        for child in node.elements:
            made = self._node(box, child)
            if node.kind == "row":
                made.grid(row=0, column=column, sticky="ew", padx=(0, 6))
                box.columnconfigure(column, weight=1)
                column += 1
            else:
                made.grid(row=row, column=0, sticky="ew", pady=1)
                row += 1
        if node.kind != "row":
            box.columnconfigure(0, weight=1)
        self.built.append((node, box))
        return box

    def _control(self, parent: tk.Misc, node: Node) -> tk.Widget:
        row = ttk.Frame(parent)
        row.columnconfigure(1, weight=1)
        text = node.label + (f"  ({node.field.unit})" if node.field.unit else "")
        ttk.Label(row, text=text).grid(row=0, column=0, sticky="w", padx=(0, 8))
        picked = self.renderers.pick(node)
        context = Context(row, node, self)
        made = picked.entry.make(node, context) if picked else _json_box(node, context)
        made.grid(row=0, column=1, sticky="ew")
        if node.field.description:
            hint = ttk.Label(row, text=node.field.description, wraplength=320, foreground=HINT)
            hint.grid(row=1, column=1, sticky="w")
        self.built.append((node, row))
        node.widget = made  # the input itself, for the enable and disable pass
        return row

    def refresh(self) -> None:
        """Show and hide from the rules. Nothing is rebuilt: a rebuild loses focus."""
        for node, widget in self.built:
            if node.visible is False:
                widget.grid_remove()
            else:
                widget.grid()
            inner = getattr(node, "widget", None)
            if inner is not None:
                _set_enabled(inner, node.enabled is not False)


def inspector(
    parent: tk.Misc,
    schema: Optional[Dict[str, Any]] = None,
    ui: Optional[Dict[str, Any]] = None,
    value: Optional[Dict[str, Any]] = None,
    on_change: Optional[Callable[[Dict[str, Any], str], None]] = None,
    renderers: Optional[Renderers] = None,
) -> SchemaPanel:
    """The entry point: a schema, an optional UI schema and a value, in, a frame out.

    The frame has a `read()` that answers with the object to send. A plugin
    that ships nothing but a data schema still gets a usable form, which is the
    promise of the fallback chain in 11 section 5.
    """
    return SchemaPanel(parent, schema, ui, value, on_change, renderers)


def _set_enabled(widget: tk.Widget, enabled: bool) -> None:
    """Grey a widget out, whichever family it came from."""
    try:
        if isinstance(widget, ttk.Widget):
            widget.state(["!disabled"] if enabled else ["disabled"])
        else:
            widget.configure(state="normal" if enabled else "disabled")
    except tk.TclError:
        pass
    for child in widget.winfo_children():
        _set_enabled(child, enabled)


# ------------------------------------------------------------------ widgets


def default_renderers() -> Renderers:
    """The kit's own ttk widgets, at the ranks the browser uses."""
    registry = Renderers()
    register_table(
        registry,
        {
            "text": lambda n, c: _entry(n, c, secret=False),
            "textarea": _area,
            "number": lambda n, c: _number(n, c, integer=False),
            "integer": lambda n, c: _number(n, c, integer=True),
            "slider": _slider,
            "boolean": _toggle,
            "select": lambda n, c: _select(n, c, many=False),
            "multiselect": lambda n, c: _select(n, c, many=True),
            "radio": _radio,
            "colour": _colour,
            "file": lambda n, c: _entry(n, c, secret=False),
            "font": lambda n, c: _entry(n, c, secret=False),
            "source": lambda n, c: _select(n, c, many=False),
            "unit": lambda n, c: _number(n, c, integer=False),
            "json": _json_box,
        },
        BASE,
    )
    # The host tier: a plugin that asks for `template.colour` gets a real
    # colour control without shipping one, which is the escape hatch 11
    # section 5 names.
    register_table(
        registry,
        {
            "template.colour": _colour,
            "template.chroma": _colour,
            "template.curve": _slider,
            "template.meter": _slider,
        },
        SPECIFIC,
    )
    # A secret is a secret whatever the UI schema calls it.
    registry.register(
        "secret",
        lambda node, field: SPECIFIC + 1 if field is not None and field.kind == "secret" else 0,
        lambda n, c: _entry(n, c, secret=True),
    )
    return registry


def _text_of(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value)


def _entry(node: Node, ctx: Context, secret: bool) -> tk.Widget:
    start = SECRET_KEPT if (secret and ctx.value()) else _text_of(ctx.value())
    var = tk.StringVar(value=start)
    box = ttk.Entry(ctx.parent, textvariable=var, show="*" if secret else "")

    def changed(*_: Any) -> None:
        if secret:
            ctx.touch_secret()
        ctx.set(var.get())

    var.trace_add("write", changed)
    return box


def _area(node: Node, ctx: Context) -> tk.Widget:
    value = ctx.value()
    box = tk.Text(ctx.parent, height=3, width=28, wrap="word")
    box.insert("1.0", "\n".join(_text_of(v) for v in value) if isinstance(value, list) else _text_of(value))

    def changed(_event: Any) -> None:
        ctx.set(box.get("1.0", "end-1c"))

    box.bind("<KeyRelease>", changed)
    return box


def _number(node: Node, ctx: Context, integer: bool) -> tk.Widget:
    field = node.field
    var = tk.StringVar(value=_text_of(ctx.value()))
    low = -1e9 if field.minimum is None else field.minimum
    high = 1e9 if field.maximum is None else field.maximum
    step = field.step or (1 if integer else 0.1)
    box = ttk.Spinbox(ctx.parent, textvariable=var, from_=low, to=high, increment=step)

    def changed(*_: Any) -> None:
        text = var.get().strip()
        if text == "":
            ctx.set(None)
            return
        try:
            ctx.set(int(float(text)) if integer else float(text))
        except ValueError:
            # A half typed number ("-", "1.") is not an error, it is a person
            # still typing. The value is left where it was until it parses.
            pass

    var.trace_add("write", changed)
    return box


def _slider(node: Node, ctx: Context) -> tk.Widget:
    field = node.field
    options = node.options
    low = field.minimum if field.minimum is not None else options.get("min", 0)
    high = field.maximum if field.maximum is not None else options.get("max", 1)
    start = ctx.value()
    start = low if start is None else start
    row = ttk.Frame(ctx.parent)
    row.columnconfigure(0, weight=1)
    readout = ttk.Label(row, text=_text_of(start), width=8)
    scale = ttk.Scale(row, from_=low, to=high, value=float(start))

    def changed(raw: Any) -> None:
        value = round(float(raw), 4)
        readout.configure(text=_text_of(value))
        ctx.set(value)

    scale.configure(command=changed)
    scale.grid(row=0, column=0, sticky="ew")
    readout.grid(row=0, column=1, sticky="e", padx=(6, 0))
    return row


def _toggle(node: Node, ctx: Context) -> tk.Widget:
    var = tk.BooleanVar(value=bool(ctx.value()))
    box = ttk.Checkbutton(ctx.parent, variable=var, text=node.options.get("label", ""))
    var.trace_add("write", lambda *_: ctx.set(bool(var.get())))
    return box


def _choices_of(node: Node) -> List[Any]:
    return list(node.field.choices or node.options.get("choices") or [])


def _select(node: Node, ctx: Context, many: bool) -> tk.Widget:
    choices = _choices_of(node)
    if not many:
        var = tk.StringVar(value=_text_of(ctx.value()))
        box = ttk.Combobox(ctx.parent, textvariable=var, state="readonly")
        box["values"] = [_text_of(c) for c in choices]
        var.trace_add("write", lambda *_: ctx.set(_matched(var.get(), choices)))
        return box

    box = tk.Listbox(ctx.parent, selectmode="multiple", exportselection=False, height=min(5, max(3, len(choices))))
    for choice in choices:
        box.insert("end", _text_of(choice))
    current = ctx.value() if isinstance(ctx.value(), list) else []
    for index, choice in enumerate(choices):
        if _text_of(choice) in [_text_of(c) for c in current]:
            box.selection_set(index)

    def changed(_event: Any) -> None:
        ctx.set([choices[i] for i in box.curselection()])

    box.bind("<<ListboxSelect>>", changed)
    return box


def _matched(text: str, choices: List[Any]) -> Any:
    """The choice a combo box's string stands for, so an enum of ints stays int."""
    for choice in choices:
        if _text_of(choice) == text:
            return choice
    return text


def _radio(node: Node, ctx: Context) -> tk.Widget:
    choices = _choices_of(node)
    row = ttk.Frame(ctx.parent)
    var = tk.StringVar(value=_text_of(ctx.value()))
    for column, choice in enumerate(choices):
        button = ttk.Radiobutton(row, text=_text_of(choice), value=_text_of(choice), variable=var)
        button.grid(row=0, column=column, sticky="w", padx=(0, 8))
    var.trace_add("write", lambda *_: ctx.set(_matched(var.get(), choices)))
    return row


def _colour(node: Node, ctx: Context) -> tk.Widget:
    """A hex string and a swatch beside it.

    `tkinter.colorchooser` opens a modal, which a designer wants on a button
    rather than on every repaint, so the button is what opens it.
    """
    row = ttk.Frame(ctx.parent)
    row.columnconfigure(0, weight=1)
    var = tk.StringVar(value=_text_of(ctx.value()) or "#000000")
    entry = ttk.Entry(row, textvariable=var)
    swatch = tk.Frame(row, width=22, height=18, background=_safe_colour(var.get()))

    def changed(*_: Any) -> None:
        swatch.configure(background=_safe_colour(var.get()))
        ctx.set(var.get())

    def pick() -> None:
        from tkinter import colorchooser

        chosen = colorchooser.askcolor(color=_safe_colour(var.get()), parent=row)[1]
        if chosen:
            var.set(chosen)

    var.trace_add("write", changed)
    entry.grid(row=0, column=0, sticky="ew")
    swatch.grid(row=0, column=1, padx=(6, 4))
    ttk.Button(row, text="...", width=3, command=pick).grid(row=0, column=2)
    return row


def _safe_colour(text: str) -> str:
    """Tk raises on a colour it cannot parse, and a half typed hex is common."""
    candidate = (text or "").strip()
    if len(candidate) in (4, 7) and candidate.startswith("#"):
        try:
            int(candidate[1:], 16)
            return candidate
        except ValueError:
            pass
    return "#000000"


def _json_box(node: Node, ctx: Context) -> tk.Widget:
    """The last link of the fallback chain: the value itself, editable.

    Home Assistant's YAML fallback and VS Code's "edit in settings.json" are
    the same idea. The last link always works, so a plugin written against a
    newer core is still editable on an older client.
    """
    row = ttk.Frame(ctx.parent)
    row.columnconfigure(0, weight=1)
    box = tk.Text(row, height=5, width=28, wrap="none")
    value = ctx.value()
    box.insert("1.0", json.dumps(value, indent=2) if value is not None else "")
    problem = ttk.Label(row, text="", foreground="#b00")

    def changed(_event: Any) -> None:
        text = box.get("1.0", "end-1c").strip()
        if not text:
            problem.configure(text="")
            ctx.set(None)
            return
        try:
            ctx.set(json.loads(text))
            problem.configure(text="")
        except ValueError as error:
            problem.configure(text=str(error))

    box.bind("<KeyRelease>", changed)
    box.grid(row=0, column=0, sticky="ew")
    problem.grid(row=1, column=0, sticky="w")
    return row
