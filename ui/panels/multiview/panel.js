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
import { settings, setSetting, onSettingsChanged } from "../../shell/settings.js";
import { toast, errorToast } from "../../shell/toast.js";
import { register } from "../../shell/commands.js";
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
    this.note = el("div.empty", { hidden: true }, [el("div.dim", { text: "Multiview is switched off, so there is no picture here. The programme is still going out." })]);

    this.previewWrap = el("div.monitor-pane.preview-pane", { hidden: true });
    this.previewCanvas = el("canvas", { width: 320, height: 180, style: { width: "100%", display: "block", background: "#000" } });
    this.previewTitle = el("strong", { text: "PREVIEW" });
    this.previewName = el("span.ellipsis", { text: "Choose a scene" });
    this.previewWrap.append(el("div.monitor-label", {}, [this.previewTitle, this.previewName]), this.previewCanvas);

    this.programName = el("span.ellipsis", { text: "Black" });
    const monitor = el("div.program.monitor-pane", {}, [el("div.monitor-label", {}, [
      el("strong", { text: "PROGRAMME" }), this.programName,
    ]), this.canvas, this.still, this.note]);
    this.row = el("div.monitor-stage", {}, [this.previewWrap, monitor]);

    this.takeBtn = el("button.btn.primary", { text: "Cut", title: "Take the preview immediately", hidden: true, onclick: () => this.take(0) });
    this.autoBtn = el("button.btn", { text: "Fade", hidden: true, onclick: () => this.take(Number(this.fadeDuration.value)) });
    this.fadeDuration = el("select", { "aria-label": "Fade duration", title: "Fade duration" }, [
      el("option", { value: "250", text: "0.25 s" }), el("option", { value: "500", text: "0.5 s", selected: true }),
      el("option", { value: "1000", text: "1 s" }), el("option", { value: "2000", text: "2 s" }),
    ]);
    this.studioButton = el("button.btn", { text: "Studio mode", onclick: () => setSetting("producer", !settings().producer) });
    this.streamState = el("span.sm.dim", { text: "Waiting for preview frames", role: "status" });
    this.bar = el("div.row.pad.monitor-controls", {}, [
      el("span.sm.dim.grow", { text: "What your audience is seeing" }),
      this.streamState,
      this.studioButton,
      this.fadeDuration,
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
    cancelAnimationFrame(this.resizeFrame);
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

  scheduleRetune() {
    if (this.resizeFrame) return;
    this.resizeFrame = requestAnimationFrame(() => { this.resizeFrame = null; this.retune(); });
  }

  retune() {
    const s = this.client.state;
    const cell = this.programCell(s);
    this.retunePreview(s);
    if (!mosaicWanted(s, this.visible)) {
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
    // The programme cell exists only once the mosaic is up, and the mosaic
    // comes up because somebody subscribed. Subscribe first, attach when the
    // layout names the cell: the next render brings it here.
    this.detach = cell === null ? null : this.client.sheet.attach(this.canvas, cell);
  }

  /**
   * The armed scene in the pane beside the programme.
   *
   * Its own subscription: `ext.preview` is what builds the compositor, and it
   * is asked for only in producer mode, with something armed, and while
   * somebody can see it, which is the rule the monitor above follows too.
   */
  retunePreview(s) {
    const wanted = !!(settings().producer && s.preview && this.visible && !document.hidden);
    if (!wanted) {
      this.releasePreview();
      return;
    }
    const box = this.previewCanvas.getBoundingClientRect();
    const fps = settings().multiviewFps;
    // One picture rather than a sheet, so one cell across.
    const width = sheetWidthFor(box.width || 320, 1);
    if (!this.previewWant) this.previewWant = this.client.want("preview", { fps, width });
    else if (width !== this.lastPreviewWidth || fps !== this.lastPreviewFps) {
      this.previewWant.update({ fps, width });
    }
    this.lastPreviewWidth = width;
    this.lastPreviewFps = fps;

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.round((box.width || 320) * dpr);
    const h = Math.round((w * 9) / 16);
    if (w > 0 && (this.previewCanvas.width !== w || this.previewCanvas.height !== h)) {
      this.previewCanvas.width = w;
      this.previewCanvas.height = h;
    }
    if (!this.detachPreview) this.detachPreview = this.client.preview.attach(this.previewCanvas);
  }

  releasePreview() {
    if (this.previewWant) {
      this.previewWant.release();
      this.previewWant = null;
    }
    if (this.detachPreview) {
      this.detachPreview();
      this.detachPreview = null;
    }
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
    this.releasePreview();
  }

  applyMode() {
    const producer = settings().producer;
    this.takeBtn.hidden = !producer;
    this.autoBtn.hidden = !producer;
    this.fadeDuration.hidden = !producer;
    this.studioButton.setAttribute("aria-pressed", String(producer));
    this.previewWrap.hidden = !producer;
    document.body.classList.toggle("producer", producer);
    this.render(this.client.state);
  }

  /** In producer mode a tap arms; this is what puts the armed tile on air. */
  async take(durationMs) {
    const target = this.armed;
    if (!target) {
      toast({ text: "Nothing is armed. Tap a tile first." });
      return;
    }
    try {
      const request = this.client.state.preview === target ? { scene: target } : { source: target };
      if (durationMs) request.transition = { type: "fade", duration_ms: durationMs };
      await this.client.call("program.take", request);
      this.armed = null;
      document.body.dataset.armed = "";
    } catch (e) {
      errorToast(e, "Take");
    }
  }

  render(s) {
    this.armed = s.preview || (document.body.dataset.armed || null) || null;
    // A scene of more than one item is `scene`, not `program`; reading only
    // the source said "black" under a live two box.
    const onAir = s.program || s.scene;
    this.row.querySelector(".program").classList.toggle("on", !!onAir);
    this.previewWrap.hidden = !settings().producer;
    this.programName.textContent = (s.sources.find((x) => x.id === s.program) || {}).name || onAir || "Black";
    this.previewName.textContent = this.armed || "Choose a scene";
    this.takeBtn.disabled = this.autoBtn.disabled = !this.armed;
    this.bar.firstChild.textContent = onAir
      ? `Audience sees ${(s.sources.find((x) => x.id === s.program) || {}).name || onAir}`
      : "Audience sees black";
    this.retune();
  }
}

customElements.define("gmx-program", ProgramPanel);
window.godwinmixPanels.push(ProgramPanel);
export default ProgramPanel;
