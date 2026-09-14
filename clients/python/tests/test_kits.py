"""The designer kits, replayed against the reference implementation's answers.

`ui/kits/fixtures.json` is written by `dev/kit-fixtures.mjs`: it runs the plain
modules in `ui/kits/` over a list of inputs and records what they answered. The
TypeScript suite replays the same file, and so does this one. Three ports of one
algorithm drift; this is what stops them, and a case that disagrees fails here
by name.

Floats are rounded to two decimal places before they are compared, exactly the
way the generator does, so a port that computes in a different order is not
failed by the last bit of a double.

The whole suite skips itself when `ui/kits/fixtures.json` is not there, which is
the case when this package is installed from PyPI with no repository around it.
"""

from __future__ import annotations

import json
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from godwinmix.kits import canvas as ckit  # noqa: E402
from godwinmix.kits import protocol as pkit  # noqa: E402
from godwinmix.kits import schema as skit  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
FIXTURES = os.path.join(ROOT, "ui", "kits", "fixtures.json")


def load():
    if not os.path.exists(FIXTURES):
        return None
    with open(FIXTURES, encoding="utf-8") as fh:
        return json.load(fh)


CASES = load()
WHY = "ui/kits/fixtures.json is not in this tree"


def r2(value):
    """Two decimal places, the way `dev/kit-fixtures.mjs` rounds."""
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return value
    return ckit._js_round(value * 100) / 100


def rounded(value):
    """`r2` over a whole structure, leaving keys and strings alone."""
    if isinstance(value, list):
        return [rounded(v) for v in value]
    if isinstance(value, dict):
        return {k: rounded(v) for k, v in value.items()}
    return r2(value)


#: The sections this file replays. `note` is prose, not a case list.
COVERED = {"mirror", "predict", "merge", "gizmos", "handles", "drag", "snap", "safe", "schema", "read", "uiSchema"}


@unittest.skipIf(CASES is None, WHY)
class CoverageTest(unittest.TestCase):
    """A section nobody replays is a section that can drift unnoticed."""

    def test_every_section_of_the_file_is_replayed_here(self):
        sections = {k for k in CASES if k != "note"}
        self.assertEqual(sections, COVERED, "a fixture section has appeared or gone; add or drop a case class")

    def test_no_section_is_empty(self):
        for name in COVERED:
            with self.subTest(name):
                self.assertTrue(CASES[name], f"{name} has no cases")


@unittest.skipIf(CASES is None, WHY)
class MirrorTest(unittest.TestCase):
    """Views land, patches apply, echoes are named and gaps are reported."""

    def test_every_case(self):
        for case in CASES["mirror"]:
            with self.subTest(case["name"]):
                mirror = pkit.SceneMirror(client_id=case.get("clientId"))
                steps = []
                for step in case["steps"]:
                    if step.get("view"):
                        mirror.apply_view(step["view"])
                        steps.append({"kind": "view"})
                        continue
                    answer = mirror.apply_patch(step.get("patch"))
                    steps.append(
                        {
                            "kind": "patch",
                            "applied": answer["applied"],
                            "echo": answer["echo"],
                            "gap": answer["gap"],
                            "seq": answer["seq"],
                        }
                    )
                scenes = mirror.scenes()
                ours = {
                    "steps": steps,
                    "seq": mirror.seq,
                    "unknown": mirror.unknown,
                    "scenes": [s["id"] for s in scenes],
                    "items": {s["id"]: [i["id"] for i in mirror.items(s["id"])] for s in scenes},
                    "names": {r["id"]: r.get("name") for r in mirror.records.values()},
                }
                self.assertEqual(ours, case["out"])

    def test_scene_of_follows_parents_up(self):
        mirror = pkit.SceneMirror()
        mirror.apply_view(
            {
                "id": "s1",
                "canvas": {"width": 1920, "height": 1080},
                "records": [
                    {"id": "s1", "kind": "scene", "name": "wide", "order": "a0"},
                    {"id": "g1", "kind": "item", "parent": "s1", "order": "a0"},
                    {"id": "i1", "kind": "item", "parent": "g1", "order": "a0"},
                ],
            }
        )
        self.assertEqual(mirror.scene_of("i1"), "s1")
        self.assertEqual([r["id"] for r in mirror.descendants("s1")], ["g1", "i1"])

    def test_geometry_index_is_keyed_by_item(self):
        view = {"geometry": [{"item": "i1", "x": 0, "y": 0, "width": 8, "height": 4}]}
        self.assertEqual(pkit.geometry_index(view)["i1"]["width"], 8)


@unittest.skipIf(CASES is None, WHY)
class PredictTest(unittest.TestCase):
    """A drag draws locally and snaps to the core only when the numbers meet."""

    def test_every_case(self):
        for case in CASES["predict"]:
            with self.subTest(case["name"]):
                prediction = pkit.Prediction()
                seqs = []
                settled = []
                for op in case["ops"]:
                    if op[0] == "predict":
                        seqs.append(prediction.predict(op[1], op[2]))
                    else:
                        settled.append(prediction.settle(op[1]))
                resolved = {
                    item: prediction.resolve(item, server)
                    for item, server in (case.get("resolve") or {}).items()
                }
                ours = {
                    "seqs": seqs,
                    "settled": settled,
                    "acked": prediction.acked,
                    "pending": sorted(prediction.pending),
                    "resolved": resolved,
                    "accepted": [prediction.accepts(item, echo) for item, echo in (case.get("accepts") or [])],
                }
                self.assertEqual(rounded(ours), rounded(case["out"]))

    def test_busy_says_whether_anything_is_in_flight(self):
        prediction = pkit.Prediction()
        self.assertFalse(prediction.busy)
        prediction.predict("i1", {"opacity": 0.5})
        self.assertTrue(prediction.busy)
        prediction.reset()
        self.assertFalse(prediction.busy)

    def test_the_echo_number_is_read_under_every_spelling(self):
        self.assertEqual(pkit.echo_seq_of({"client_seq": 4}), 4)
        self.assertEqual(pkit.echo_seq_of({"echo_seq": 5}), 5)
        self.assertEqual(pkit.echo_seq_of({"seq_echo": 6}), 6)
        self.assertEqual(pkit.echo_seq_of({}), 0)
        self.assertEqual(pkit.echo_seq_of(None), 0)


@unittest.skipIf(CASES is None, WHY)
class MergeTest(unittest.TestCase):
    """Props merge the way `scene.item.set` merges them, or the frames differ."""

    def test_every_case(self):
        for case in CASES["merge"]:
            with self.subTest(case["name"]):
                self.assertEqual(pkit.merge_props(case["base"], case["next"]), case["out"])

    def test_neither_argument_is_modified(self):
        base = {"transform": {"position": {"x": 1}}}
        pkit.merge_props(base, {"transform": {"position": {"x": 9}}})
        self.assertEqual(base["transform"]["position"]["x"], 1)


@unittest.skipIf(CASES is None, WHY)
class GizmoTest(unittest.TestCase):
    """Handles are data: what the manifest says, expanded."""

    def test_every_case(self):
        for case in CASES["gizmos"]:
            with self.subTest(case["name"]):
                gizmos = ckit.gizmos_for(case["designer"])
                ours = [
                    {"kind": g["kind"], "action": g.get("action"), "target": g.get("target"), "anchor": g["anchor"]}
                    for g in gizmos
                ]
                self.assertEqual(ours, case["out"])

    def test_handles_land_where_the_reference_puts_them(self):
        for case in CASES["handles"]:
            with self.subTest(case["name"]):
                handles = ckit.handles_for(case["box"], ckit.gizmos_for(case["designer"]))
                ours = [
                    {
                        "kind": h["kind"],
                        "action": h["action"],
                        "x": r2(h["x"]),
                        "y": r2(h["y"]),
                        "dir": h["dir"],
                        "cursor": h["cursor"],
                    }
                    for h in handles
                ]
                self.assertEqual(ours, case["out"])

    def test_the_cage_is_never_the_handle_a_click_finds(self):
        handles = ckit.handles_for({"x": 0, "y": 0, "width": 100, "height": 100}, ckit.DEFAULT_GIZMOS)
        self.assertIsNone(ckit.hit_test(handles, 50, 50, 4))
        self.assertEqual(ckit.hit_test(handles, 0, 0, 4)["kind"], "corner")


@unittest.skipIf(CASES is None, WHY)
class DragTest(unittest.TestCase):
    """A drag on one handle, as the props to assign and the box to draw."""

    def test_every_case(self):
        for case in CASES["drag"]:
            with self.subTest(case["name"]):
                result = ckit.apply_drag(case["handle"], case["start"], case["dx"], case["dy"], case.get("mods") or {})
                ours = {
                    "props": rounded(result["props"]),
                    "box": rounded(result["box"]) if result["box"] else None,
                }
                self.assertEqual(ours, rounded(case["out"]))

    def test_a_path_becomes_a_nest_and_reads_back(self):
        props = ckit.path_props("params.key_tolerance", 0.25)
        self.assertEqual(props, {"params": {"key_tolerance": 0.25}})
        self.assertEqual(ckit.path_value(props, "params.key_tolerance"), 0.25)
        self.assertIsNone(ckit.path_value(props, "params.missing"))


@unittest.skipIf(CASES is None, WHY)
class SnapTest(unittest.TestCase):
    """The nudge that lines a box up, and the guides drawn while it does."""

    def test_every_case(self):
        for case in CASES["snap"]:
            with self.subTest(case["name"]):
                targets = ckit.snap_targets(case["targets"])
                delta = ckit.snap_delta(case["box"], targets, case.get("opts") or {})
                ours = {
                    "dx": r2(delta["dx"]),
                    "dy": r2(delta["dy"]),
                    "guides": [
                        {"axis": g["axis"], "at": r2(g["at"]), "span": [r2(s) for s in g["span"]]}
                        for g in delta["guides"]
                    ],
                }
                self.assertEqual(ours, case["out"])

    def test_a_named_point_is_a_fraction_of_the_item_box(self):
        item = {"params": {"anchor_point": {"x": 0.5, "y": 1}}}
        box = {"x": 100, "y": 100, "width": 200, "height": 100}
        points = ckit.points_of(item, box, {"points": ["params.anchor_point"]})
        self.assertEqual(points, [{"x": 200, "y": 200}])


@unittest.skipIf(CASES is None, WHY)
class SafeTest(unittest.TestCase):
    """The same two numbers `scene.validate` warns about."""

    def test_every_case(self):
        for case in CASES["safe"]:
            with self.subTest(str(case["canvas"])):
                self.assertEqual(rounded(ckit.safe_areas(case["canvas"])), rounded(case["out"]))

    def test_ticks_step_up_as_the_view_zooms_out(self):
        self.assertEqual(ckit.ticks(400, 1, 80)[:3], [0, 100, 200])
        self.assertEqual(ckit.ticks(400, 8, 80)[:3], [0, 10, 20])


@unittest.skipIf(CASES is None, WHY)
class SchemaTest(unittest.TestCase):
    """The data schema reader, read through the kit rather than ported again."""

    def test_every_case(self):
        for case in CASES["schema"]:
            with self.subTest(case["name"]):
                form = skit.describe_form(case["schema"], case.get("value") or {})
                ours = {
                    "groups": form.groups,
                    "fields": [
                        {
                            "name": f.name,
                            "kind": f.kind,
                            "group": f.group,
                            "required": f.required,
                            "visible": f.visible,
                            "unit": f.unit,
                            "value": f.value,
                            "choices": list(f.choices) if f.choices else None,
                        }
                        for f in form.fields
                    ],
                }
                self.assertEqual(ours, case["out"])


@unittest.skipIf(CASES is None, WHY)
class ReadTest(unittest.TestCase):
    """What a form sends back: coerced, with the hidden and the untouched left out."""

    def test_every_case(self):
        for case in CASES["read"]:
            with self.subTest(case["name"]):
                form = skit.describe_form(case["schema"], case.get("value") or {})
                values = dict(skit.values_of(form))
                values.update(case.get("values") or {})
                ours = {
                    "read": skit.read_form(form, values, case.get("touched") or []),
                    "missing": skit.missing(form, values),
                }
                self.assertEqual(ours, case["out"])


@unittest.skipIf(CASES is None, WHY)
class UiSchemaTest(unittest.TestCase):
    """A data schema and a UI schema, merged into a layout with its rules run."""

    def test_every_case(self):
        for case in CASES["uiSchema"]:
            with self.subTest(case["name"]):
                form = skit.describe_form(case["schema"], case.get("value") or {})
                layout = skit.apply_rules(skit.layout_for(form, case["ui"]), skit.values_of(form))
                ours = {
                    "controls": [
                        {
                            "field": n.field.name,
                            "control": n.control,
                            "visible": n.visible is not False,
                            "enabled": n.enabled is not False,
                        }
                        for n in skit.controls_of(layout)
                    ]
                }
                self.assertEqual(ours, case["out"])

    def test_a_forgotten_field_is_appended_rather_than_dropped(self):
        form = skit.describe_form({"type": "object", "properties": {"a": {"type": "string"}, "b": {"type": "string"}}})
        layout = skit.layout_for(form, {"type": "vertical", "elements": [{"scope": "#/properties/a"}]})
        self.assertEqual([n.field.name for n in skit.controls_of(layout)], ["a", "b"])

    def test_an_unknown_control_falls_back_to_what_the_data_schema_says(self):
        form = skit.describe_form({"type": "object", "properties": {"a": {"type": "integer"}}})
        layout = skit.layout_for(form, {"type": "control", "scope": "#/properties/a", "control": "hologram"})
        self.assertEqual(skit.controls_of(layout)[0].control, "integer")


@unittest.skipIf(CASES is None, WHY)
class RegistryTest(unittest.TestCase):
    """The ranked tester registry: the highest number wins, ties go to the last."""

    def _layout(self):
        form = skit.describe_form({"type": "object", "properties": {"a": {"type": "string"}}})
        return skit.layout_for(form, None)

    def test_the_highest_rank_wins_and_a_tie_goes_to_the_later_one(self):
        registry = skit.Renderers()
        skit.register_table(registry, {"text": lambda n, c: "kit"}, skit.BASE)
        skit.register_table(registry, {"text": lambda n, c: "host"}, skit.BASE)
        self.assertEqual(registry.explain(self._layout()), {"a": "text"})
        picked = registry.pick(skit.controls_of(self._layout())[0])
        self.assertEqual(picked.entry.make(None, None), "host")

    def test_a_removed_renderer_is_not_asked_again(self):
        registry = skit.Renderers()
        off = skit.register_table(registry, {"text": lambda n, c: "kit"}, skit.BASE)
        off()
        self.assertEqual(registry.explain(self._layout()), {"a": None})

    def test_a_renderer_needs_both_halves(self):
        registry = skit.Renderers()
        with self.assertRaises(TypeError):
            registry.register("broken", None, lambda n, c: None)


class TkWidgetTest(unittest.TestCase):
    """The two Tkinter modules, built and torn down once.

    Skipped where Tk is not compiled in or no display can be opened, which is
    every CI runner and most Raspberry Pi images. The arithmetic underneath
    them is covered by the fixtures above; this only checks that the widgets
    are wired to it correctly.
    """

    @classmethod
    def setUpClass(cls):
        try:
            import tkinter
        except ImportError as error:  # no _tkinter in this build
            raise unittest.SkipTest(f"tkinter is not importable: {error}")
        try:
            cls.root = tkinter.Tk()
        except Exception as error:  # no display, or no window server
            raise unittest.SkipTest(f"Tk will not open a window here: {error}")
        cls.root.withdraw()

    @classmethod
    def tearDownClass(cls):
        cls.root.destroy()

    def test_an_inspector_builds_from_a_schema_and_reads_back(self):
        from godwinmix.kits.tkrender import inspector

        schema = {
            "type": "object",
            "properties": {
                "name": {"type": "string", "title": "Name"},
                "gain": {"type": "number", "minimum": 0, "maximum": 1, "x-gmx-unit": "dB"},
                "loop": {"type": "boolean"},
                "codec": {"type": "string", "enum": ["h264", "hevc"]},
                "extra": {"type": "object"},
            },
        }
        ui = {
            "type": "vertical",
            "elements": [
                {"type": "group", "label": "Look", "elements": [{"scope": "#/properties/gain", "control": "slider"}]},
                {"scope": "#/properties/name"},
            ],
        }
        panel = inspector(self.root, schema, ui=ui, value={"name": "pulpit", "gain": 0.5})
        try:
            self.assertEqual(panel.read()["name"], "pulpit")
            panel.set("name", "stage")
            self.assertEqual(panel.read()["name"], "stage")
        finally:
            panel.destroy()

    def test_the_painter_draws_handles_and_a_drag_answers_with_props(self):
        import tkinter

        from godwinmix.kits.tkcanvas import DesignerCanvas

        widget = tkinter.Canvas(self.root, width=320, height=180)
        widget.pack()
        self.root.update_idletasks()
        sent = []
        painter = DesignerCanvas(widget, on_drag=lambda i, props, box: sent.append((i, props)))
        painter.set_scene(
            {"width": 1920, "height": 1080},
            [
                {
                    "id": "i1",
                    "box": {"x": 100, "y": 100, "width": 400, "height": 200},
                    "transform": {"position": {"x": 100, "y": 100}},
                    "designer": None,
                }
            ],
        )
        try:
            painter.select("i1")
            painter.draw()
            self.assertTrue(widget.find_withtag("gmx-overlay"))
            self.assertEqual([h["kind"] for h in painter.handles][0], "cage")

            # A press on the body picks the cage up, and a move sends a position.
            start = painter.view.to_surface(200, 150)
            painter._press(_Press(int(start[0]), int(start[1])))
            self.assertIsNotNone(painter._drag)
            moved = painter.view.to_surface(260, 150)
            painter._motion(_Press(int(moved[0]), int(moved[1])))
            painter._release(_Press(0, 0))
            self.assertTrue(sent)
            self.assertIn("transform", sent[-1][1])
        finally:
            widget.destroy()


class _Press:
    """The two fields the painter reads off a Tk event, without Tk generating one."""

    def __init__(self, x: int, y: int, state: int = 0):
        self.x = x
        self.y = y
        self.state = state


if __name__ == "__main__":
    unittest.main()
