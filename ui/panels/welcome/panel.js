// The first five minutes: five tiles, one of which gets this machine streaming.
//
// It opens only when asked, from Settings ("Show the welcome tiles again") or
// the palette. It used to open by itself on any core with no sources and no
// preset, which on a station meant every new show, and that is not what an
// operator setting up a show wants in front of them.
//
// Picking a tile calls `preset.apply` over the same protocol every other client
// uses, then shows that preset's own steps as a checklist, each with the
// button that does it. Nothing here is private to the first party UI.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { modal } from "../../shell/modal.js";
import { openPicker } from "../../shell/picker-loader.js";
import { run } from "../../shell/commands.js";
import { applyCoreDefaults, forgetPreset } from "./defaults.js";

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

  /** Apply the core's defaults, and open only when Settings asks. */
  async decide() {
    await applyCoreDefaults(this.client);
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
    // been set up never draws them. They arrive with the dialog, and so do
    // the words on the tiles.
    const { art, CHOICES } = await import("./tiles.js");
    const grid = el("div.welcome-grid");
    for (const choice of CHOICES) grid.appendChild(this.tile(choice, art));
    this.dialog = modal({
      title: "What are you streaming?",
      wide: true,
      body: el("div", {}, [
        el("p.dim", {
          text:
            "Pick the one closest to what you are doing. It sets this mixer up and tells " +
            "you what is left to do. Nothing here is permanent: Settings changes " +
            "any of it afterwards.",
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
      return (await import("./after.js")).importFromObs(this.client);
    }
    if (!choice.id) {
      this.close();
      // The Sources panel's own door, the one Ctrl+N uses, because it puts
      // what is added into the focused scene. The bare picker adds to the
      // mixer and to no scene, and a first source nobody can see reads as a
      // mixer that does not work. It is the fallback for a closed panel.
      if (await run("tray.add")) return;
      return openPicker(this.client, "source");
    }
    const tile = this.dialog && this.dialog.el.querySelector(".welcome-tile:focus");
    if (tile) tile.disabled = true;
    try {
      const result = await this.client.call("preset.apply", { name: choice.id });
      await applyCoreDefaults(this.client, { force: true });
      this.close();
      (await import("./after.js")).showSteps(this.client, choice, result);
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

/**
 * Settings calls this to put the welcome tiles back up. The panel is not part
 * of the page a person opens, since it never opens by itself, so the first
 * call makes one.
 */
export function showWelcomeAgain(client = window.gmxClient) {
  if (!LISTENERS.size && client) {
    const panel = new WelcomePanel();
    panel.setClient(client);
    document.body.appendChild(panel);
    forgetPreset();
    panel.open().catch((e) => console.error("welcome", e));
    return;
  }
  for (const fn of LISTENERS) fn();
}

function onShowAgain(fn) {
  LISTENERS.add(fn);
  return () => LISTENERS.delete(fn);
}

customElements.define("gmx-welcome", WelcomePanel);
if (window.godwinmixPanels) window.godwinmixPanels.push(WelcomePanel);
export default WelcomePanel;
