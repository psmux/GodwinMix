// Two monitors, preview on the left and programme on the right, and the bar
// that puts one onto the other.
//
// This is the desk every volunteer has already seen: you line the next shot up
// in preview, you look at it, and then you Cut or Auto. Clicking a scene tab
// arms it, so the tab and the left hand picture always say the same thing, and
// nothing reaches air until somebody presses the button that says so. The
// number keys and the "Tap cuts directly" setting are the way out for the
// volunteer with three cameras who wants the old single tap.
//
// One picture is always on: the programme. It is the only thing on the page
// that subscribes to the multiview stream regardless of what the gallery is
// doing, because the operator has to be able to see what the audience sees on
// any machine. Everything else steps down.
//
// It still costs nothing when nobody is looking: both subscriptions are
// released when the tab is hidden and when the monitors scroll out of view,
// and each is taken at its own pixel size, never upscaled. With nothing armed
// there is no preview subscription at all, so the compositor is never built.

import { el, on } from "../../shell/dom.js";
import { sheetWidthFor } from "../../client/frames.js";
import { settings, onSettingsChanged } from "../../shell/settings.js";
import { toast, errorToast } from "../../shell/toast.js";
import { register } from "../../shell/commands.js";
import { mosaicWanted } from "./wanted.js";

/** How long Auto takes, in milliseconds, remembered on this device. */
const FADE_KEY = "gmx.transition.ms";
const DEFAULT_FADE_MS = 300;

export function savedFade() {
  try {
    const ms = Number(localStorage.getItem(FADE_KEY));
    return Number.isFinite(ms) && ms > 0 && ms <= 5000 ? ms : DEFAULT_FADE_MS;
  } catch {
    return DEFAULT_FADE_MS;
  }
}

/**
 * What a take should say, given what is armed and how long it takes.
 *
 * Its own function with nothing imported, because the shape of the request is
 * the part worth testing: `duration_ms` is inside the transition object and
 * not beside it, and a cut names no transition at all.
 */
export function takeRequest(armed, durationMs) {
  if (!armed) return null;
  const request = armed.scene ? { scene: armed.scene } : { source: armed.source };
  if (durationMs > 0) request.transition = { type: "fade", duration_ms: durationMs };
  return request;
}

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
    this.append(this.buildMonitors(), this.buildBar());

    this.offs = [
      this.client.onRender((s) => this.render(s)),
      on(document, "visibilitychange", () => this.retune()),
      this.client.on("frame", () => {
        this.lastFrameAt = Date.now();
        this.streamState.textContent = "Live";
      }),
      onSettingsChanged(() => this.applyMode()),
      register({
        id: "program.take-armed",
        title: "Take the armed scene",
        group: "Programme",
        enabled: () => !!this.armed,
        run: () => this.take(0),
      }),
      register({
        id: "program.auto-armed",
        title: "Fade the armed scene on air",
        group: "Programme",
        enabled: () => !!this.armed,
        run: () => this.take(savedFade()),
      }),
    ];

    this.frameTimer = setInterval(() => {
      if (!this.want) this.streamState.textContent = "Paused while hidden";
      else if (!this.lastFrameAt || Date.now() - this.lastFrameAt > 2000) {
        this.streamState.textContent = "Waiting for frames";
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

  /** The two pictures, side by side, each with its label. */
  buildMonitors() {
    this.previewCanvas = el("canvas", { width: 320, height: 180 });
    this.previewEmpty = el("div.empty.dim.sm", { text: "Nothing armed. Click a scene." });
    this.previewPane = el("div.pane.preview", {}, [this.previewCanvas, this.previewEmpty]);
    this.previewName = el("span.who.sm");

    this.canvas = el("canvas", { width: 640, height: 360 });
    this.still = el("img", { alt: "", hidden: true });
    this.note = el("div.empty.dim.sm", {
      hidden: true,
      text: "Multiview is switched off, so there is no picture here. The programme is still going out.",
    });
    this.programPane = el("div.pane.program", {}, [this.canvas, this.still, this.note]);
    this.programName = el("span.who.sm");

    this.monitors = el("div.monitors", {}, [
      el("div.monitor", {}, [
        el("div.mlabel.sm", {}, [el("span.armed", { text: "Preview" }), this.previewName]),
        this.previewPane,
      ]),
      el("div.monitor", {}, [
        el("div.mlabel.sm", {}, [el("span.live", { text: "Programme" }), this.programName]),
        this.programPane,
      ]),
    ]);
    return this.monitors;
  }

  /** Cut, Auto and how long Auto takes. */
  buildBar() {
    this.cutBtn = el("button.btn.primary", {
      text: "Cut",
      title: "Put the preview on air now",
      onclick: () => this.take(0),
    });
    this.autoBtn = el("button.btn", {
      text: "Auto",
      title: "Fade the preview on air",
      onclick: () => this.take(savedFade()),
    });
    this.fade = el("input.num.sm", {
      type: "number",
      min: "50",
      max: "5000",
      step: "50",
      value: String(savedFade()),
      "aria-label": "How long Auto takes, in milliseconds",
      title: "How long Auto takes, in milliseconds",
      onchange: () => this.keepFade(),
    });
    this.audience = el("span.sm.dim.grow", { text: "Audience sees black" });
    this.streamState = el("span.sm.dim", { text: "Waiting for frames", role: "status" });
    this.bar = el("div.row.pad.transition", {}, [
      this.audience,
      this.streamState,
      this.cutBtn,
      this.autoBtn,
      this.fade,
      el("span.sm.dim", { text: "ms" }),
    ]);
    return this.bar;
  }

  keepFade() {
    const ms = Math.max(50, Math.min(5000, Number(this.fade.value) || DEFAULT_FADE_MS));
    this.fade.value = String(ms);
    try {
      localStorage.setItem(FADE_KEY, String(ms));
    } catch {
      /* the choice lasts the session */
    }
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
    this.size(this.canvas, box.width || 640);
    if (this.detach) this.detach();
    // The programme cell exists only once the mosaic is up, and the mosaic
    // comes up because somebody subscribed. Subscribe first, attach when the
    // layout names the cell: the next render brings it here.
    this.detach = cell === null ? null : this.client.sheet.attach(this.canvas, cell);
  }

  /**
   * The armed scene in the left hand monitor.
   *
   * Its own subscription: `ext.preview` is what builds the compositor, and it
   * is asked for only with something armed and while somebody can see it,
   * which is the rule the programme monitor follows too. Nothing armed is not
   * a black box: it is the line that says what to do about it.
   */
  retunePreview(s) {
    const wanted = !!(s.preview && this.visible && !document.hidden);
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

    this.size(this.previewCanvas, box.width || 320);
    if (!this.detachPreview) this.detachPreview = this.client.preview.attach(this.previewCanvas);
  }

  /** The backing store, in device pixels, at sixteen by nine. */
  size(canvas, cssWidth) {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.round(cssWidth * dpr);
    const h = Math.round((w * 9) / 16);
    if (w > 0 && (canvas.width !== w || canvas.height !== h)) {
      canvas.width = w;
      canvas.height = h;
    }
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
    document.body.classList.toggle("producer", settings().producer);
  }

  /**
   * Cut, and Auto with a duration.
   *
   * The armed scene is taken by name, because a scene of more than one item is
   * not a source. A source armed from the tray is still taken as a source, so
   * the older gesture keeps working on a core with no scene server.
   */
  async take(durationMs) {
    const armed = this.armed;
    const request = takeRequest(armed, durationMs);
    if (!request) {
      toast({ text: "Nothing is armed. Click a scene first." });
      return;
    }
    const wasLive = this.live;
    try {
      await this.client.call("program.take", request);
    } catch (e) {
      errorToast(e, durationMs > 0 ? "Auto" : "Cut");
      return;
    }
    document.body.dataset.armed = "";
    this.swap(armed, wasLive);
  }

  /**
   * Preview and programme change places, the way they do on a desk.
   *
   * What was on air is armed, so the operator can put it back with one press,
   * and the preview never sits there showing the same picture as the
   * programme. A core with no scene server has nothing to arm and says so by
   * refusing, which is not worth a toast.
   */
  swap(armed, wasLive) {
    if (!armed.scene) return;
    const back = wasLive && wasLive !== armed.scene ? { scene: wasLive } : {};
    this.client.call("scene.preview.set", back).catch(() => {});
  }

  /** What is armed: a scene by name, or a source armed from the tray. */
  armedFrom(s) {
    if (s.preview) return { scene: s.preview };
    const source = document.body.dataset.armed || null;
    return source ? { source } : null;
  }

  render(s) {
    this.armed = this.armedFrom(s);
    // A scene of more than one item is `scene`, not `program`; reading only
    // the source said "black" under a live two box.
    const onAir = s.program || s.scene;
    this.live = s.scene || null;
    this.programPane.classList.toggle("on", !!onAir);
    this.previewPane.classList.toggle("armed", !!this.armed);
    this.previewCanvas.hidden = !this.armed;
    this.previewEmpty.hidden = !!this.armed;
    this.previewName.textContent = this.armed
      ? this.armed.scene || this.nameOf(s, this.armed.source)
      : "";
    this.programName.textContent = onAir ? this.nameOf(s, s.program) || s.scene : "black";
    this.cutBtn.disabled = !this.armed;
    this.autoBtn.disabled = !this.armed;
    this.audience.textContent = onAir
      ? `Audience sees ${this.nameOf(s, s.program) || s.scene}`
      : "Audience sees black";
    this.retune();
  }

  /** A source's name, for a label that should not read as an id. */
  nameOf(s, id) {
    if (!id) return "";
    const source = s.sources.find((x) => x.id === id);
    return source ? source.name : id;
  }
}

customElements.define("gmx-program", ProgramPanel);
window.godwinmixPanels.push(ProgramPanel);
export default ProgramPanel;
