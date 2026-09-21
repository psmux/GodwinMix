// The inspector: what the selected item is, and everything about it you can
// change without touching the canvas.
//
// Three parts, top to bottom. The item's own properties, which every item has:
// name, opacity, fit, blend, sound and the numbers of its box. The plugin's
// settings, rendered through the fallback chain in the schema kit, so a plugin
// that ships a data schema and a `designer` block gets a real editor with no
// HTML anywhere in it. Then its filters, which hang on the item rather than on
// the source, so a camera keyed in one scene is not keyed in all of them.

import { el, clear, on } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { editorFor } from "../../kits/schema/index.js";
import { rectToTransform } from "../../kits/canvas/geometry.js";
import { itemProps, filters, filterTypes, BLENDS, AUDIO, FITS } from "./ops.js";

export class Inspector {
  /**
   * @param {{client, scenes, catalogue, context: () => object,
   *          geometry?: (itemId: string) => ({x, y, width, height}|undefined),
   *          onChanged?: Function}} opts
   */
  constructor(opts) {
    this.o = opts;
    this.el = el("div.composer-inspector.col");
    this.showing = null;
  }

  /** Draw for the current selection. Called on every selection change. */
  async show(records) {
    const record = records && records.length === 1 ? records[0] : null;
    // A rebuild while somebody is typing into this panel would take the focus
    // and the half typed word with it. The core's echo of their own edit is
    // exactly when that would happen, so the same item with the hand still in
    // it is left alone.
    if (record && record.id === this.showing && this.el.contains(document.activeElement)) return;
    clear(this.el);
    if (!record) {
      this.showing = null;
      this.el.appendChild(
        el("div.dim.sm.pad", {
          text: records && records.length
            ? `${records.length} items selected. The arrange buttons work on all of them.`
            : "Nothing selected. Click an item on the canvas, or sweep over several.",
        })
      );
      return;
    }
    this.showing = record.id;
    this.el.append(this.itemSection(record), el("div.composer-sep"));
    await this.pluginSection(record);
    this.el.append(el("div.composer-sep"), await this.filterSection(record));
  }

  // ------------------------------------------------------------ the item

  itemSection(record) {
    const props = itemProps(this.o.scenes, this.o.context);
    // The answer goes on to the composer. A change to a draft is in nothing
    // but its answer, which carries the new records and geometry, and this
    // used to drop it: a number typed here reached the mixer and the canvas
    // went on drawing the old box.
    const guard = (fn) => (value) => Promise.resolve(fn(value)).then((answer) => this.changed(answer)).catch((e) => errorToast(e, "Change"));

    const name = el("input", { type: "text", value: record.name || "" });
    on(name, "change", () => guard(props.name)(name.value));

    const opacity = el("input", { type: "range", min: "0", max: "1", step: "0.01", value: String(record.opacity ?? 1) });
    const opacityOut = el("span.num.sm", { text: pct(record.opacity ?? 1) });
    on(opacity, "input", () => {
      opacityOut.textContent = pct(Number(opacity.value));
    });
    on(opacity, "change", () => guard(props.opacity)(opacity.value));

    const visible = el("input", { type: "checkbox", checked: record.visible !== false });
    on(visible, "change", () => guard(props.visible)(visible.checked));
    const locked = el("input", { type: "checkbox", checked: !!record.locked });
    on(locked, "change", () => guard(props.locked)(locked.checked));

    // The box an item sits in is not on the record. It is flattened geometry
    // the core answers with, so a mixer without it gets no group rather than a
    // guess. A null child is skipped by `el`, which keeps the row below it in
    // place either way.
    const box = this.o.geometry ? this.o.geometry(record.id) : null;
    const geometry = box ? field("Position and size", this.geometrySection(record, box, guard)) : null;

    return el("div.form.pad", {}, [
      field("Name", name),
      geometry,
      field("Opacity", el("div.row", {}, [opacity, opacityOut])),
      field("Fit", choice(FITS, (record.transform && record.transform.fit) || "none", guard(props.fit))),
      field("Blend", choice(BLENDS, record.blend || "normal", guard(props.blend))),
      field("Sound", choice(AUDIO, record.audio || "follow", guard(props.audio))),
      el("div.row", {}, [
        el("label.inline", {}, [visible, el("span.sm", { text: "Visible" })]),
        el("label.inline", {}, [locked, el("span.sm", { text: "Locked" })]),
      ]),
    ]);
  }

  // position and size

  /**
   * The item's box in canvas pixels, and the quarter turns the mixer can make,
   * because those are the only turns `videoflip` offers (see `set_rotation` in
   * the core). One `scene.item.set` per edit, from all four numbers at once, so
   * the rectangle is never half of one edit and half of another.
   *
   * The box arrives from the caller, taken from the flattened geometry, and the
   * change goes back through `rectToTransform` with the item's own scale and
   * anchor so the numbers round trip. There is nothing here that a mixer
   * without geometry can draw, which is why absence is handled by not being
   * called.
   */
  geometrySection(record, box, guard) {
    const inputs = {
      x: numberInput("X", box.x),
      y: numberInput("Y", box.y),
      w: numberInput("W", box.width),
      h: numberInput("H", box.height),
    };
    const wanted = () => ({
      x: whole(inputs.x.value, whole(box.x, 0)),
      y: whole(inputs.y.value, whole(box.y, 0)),
      width: whole(inputs.w.value, whole(box.width, 0)),
      height: whole(inputs.h.value, whole(box.height, 0)),
    });
    const restore = () => {
      inputs.x.value = String(whole(box.x, 0));
      inputs.y.value = String(whole(box.y, 0));
      inputs.w.value = String(whole(box.width, 0));
      inputs.h.value = String(whole(box.height, 0));
    };
    const commit = () => {
      const rect = wanted();
      // The core refuses a box below this and would answer with an error. The
      // old value goes back instead, which is the state that is actually true.
      if (rect.width < 8 || rect.height < 8) {
        restore();
        return;
      }
      guard(() => {
        const c = this.o.context();
        return this.o.scenes.itemSet(
          c.scene,
          c.items[0],
          { transform: rectToTransform(rect, record.transform || {}) },
          { duration_ms: 0, draft: c.draft }
        ).then((answer) => {
          // What was sent is the box now, so a later refusal puts back these
          // numbers and not the ones the panel opened with.
          Object.assign(box, rect);
          return answer;
        });
      })();
    };
    for (const input of Object.values(inputs)) on(input, "change", commit);

    // The panel does not rebuild while the hand is still in it, so the angle
    // beside the buttons is kept here as well as on the record.
    let current = quarter(rotationOf(record));
    const shown = el("span.num.sm", { text: degrees(current) });
    const turn = (value) => {
      guard(() => {
        const c = this.o.context();
        return this.o.scenes.itemSet(
          c.scene,
          c.items[0],
          { transform: { rotation: value } },
          { duration_ms: 0, draft: c.draft }
        ).then((answer) => {
          current = value;
          shown.textContent = degrees(value);
          return answer;
        });
      })();
    };

    return el("div.col", {}, [
      el("div.row", {}, [numberField("X", inputs.x), numberField("Y", inputs.y)]),
      el("div.row", {}, [numberField("W", inputs.w), numberField("H", inputs.h)]),
      el("div.row.composer-rotate", {}, [
        el("button.btn.sm", { text: "Rotate left", title: "A quarter turn anticlockwise", onclick: () => turn(quarter(current - 90)) }),
        el("button.btn.sm", { text: "Rotate right", title: "A quarter turn clockwise", onclick: () => turn(quarter(current + 90)) }),
        el("button.btn.sm", { text: "Reset rotation", onclick: () => turn(0) }),
        shown,
      ]),
    ]);
  }

  // ---------------------------------------------------------- the plugin

  /**
   * The plugin's own settings, through the fallback chain: its editor element,
   * else its UI schema, else its data schema, else the values themselves.
   */
  async pluginSection(record) {
    const box = el("div.col");
    this.el.appendChild(box);
    await this.o.catalogue.load();
    const type = this.o.catalogue.typeOf(record);
    if (!type) {
      box.appendChild(el("div.dim.sm.pad", { text: "This item has no plugin behind it, so there is nothing else to set." }));
      return;
    }
    const content = record.content || {};
    const value = content.params || {};
    const editor = await editorFor({
      plugin: type.plugin,
      designer: type.designer,
      schema: type.schema,
      value,
      onChange: null,
    });
    const apply = el("button.btn.primary", {
      text: "Apply settings",
      onclick: async () => {
        try {
          await this.o.scenes.itemSet(this.o.context().scene, record.id, { content: Object.assign({}, content, { params: editor.read() }) }, {
            duration_ms: 0,
            draft: this.o.context().draft,
          });
          this.changed();
        } catch (e) {
          errorToast(e, "Settings");
        }
      },
    });
    box.append(
      el("div.row.pad", {}, [el("strong.sm.grow", { text: type.title }), el("span.sm.faint", { text: type.plugin })]),
      el("div.pad", {}, [editor.el]),
      el("div.row.pad", {}, [el("span.sm.faint.grow", { text: editor.why }), apply])
    );
  }

  // --------------------------------------------------------- the filters

  async filterSection(record) {
    const api = filters(this.o.scenes, this.o.context);
    const list = el("div.col.pad");
    for (const [i, filter] of (record.filters || []).entries()) {
      const enabled = el("input", { type: "checkbox", checked: filter.enabled !== false });
      on(enabled, "change", () =>
        api.set(filter.name || filter.type || i, undefined, enabled.checked).then(() => this.changed()).catch((e) => errorToast(e, "Filter"))
      );
      list.appendChild(
        el("div.row", {}, [
          el("label.inline.grow", {}, [enabled, el("span.sm", { text: filter.name || filter.type })]),
          el("button.btn.icon", {
            text: "×",
            title: "Remove this filter",
            onclick: () => api.remove(filter.name || filter.type || i).then(() => this.changed()).catch((e) => errorToast(e, "Filter")),
          }),
        ])
      );
    }
    if (!(record.filters || []).length) {
      list.appendChild(el("div.dim.sm", { text: "No filters on this item." }));
    }

    const picker = el("select", { "aria-label": "Add a filter" });
    picker.appendChild(el("option", { value: "", text: "Add a filter" }));
    // Asked once for the life of the composer: the filter list does not change
    // while somebody is arranging a scene, and the inspector is rebuilt often.
    if (!this.filterTypes) this.filterTypes = await filterTypes(this.o.client);
    for (const type of this.filterTypes) {
      picker.appendChild(el("option", { value: type.id, text: type.title }));
    }
    on(picker, "change", async () => {
      const type = picker.value;
      picker.value = "";
      if (!type) return;
      try {
        await api.add(type, {});
        this.changed();
      } catch (e) {
        errorToast(e, "Filter");
      }
    });

    return el("div.col", {}, [el("div.row.pad", {}, [el("strong.sm.grow", { text: "Filters" }), picker]), list]);
  }

  changed(answer) {
    if (this.o.onChanged) this.o.onChanged(answer);
  }
}

function field(label, control) {
  return el("div.field", {}, [el("span.lbl", { text: label }), control]);
}

function choice(values, current, onPick) {
  const select = el("select");
  for (const value of values) select.appendChild(el("option", { value, text: value }));
  select.value = current;
  on(select, "change", () => onPick(select.value));
  return select;
}

function pct(v) {
  return `${Math.round(v * 100)}%`;
}

function numberInput(label, value) {
  return el("input", { type: "number", step: "1", value: String(whole(value, 0)), "aria-label": label });
}

function numberField(label, input) {
  return el("label.inline.grow", {}, [el("span.sm.faint", { text: label }), input]);
}

/** A whole number, or the fallback when the box holds something else. */
function whole(value, fallback) {
  const n = Math.round(Number(value));
  return isFinite(n) ? n : fallback === undefined ? 0 : fallback;
}

/** The nearest quarter turn, kept on 0, 90, 180 or 270. */
function quarter(value) {
  return (((Math.round(value / 90) % 4) + 4) % 4) * 90;
}

function rotationOf(record) {
  const rotation = record.transform && record.transform.rotation;
  return typeof rotation === "number" && isFinite(rotation) ? rotation : 0;
}

function degrees(value) {
  return `${Math.round(value)}°`;
}
