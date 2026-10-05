// Studio mode: the preview beside the programme, and the Take between them.
//
// Loaded the first time Studio mode is switched on, so a page that never uses
// it never downloads it. The panel keeps the programme monitor; this file owns
// the preview pane, the take bar and the keys that send preview to programme.
//
// Two things can be in preview. A scene is armed on the core with
// scene.preview.set, which every client sees and which the core composites
// while somebody watches. A single source is not something the core's preview
// holds, so it is armed on this page: its picture is its own tile out of the
// mosaic the programme monitor already has, and the take is
// `program.take {source}`. Arming a source disarms the scene, so there is one
// preview and not two, and arming a scene clears the source.
//
// After a take the preview keeps what it had, because that is what the core
// does with an armed scene, and a source is kept the same way so the two
// behave alike.

import { el, on } from "../../shell/dom.js";
import { settings } from "../../shell/settings.js";
import { toast, errorToast } from "../../shell/toast.js";
import { register } from "../../shell/commands.js";
import { releasePreview, retunePreview } from "./studio-picture.js";
import { render } from "./studio-armed.js";
import { picker } from "./transition-picker.js";
import { gallery, effects } from "./fx-gallery.js";

export { retunePreview, releasePreview, render };

/** The preview pane and the take bar, put into the panel's row once. */
export function build(panel) {
  if (panel.takeBar) return;
  document.head.appendChild(el("link", { rel: "stylesheet", href: new URL("./studio.css", import.meta.url).href }));

  panel.previewCanvas = el("canvas", { width: 320, height: 180 });
  panel.previewName = el("span.ellipsis");
  panel.previewNote = el("div.preview-note", { role: "status", hidden: true });
  panel.previewWrap = el("div.monitor-pane.preview-pane", {
    title: "Double click to take this to programme",
    ondblclick: () => take(panel, duration(panel)),
  }, [el("div.monitor-label", {}, [el("strong", { text: "PREVIEW" }), panel.previewName]), panel.previewNote, panel.previewCanvas]);

  panel.picker = picker(() => describe());
  panel.takeHow = el("span.take-how");
  panel.takeBtn = el("button.btn.primary.take-button", {
    title: "Send preview to programme with the transition below. Space does the same.",
    onclick: () => take(panel, duration(panel)),
  }, [el("span", { text: "Take" }), panel.takeHow]);
  panel.cutBtn = el("button.btn.cut-button", {
    text: "Cut",
    title: "Send preview to programme at once, with no transition",
    onclick: () => take(panel, 0),
  });
  // The fx library: a Looks panel to pick an imported transition, and a
  // row of effect buttons that play over the programme.
  panel.fx = panel.client ? gallery(panel.client, { pick: (name) => panel.picker.choose(name), armedScene: () => panel.armedScene || panel.client.state.preview || null }) : null;
  panel.effects = panel.client ? effects(panel.client) : null;
  panel.takeBar = el("div.take-bar", { role: "group", "aria-label": "Take" }, [
    panel.takeBtn, panel.cutBtn, panel.picker.el,
    ...(panel.fx ? [panel.fx.button, panel.fx.panel] : []),
    ...(panel.effects ? [panel.effects.el] : []),
  ]);
  function describe() {
    panel.takeHow.textContent = panel.picker.describe();
  }
  describe();
  // The collection's own transitions and the plugins', added to the list.
  if (panel.client) panel.picker.load(panel.client).then(describe);

  const programme = panel.row.querySelector(".program");
  panel.row.insertBefore(panel.previewWrap, programme);
  panel.row.insertBefore(panel.takeBar, programme);
  if (panel.effects) panel.offs.push(panel.effects.off);
  panel.offs.push(
    // A source tile armed in the Sources panel says so here at once, rather
    // than on the next state the core sends.
    on(document, "gmx-armed", () => panel.render(panel.client.state)),
    register({
      id: "program.take-armed",
      title: "Take the preview to programme",
      group: "Programme",
      key: "Space",
      enabled: () => settings().producer,
      run: () => take(panel, duration(panel)),
    }),
    register({
      id: "program.cut-armed",
      title: "Cut the preview to programme",
      group: "Programme",
      enabled: () => settings().producer,
      run: () => take(panel, 0),
    }),
  );
}

function duration(panel) {
  return panel.picker ? panel.picker.ms() : 500;
}

/** Shown in Studio mode and hidden, with its pictures let go, outside it. */
export function show(panel, producer) {
  if (!panel.takeBar) return;
  panel.previewWrap.hidden = panel.takeBar.hidden = !producer;
  panel.classList.toggle("studio", producer);
  if (!producer) releasePreview(panel);
}

/** Preview to programme: a cut with no length, the chosen transition with one. */
export async function take(panel, durationMs) {
  const target = panel.armed;
  if (!target) {
    toast({ text: "Nothing is in preview. Click a scene or a source first." });
    return;
  }
  const scene = panel.client.state.preview === target || panel.armedScene === target;
  const request = scene ? { scene: target } : { source: target };
  // Cut says so, so a default transition set with fx.assign does not turn
  // the Cut button into a Take.
  request.transition = durationMs ? (panel.picker ? panel.picker.request(durationMs) : { type: "fade", duration_ms: durationMs }) : "cut";
  try {
    await panel.client.call("program.take", request);
  } catch (e) {
    errorToast(e, "Take");
  }
}
