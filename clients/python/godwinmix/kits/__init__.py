"""The designer kits: the arithmetic a scene designer needs, in Python.

The reference implementation is the plain ES modules in `ui/kits/`. This
package is the same algorithms, with the same behaviour, checked against the
same file: `ui/kits/fixtures.json` records what the reference answered for
every case, and `clients/python/tests/test_kits.py` replays it here. A port
that drifts fails in its own language's test run, naming the case.

Three modules, and none of them imports `tkinter`:

============  =================================================================
protocol      the record mirror, the prediction buffer, the undo proxy
canvas        item boxes, handles, drags, snapping, safe areas
schema        the UI schema layer and the ranked renderer registry
============  =================================================================

Two more modules do import `tkinter`, which is why they are separate files and
are not imported here: :mod:`godwinmix.kits.tkrender` builds ttk widgets for a
layout, and :mod:`godwinmix.kits.tkcanvas` draws the geometry onto a
`tkinter.Canvas`. A headless script imports this package and never loads Tk.
"""

from __future__ import annotations

from .canvas import (
    ACTION_SAFE,
    ARCHETYPES,
    DEFAULT_GIZMOS,
    THRESHOLD,
    TITLE_SAFE,
    View,
    apply_drag,
    gizmos_for,
    handles_for,
    hit_test,
    inset_fraction,
    inside,
    item_rect,
    overlaps,
    path_props,
    path_value,
    points_of,
    rect_from,
    rect_to_transform,
    safe_areas,
    snap_delta,
    snap_targets,
    ticks,
    union_of,
)
from .protocol import (
    Prediction,
    SceneMirror,
    UndoProxy,
    echo_seq_of,
    geometry_index,
    merge_props,
)
from .schema import (
    BASE,
    CONTROLS,
    HOST,
    SPECIFIC,
    Field,
    Form,
    Node,
    Renderers,
    apply_conditions,
    apply_rules,
    control_for,
    controls_of,
    default_layout,
    describe_form,
    field_of_scope,
    layout_for,
    missing,
    read_form,
    register_table,
    values_of,
)

__all__ = [
    "ACTION_SAFE",
    "ARCHETYPES",
    "BASE",
    "CONTROLS",
    "DEFAULT_GIZMOS",
    "Field",
    "Form",
    "HOST",
    "Node",
    "Prediction",
    "Renderers",
    "SPECIFIC",
    "SceneMirror",
    "THRESHOLD",
    "TITLE_SAFE",
    "UndoProxy",
    "View",
    "apply_conditions",
    "apply_drag",
    "apply_rules",
    "control_for",
    "controls_of",
    "default_layout",
    "describe_form",
    "echo_seq_of",
    "field_of_scope",
    "geometry_index",
    "gizmos_for",
    "handles_for",
    "hit_test",
    "inset_fraction",
    "inside",
    "item_rect",
    "layout_for",
    "merge_props",
    "missing",
    "overlaps",
    "path_props",
    "path_value",
    "points_of",
    "rect_from",
    "read_form",
    "rect_to_transform",
    "register_table",
    "safe_areas",
    "snap_delta",
    "snap_targets",
    "ticks",
    "union_of",
    "values_of",
]
