"""A plugin that ships a data schema and a designer block, and no code at all.

07 Phase 3 asks that such a plugin "gets an inspector and handles in the web
designer and the Tkinter example with no HTML in the plugin". This suite is the
Python half of that acceptance. The web half is in `ui/test/run.js`, and both
read the same file, `tests/fixtures/designer/lower-third.json`, which is what
`plugin.describe` answers for the plugin.

Nothing here needs Tk: the handles, the layout and the form are all data, and
the widgets are one function call further on. The two tests that do build
widgets skip themselves when `tkinter` cannot open a display, and say so.
"""

from __future__ import annotations

import json
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from godwinmix.kits import canvas as ckit  # noqa: E402
from godwinmix.kits import schema as skit  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
FIXTURE = os.path.join(ROOT, "tests", "fixtures", "designer", "lower-third.json")


def load():
    if not os.path.exists(FIXTURE):
        return None
    with open(FIXTURE, encoding="utf-8") as fh:
        return json.load(fh)


FIX = load()
WHY = "tests/fixtures/designer/lower-third.json is not in this tree"


def designer_block(described, type_id):
    """`[provides.designer]` for one provide, the way a surface finds it."""
    short = type_id.split("/")[-1]
    for provide in (described.get("manifest") or {}).get("provides") or []:
        if provide.get("id") in (type_id, short):
            return provide.get("designer")
    return None


@unittest.skipIf(FIX is None, WHY)
class DesignerFixtureTest(unittest.TestCase):
    def setUp(self):
        self.record = FIX["record"]
        self.type_id = self.record["content"]["graphic"]
        self.designer = designer_block(FIX["describe"], self.type_id)
        self.schema = FIX["describe"]["schemas"][self.type_id]

    def test_the_plugin_ships_no_editor_which_is_the_point_of_it(self):
        self.assertIsNotNone(self.designer)
        self.assertNotIn("editor", self.designer)
        self.assertNotIn("ui", self.designer)

    def test_the_handles_come_from_its_designer_block(self):
        gizmos = ckit.gizmos_for(self.designer)
        self.assertEqual([g["kind"] for g in gizmos], FIX["expect"]["gizmo_kinds"])

    def test_the_handles_land_on_the_item_s_own_box(self):
        box = FIX["box"]
        handles = ckit.handles_for(box, ckit.gizmos_for(self.designer))
        corners = [(h["x"], h["y"]) for h in handles if h["kind"] == "corner"]
        self.assertIn((box["x"], box["y"]), corners)
        self.assertIn((box["x"] + box["width"], box["y"] + box["height"]), corners)
        rotate = [h for h in handles if h["kind"] == "rotate"][0]
        self.assertLess(rotate["y"], box["y"], "the rotation handle sits above the item")

    def test_dragging_a_corner_answers_with_a_command_not_a_coordinate(self):
        box = FIX["box"]
        handles = ckit.handles_for(box, ckit.gizmos_for(self.designer))
        corner = [h for h in handles if h["kind"] == "corner" and h["dir"] == [1, 1]][0]
        out = ckit.apply_drag(corner, {"box": box, "transform": self.record["transform"]}, 40.0, 20.0, {})
        self.assertIn("transform", out["props"])
        self.assertAlmostEqual(out["props"]["transform"]["frame"]["w"], box["width"] + 40, places=2)
        self.assertAlmostEqual(out["props"]["transform"]["frame"]["h"], box["height"] + 20, places=2)

    def test_it_snaps_by_its_bounds_and_by_the_point_it_named(self):
        self.assertTrue(self.designer["snap"]["bounds"])
        self.assertEqual(self.designer["snap"]["points"], ["params.anchor_point"])
        # No anchor point on this instance, so it contributes none. A surface
        # asking for them gets an empty list rather than an error.
        self.assertEqual(ckit.points_of(self.record, FIX["box"], self.designer["snap"]), [])

    def test_every_property_gets_a_control_from_the_data_schema_alone(self):
        form = skit.describe_form(self.schema, self.record["content"]["params"])
        layout = skit.apply_rules(skit.layout_for(form, None), skit.values_of(form))
        names = [node.field.name for node in skit.controls_of(layout)]
        self.assertEqual(names, FIX["expect"]["controls"])

    def test_the_controls_are_the_ones_the_schema_asked_for(self):
        form = skit.describe_form(self.schema, self.record["content"]["params"])
        layout = skit.apply_rules(skit.layout_for(form, None), skit.values_of(form))
        by_name = {node.field.name: node.control for node in skit.controls_of(layout)}
        self.assertEqual(by_name["name"], "text")
        self.assertEqual(by_name["hold_secs"], "number")
        self.assertEqual(by_name["animate"], "boolean")
        self.assertEqual(by_name["align"], "select")
        self.assertEqual(by_name["font_size"], "integer")

    def test_the_form_carries_the_values_and_the_schema_s_defaults(self):
        form = skit.describe_form(self.schema, self.record["content"]["params"])
        values = skit.values_of(form)
        self.assertEqual(values["name"], "Jane Okonjo")
        self.assertEqual(values["hold_secs"], 6)
        self.assertEqual(values["colour"], "#2f6f4f")
        self.assertEqual(skit.read_form(form, values, set())["name"], "Jane Okonjo")

    def test_the_unit_and_the_group_survive_into_the_description(self):
        form = skit.describe_form(self.schema, {})
        fields = {f.name: f for f in form.fields}
        self.assertEqual(fields["hold_secs"].unit, "s")
        self.assertEqual(fields["font_size"].group, "Advanced")
        self.assertEqual(form.groups, ["", "Advanced"])


@unittest.skipIf(FIX is None, WHY)
class DesignerFixtureTkTest(unittest.TestCase):
    """The same fixture, as widgets. Skipped where Tk cannot open a display."""

    @classmethod
    def setUpClass(cls):
        try:
            import tkinter
        except Exception as e:  # pragma: no cover, depends on the interpreter
            raise unittest.SkipTest(f"tkinter is not importable: {e}")
        try:
            cls.root = tkinter.Tk()
            cls.root.withdraw()
        except Exception as e:  # pragma: no cover, depends on the display
            raise unittest.SkipTest(f"Tk cannot open a display: {e}")

    @classmethod
    def tearDownClass(cls):
        if getattr(cls, "root", None) is not None:
            cls.root.destroy()

    def test_an_inspector_is_built_with_no_html_anywhere(self):
        from godwinmix.kits.tkrender import inspector

        record = FIX["record"]
        schema = FIX["describe"]["schemas"][record["content"]["graphic"]]
        panel = inspector(self.root, schema=schema, value=record["content"]["params"])
        try:
            read = panel.read()
            self.assertEqual(read["name"], "Jane Okonjo")
            self.assertEqual(read["hold_secs"], 6)
            self.assertEqual(read["colour"], "#2f6f4f")
        finally:
            panel.destroy()

    def test_the_canvas_draws_the_handles_the_block_asked_for(self):
        import tkinter

        from godwinmix.kits.tkcanvas import DesignerCanvas

        canvas = tkinter.Canvas(self.root, width=640, height=360)
        painter = DesignerCanvas(canvas)
        try:
            designer = designer_block(FIX["describe"], FIX["record"]["content"]["graphic"])
            painter.set_scene(
                {"width": 1920, "height": 1080},
                [
                    {
                        "id": FIX["record"]["id"],
                        "box": FIX["box"],
                        "transform": FIX["record"]["transform"],
                        "label": FIX["record"]["name"],
                        "designer": designer,
                    }
                ],
            )
            painter.select(FIX["record"]["id"])
            painter.widget.update_idletasks()
            painter.draw()
            self.assertGreater(len(canvas.find_all()), 0, "nothing was drawn")
        finally:
            canvas.destroy()


if __name__ == "__main__":
    unittest.main()
