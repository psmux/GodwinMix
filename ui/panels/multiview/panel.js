// The programme monitor, and the producer's preview beside it.
//
// One picture is always on: this one. It is the only thing on the page that
// subscribes to the multiview stream regardless of what the gallery is doing,
// because the operator has to be able to see what the audience sees on any
// machine. Everything else steps down.
//
// It still costs nothing when nobody is looking: the subscription is released
// when the tab is hidden and when the monitor scrolls out of view, and it is
// taken at the monitor's own pixel size, never upscaled.

import { el, on } from "../../shell/dom.js";
import { sheetWidthFor } from "../../client/frames.js";
import { programLabel } from "../../client/store.js";
import { settings, setSetting, onSettingsChanged } from "../../shell/settings.js";
import { errorToast } from "../../shell/toast.js";
import { mosaicWanted } from "./wanted.js";

class ProgramPanel extends HTMLElement {
  static get panel() {
    return { id: "core/program", title: "Programme", slots: ["monitor"], tag: "gmx-program" };
  }

  setClient(client) {
    this.client = client;
  }

  connectedCallback() {
    if (this.built) return;
    this.built = true;
    this.style.display = "block";

    this.canvas = el("canvas", { width: 640, height: 360, style: { width: "100%", display: "block", background: "#000" } });
    this.still = el("img", { alt: "", hidden: true, style: { width: "100%", display: "block", background: "#000" } });
    this.noteSwitch = el("div");
    this.note = el("div.empty", { hidden: true }, [el("div.col", {}, [
      el("div.dim", { text: "Multiview is switched off, so there is no picture here. The programme is still going out." }),
      this.noteSwitch,
    ])]);

    this.programName = el("span.ellipsis", { text: "Black" });
    const monitor = el("div.program.monitor-pane", {}, [el("div.monitor-label", {}, [
      el("strong", { text: "PROGRAMME" }), this.programName,
    ]), this.canvas, this.still, this.note]);
    this.row = el("div.monitor-stage", {}, [monitor]);
    this.studioButton = el("button.btn", { text: "Studio mode", onclick: () => setSetting("producer", !settings().producer) });
    this.streamState = el("span.sm.dim", { text: "Waiting for preview frames", role: "status" });
    this.bar = el("div.row.pad.monitor-controls", {}, [
      el("span.sm.dim.grow", { text: "What your audience is seeing" }),
      this.streamState,
      this.studioButton,
    ]);

    this.append(this.row, this.bar);

    this.offs = [
      this.client.onRender((s) => this.render(s)),
      on(document, "visibilitychange", () => this.retune()),
      this.client.on("frame", () => {
        this.lastFrameAt = Date.now();
        this.streamState.textContent = "Preview live";
      }),
      onSettingsChanged(() => this.applyMode()),
    ];

    this.frameTimer = setInterval(() => {
      if (!this.want) this.streamState.textContent = "Preview paused while hidden";
      else if (!this.lastFrameAt || Date.now() - this.lastFrameAt > 2000) {
        this.streamState.textContent = "Waiting for preview frames";
      }
    }, 1000);
    this.ro = new ResizeObserver(() => this.scheduleRetune());
    this.ro.observe(this);
    this.io = new IntersectionObserver((entries) => {
      this.visible = entries.some((e) => e.isIntersecting);
      this.retune();
    }, { threshold: 0.01 });
    this.io.observe(this);
    this.visible = true;

    this.applyMode();
    this.render(this.client.state);
  }

  disconnectedCallback() {
    clearInterval(this.frameTimer);
    clearTimeout(this.resizeTimer);
    for (const off of this.offs || []) off();
    this.offs = [];
    if (this.ro) this.ro.disconnect();
    if (this.io) this.io.disconnect();
    this.release();
  }

  /** The cell with no source is the programme return. */
  programCell(s) {
    const cells = (s.layout && s.layout.cells) || (s.multiview && s.multiview.cells) || [];
    const cell = cells.find((c) => c.source === null || c.source === undefined);
    return cell ? cell.index : null;
  }

  setWorkspaceActive(active) {
    this.workspaceActive = active;
    if (active) this.render(this.client.state);
    else this.release();
  }

  scheduleRetune() {
    if (!this.visible || this.workspaceActive === false || document.hidden) {
      this.retune();
      return;
    }
    clearTimeout(this.resizeTimer);
    this.resizeTimer = setTimeout(() => { this.resizeTimer = null; this.retune(); }, 150);
  }

  /** The switch that turns multiview back on, loaded the first time the note shows. */
  async offerSwitch() {
    if (this.switchOffered) return;
    this.switchOffered = true;
    const { configSwitch } = await import("../../shell/mixer-config.js");
    this.noteSwitch.appendChild(el("div.form", {}, [await configSwitch(this.client, {
      key: "multiview.enabled",
      label: "Multiview on",
      about: "The picture here and the source tiles come from multiview. It costs the mixer a little while a page is watching and nothing while none is.",
    })]));
  }

  retune() {
    const s = this.client.state;
    const cell = this.programCell(s);
    this.retunePreview(s);
    if (!mosaicWanted(s, this.visible && this.workspaceActive !== false)) {
      this.release();
      this.canvas.hidden = true;
      this.note.hidden = !(s.multiview && !s.multiview.enabled);
      if (!this.note.hidden) this.offerSwitch();
      return;
    }
    this.canvas.hidden = false;
    this.note.hidden = true;

    const box = this.canvas.getBoundingClientRect();
    const cols = (s.multiview && s.multiview.cols) || 1;
    // State flushes during dragging must not renegotiate an intermediate size.
    const width = this.resizeTimer && this.want ? this.lastWidth : sheetWidthFor(box.width || 640, cols);
    const fps = settings().multiviewFps;
    if (!this.want) this.want = this.client.want("multiview", { fps, width });
    else if (width !== this.lastWidth || fps !== this.lastFps) this.want.update({ fps, width });
    this.lastWidth = width;
    this.lastFps = fps;

    // The canvas backing store follows the element at the device's pixel ratio,
    // so the picture is sharp on a retina screen and not oversized on a Pi.
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.round((box.width || 640) * dpr);
    const h = Math.round((w * 9) / 16);
    if (!this.resizeTimer && w > 0 && (this.canvas.width !== w || this.canvas.height !== h)) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    if (cell !== this.attachedCell || (!this.detach && cell !== null)) {
      if (this.detach) this.detach();
      this.detach = cell === null ? null : this.client.sheet.attach(this.canvas, cell);
      this.attachedCell = cell;
    }
  }

  /** The preview pane is Studio mode's, in studio.js, loaded when first on. */
  retunePreview(s) {
    if (this.studio) this.studio.retunePreview(this, s);
  }

  releasePreview() {
    if (this.studio) this.studio.releasePreview(this);
  }

  release() {
    clearTimeout(this.resizeTimer);
    this.resizeTimer = null;
    if (this.want) {
      this.want.release();
      this.want = null;
    }
    if (this.detach) {
      this.detach();
      this.detach = null;
    }
    this.releasePreview();
  }

  applyMode() {
    const producer = settings().producer;
    this.studioButton.setAttribute("aria-pressed", String(producer));
    document.body.classList.toggle("producer", producer);
    if (producer && !this.studio) this.loadStudio();
    if (this.studio) this.studio.show(this, producer);
    this.render(this.client.state);
  }

  async loadStudio() {
    if (this.studioLoading) return;
    this.studioLoading = true;
    try {
      const studio = await import("./studio.js");
      studio.build(this);
      this.studio = studio;
      this.applyMode();
    } catch (e) {
      this.studioLoading = false;
      errorToast(e, "Studio mode");
    }
  }

  /** Preview to programme; Studio mode's, so it waits for studio.js. */
  take(durationMs) {
    return this.studio && this.studio.take(this, durationMs);
  }

  render(s) {
    // A scene of more than one item is `scene`, not `program`; reading only
    // the source said "black" under a live two box, and reading the name the
    // core froze at take time said the old one after a rename.
    const named = programLabel(s);
    this.row.querySelector(".program").classList.toggle("on", !!named);
    this.programName.textContent = named || "Black";
    this.bar.firstChild.textContent = named ? `Audience sees ${named}` : "Audience sees black";
    if (this.studio) this.studio.render(this, s);
    this.retune();
  }
}

customElements.define("gmx-program", ProgramPanel);
window.godwinmixPanels.push(ProgramPanel);
export default ProgramPanel;
