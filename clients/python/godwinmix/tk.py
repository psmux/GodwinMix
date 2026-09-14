"""A plugin's settings as Tkinter widgets.

Optional: importing this pulls in `tkinter`, which a headless script does not
want and a Raspberry Pi image may not have. `import godwinmix` does not import
it; `from godwinmix.tk import SchemaForm` does.

    form = SchemaForm(parent, schema, current)
    form.grid(row=0, column=0, sticky="ew")
    ...
    await client.call("plugin.configure", {"id": plugin, "settings": form.read()})

The schema reading is all in :mod:`godwinmix.schema`; this file is the widgets
and nothing else, which is why it is short and why a Qt or a curses version of
it would be short too.
"""

from __future__ import annotations

import tkinter as tk
from tkinter import ttk
from typing import Any, Dict, List, Optional

from .schema import Field, Form, apply_conditions, describe_form, missing, read_form

SECRET_KEPT = "•" * 8


class SchemaForm(ttk.Frame):
    """Every property of a schema as a labelled control, in schema order."""

    def __init__(self, parent: tk.Misc, schema: Dict[str, Any], value: Optional[Dict[str, Any]] = None):
        super().__init__(parent)
        self.form: Form = describe_form(schema, value)
        self.vars: Dict[str, Any] = {}
        self.rows: Dict[str, List[tk.Widget]] = {}
        self.touched_secrets: set = set()
        self.columnconfigure(1, weight=1)
        self._build()
        self._refresh()

    # ------------------------------------------------------------------ read

    def read(self) -> Dict[str, Any]:
        """The object to send: hidden fields out, secrets untouched left alone."""
        return read_form(self.form, self.values(), self.touched_secrets)

    def values(self) -> Dict[str, Any]:
        """Every control's current value, hidden ones included."""
        return {name: var.get() for name, var in self.vars.items()}

    def validate(self) -> bool:
        """Mark the empty required fields and say whether the form is usable."""
        empty = set(missing(self.form, self.values()))
        for name, widgets in self.rows.items():
            for widget in widgets:
                if isinstance(widget, ttk.Label) and widget.cget("text").startswith("!"):
                    widget.grid_remove()
            if name in empty:
                widgets[-1].grid()
        return not empty

    # ----------------------------------------------------------------- build

    def _build(self) -> None:
        row = 0
        for group in self.form.groups:
            fields = [f for f in self.form.fields if f.group == group]
            if group:
                heading = ttk.Label(self, text=group, font=("TkDefaultFont", 10, "bold"))
                heading.grid(row=row, column=0, columnspan=2, sticky="w", pady=(10, 2))
                row += 1
            for field in fields:
                row = self._add(field, row)

    def _add(self, field: Field, row: int) -> int:
        text = field.label + (f"  ({field.unit})" if field.unit else "")
        label = ttk.Label(self, text=text)
        label.grid(row=row, column=0, sticky="w", padx=(0, 8), pady=2)
        control = self._control(field)
        control.grid(row=row, column=1, sticky="ew", pady=2)
        widgets: List[tk.Widget] = [label, control]
        row += 1
        if field.description:
            hint = ttk.Label(self, text=field.description, wraplength=320, foreground="#666")
            hint.grid(row=row, column=1, sticky="w")
            widgets.append(hint)
            row += 1
        warning = ttk.Label(self, text="! this one is needed", foreground="#b00")
        warning.grid(row=row, column=1, sticky="w")
        warning.grid_remove()
        widgets.append(warning)
        self.rows[field.name] = widgets
        return row + 1

    def _control(self, field: Field) -> tk.Widget:
        changed = lambda *_: self._refresh()  # noqa: E731
        if field.kind == "boolean":
            var = tk.BooleanVar(value=bool(field.value))
            var.trace_add("write", changed)
            self.vars[field.name] = var
            return ttk.Checkbutton(self, variable=var)

        if field.kind == "choice":
            var = tk.StringVar(value="" if field.value is None else str(field.value))
            var.trace_add("write", changed)
            self.vars[field.name] = var
            box = ttk.Combobox(self, textvariable=var, state="readonly")
            box["values"] = [str(c) for c in (field.choices or [])]
            return box

        if field.kind == "lines":
            var = tk.StringVar(value="\n".join(str(v) for v in (field.value or [])))
            var.trace_add("write", changed)
            self.vars[field.name] = var
            return ttk.Entry(self, textvariable=var)

        secret = field.kind == "secret"
        start = SECRET_KEPT if (secret and field.value) else ("" if field.value is None else str(field.value))
        var = tk.StringVar(value=start)
        var.trace_add("write", changed)
        self.vars[field.name] = var
        entry = ttk.Entry(self, textvariable=var, show="*" if secret else "")
        if secret:
            entry.bind("<Key>", lambda _e, name=field.name: self.touched_secrets.add(name))
        return entry

    def _refresh(self) -> None:
        """Re-run the if/then rules and show or hide the rows they name."""
        apply_conditions(self.form, self.values())
        for field in self.form.fields:
            widgets = self.rows.get(field.name) or []
            for widget in widgets[:-1]:
                if field.visible:
                    widget.grid()
                else:
                    widget.grid_remove()
            if not field.visible:
                widgets[-1].grid_remove()
