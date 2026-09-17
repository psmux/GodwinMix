// The first five minutes: five tiles, one of which gets this machine streaming.
//
// It shows itself when a core has no sources and no preset has been applied,
// and it never shows itself again once one has. That is the whole of its state
// machine: the core knows whether a preset was applied (`core.info` carries
// `ui.preset`), so a new browser on the same mixer does not get asked again.
//
// Picking a tile calls `preset.apply` over the same protocol every other client
// uses, then puts up the checklist in `checklist.js`, which is where the
// person actually finishes the job. Nothing here is private to the first
// party UI.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { modal } from "../../shell/modal.js";
import { openPicker } from "../../shell/picker.js";
import { applyCoreDefaults, forgetPreset } from "./defaults.js";

/** The five choices, in the order a person reads them. */
const CHOICES = [
  {
    id: "church",
    art: "church",
    title: "Church service",
    line: "Two cameras, lyrics over the picture, slides, and YouTube and Facebook at once.",
  },
  {
    id: "classroom",
    art: "classroom",
    title: "Classroom",
    line: "A camera on the teacher, the screen beside it, recorded and streamed.",
  },
  {
    id: "esports",
    art: "esports",
    title: "Streamer or gaming",
    line: "The game full screen, your camera in the corner, an overlay from a page.",
  },
  {
    id: null,
    art: "empty",
    title: "Start empty",
    line: "Nothing configured. Add your first source yourself.",
  },
  {
    id: "obs",
    art: "obs",
    title: "Import from OBS",
    line: "Bring a scene collection across from OBS Studio and keep your scenes.",
  },
];

export class WelcomePanel extends HTMLElement {
  static get panel() {
    return { id: "core/welcome", title: "Welcome", slots: ["modal"], tag: "gmx-welcome" };
  }

  setClient(client) {
    this.client = client;
  }

  connectedCallback() {
    if (this.built) return;
    this.built = true;
    this.hidden = true;
    this.decide().catch((e) => console.error("welcome", e));
  }

  disconnectedCallback() {
    for (const off of this.offs || []) off();
    this.offs = [];
  }

  /** Show only on a core nobody has set up yet, or when asked from Settings. */
  async decide() {
    const info = await applyCoreDefaults(this.client);
    const applied = info && info.ui && info.ui.preset;
    const sources = (this.client.state.sources || []).length;
    if (!applied && sources === 0) await this.open();
    this.offs = [
      // Settings has a "Show this again", which fires this.
      onShowAgain(() => {
        forgetPreset();
        this.open().catch((e) => console.error("welcome", e));
      }),
    ];
  }

  async open() {
    if (this.dialog) return;
    // The five pictures are five kilobytes of inline SVG, and a mixer that has
    // been set up never draws them. They arrive with the dialog.
    const { art } = await import("./tiles.js");
    const grid = el("div.welcome-grid");
    for (const choice of CHOICES) grid.appendChild(this.tile(choice, art));
    this.dialog = modal({
      title: "What are you streaming?",
      wide: true,
      body: el("div", {}, [
        el("p.dim", {
          text:
            "Pick the one closest to what you are doing. It sets this mixer up and then " +
            "walks you through whatever is left, here on this page. Nothing is permanent: " +
            "Settings changes any of it afterwards.",
          style: { marginTop: "0" },
        }),
        grid,
      ]),
      onClose: () => {
        this.dialog = null;
      },
    });
  }

  tile(choice, art) {
    const node = el("button.welcome-tile", {
      type: "button",
      onclick: () => this.pick(choice),
    }, [
      art(choice.art),
      el("strong", { text: choice.title }),
      el("span.dim.sm", { text: choice.line }),
    ]);
    return node;
  }

  async pick(choice) {
    if (choice.id === "obs") {
      this.close();
      const { importFromObs } = await import("./obs.js");
      return importFromObs(this.client);
    }
    if (!choice.id) {
      this.close();
      return openPicker(this.client, "source");
    }
    const tile = this.dialog && this.dialog.el.querySelector(".welcome-tile:focus");
    if (tile) tile.disabled = true;
    try {
      const result = await this.client.call("preset.apply", { name: choice.id });
      await applyCoreDefaults(this.client, { force: true });
      this.close();
      const { showChecklist } = await import("./checklist.js");
      await showChecklist(this.client, choice, result);
    } catch (e) {
      if (tile) tile.disabled = false;
      errorToast(e, `Setting up ${choice.title}`);
    }
  }

  close() {
    if (this.dialog) this.dialog.close();
    this.dialog = null;
  }
}

// ------------------------------------------------------- "Show this again"

const LISTENERS = new Set();

/** Settings calls this to put the welcome tiles back up. */
export function showWelcomeAgain() {
  for (const fn of LISTENERS) fn();
}

function onShowAgain(fn) {
  LISTENERS.add(fn);
  return () => LISTENERS.delete(fn);
}

customElements.define("gmx-welcome", WelcomePanel);
if (window.godwinmixPanels) window.godwinmixPanels.push(WelcomePanel);
export default WelcomePanel;
