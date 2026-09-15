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
import { settings, onSettingsChanged } from "../../shell/settings.js";
import { toast, errorToast } from "../../shell/toast.js";
import { register } from "../../shell/commands.js";

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
    this.note = el("div.empty", { hidden: true }, [el("div.dim", { text: "Multiview is switched off, so there is no picture here. The programme is still going out." })]);

    this.previewWrap = el("div", { hidden: true, style: { flex: "1 1 0", minWidth: "0" } });
    this.previewCanvas = el("canvas", { width: 320, height: 180, style: { width: "100%", display: "block", background: "#000" } });
    this.previewWrap.append(el("div.sm.dim.pad", { text: "Preview" }), this.previewCanvas);

    const monitor = el("div.program.grow", { style: { minWidth: "0" } }, [this.canvas, this.still, this.note]);
    this.row = el("div.row", { style: { alignItems: "stretch", gap: "0" } }, [monitor, this.previewWrap]);

    this.takeBtn = el("button.btn.primary", { text: "Take", hidden: true, onclick: () => this.take(0) });
    this.autoBtn = el("button.btn", { text: "Auto", hidden: true, onclick: () => this.take(500) });
    this.streamState = el("span.sm.dim", { text: "Waiting for preview frames", role: "status" });
    this.bar = el("div.row.pad", {}, [
      el("span.sm.dim.grow", { text: "What your audience is seeing" }),
      this.streamState,
      this.takeBtn,
      this.autoBtn,
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
      register({
        id: "program.take-armed",
        title: "Take the armed tile",
        group: "Programme",
        enabled: () => settings().producer && !!this.armed,
        run: () => this.take(0),
      }),
    ];

    this.frameTimer = setInterval(() => {
      if (!this.want) this.streamState.textContent = "Preview paused while hidden";
      else if (!this.lastFrameAt || Date.now() - this.lastFrameAt > 2000) {
        this.streamState.textContent = "Waiting for preview frames";
      }
    }, 1000);
    this.ro = new ResizeObserver(() => this.retune());
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

  retune() {
    const s = this.client.state;
    const cell = this.programCell(s);
    const wanted = this.visible && !document.hidden && s.multiview && s.multiview.enabled && cell !== null;
    if (!wanted) {
      this.release();
      this.canvas.hidden = true;
      this.note.hidden = !(s.multiview && !s.multiview.enabled);
      return;
    }
    this.canvas.hidden = false;
    this.note.hidden = true;

    const box = this.canvas.getBoundingClientRect();
    const cols = (s.multiview && s.multiview.cols) || 1;
    const width = sheetWidthFor(box.width || 640, cols);
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
    if (w > 0 && (this.canvas.width !== w || this.canvas.height !== h)) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    if (this.detach) this.detach();
    this.detach = this.client.sheet.attach(this.canvas, cell);
  }

  release() {
    if (this.want) {
      this.want.release();
      this.want = null;
    }
    if (this.detach) {
      this.detach();
      this.detach = null;
    }
  }

  applyMode() {
    const producer = settings().producer;
    this.takeBtn.hidden = !producer;
    this.autoBtn.hidden = !producer;
    document.body.classList.toggle("producer", producer);
  }

  /** In producer mode a tap arms; this is what puts the armed tile on air. */
  async take(durationMs) {
    const target = this.armed;
    if (!target) {
      toast({ text: "Nothing is armed. Tap a tile first." });
      return;
    }
    try {
      await this.client.call("program.take", durationMs ? { source: target, transition: "fade", duration_ms: durationMs } : { source: target });
      this.armed = null;
      document.body.dataset.armed = "";
    } catch (e) {
      errorToast(e, "Take");
    }
  }

  render(s) {
    this.armed = s.preview || (document.body.dataset.armed || null) || null;
    this.row.querySelector(".program").classList.toggle("on", !!s.program);
    this.previewWrap.hidden = !(settings().producer && s.preview);
    this.bar.firstChild.textContent = s.program
      ? `Audience sees ${(s.sources.find((x) => x.id === s.program) || {}).name || s.program}`
      : "Audience sees black";
    this.retune();
  }
}

customElements.define("gmx-program", ProgramPanel);
window.godwinmixPanels.push(ProgramPanel);
export default ProgramPanel;
