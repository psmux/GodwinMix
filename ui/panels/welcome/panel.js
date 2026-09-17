// The first five minutes: five tiles, one of which gets this machine streaming.
//
// It shows itself when a core has no sources and no preset has been applied,
// and it never shows itself again once one has. That is the whole of its state
// machine: the core knows whether a preset was applied (`core.info` carries
// `ui.preset`), so a new browser on the same mixer does not get asked again.
//
// Picking a tile calls `preset.apply` over the same protocol every other client
// uses, then shows that preset's own three steps with the plugins that are
// still missing named. Nothing here is private to the first party UI.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { modal } from "../../shell/modal.js";
import { openPicker } from "../../shell/picker.js";
import { pluginSourceFor, listPlugins, hasPlugin } from "../../client/kinds.js";
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
            "Pick the one closest to what you are doing. It sets this mixer up and tells " +
            "you the three things left to do. Nothing here is permanent: Settings changes " +
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
      return importFromObs();
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
      showSteps(this.client, choice, result);
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

/** What to do next, from the preset's own manifest rather than from here. */
function showSteps(client, choice, result) {
  const plan = (result && result.plan) || {};
  const steps = plan.steps || [];
  const missing = (plan.plugins || []).filter((p) => !p.installed);
  const body = el("div.col");

  body.appendChild(
    el("p", { text: `${choice.title} is set up. Three things left.`, style: { marginTop: "0" } })
  );
  const list = el("ol.welcome-steps");
  for (const step of steps) list.appendChild(el("li", { text: step }));
  if (!steps.length) list.appendChild(el("li", { text: "Add a source and press its tile." }));
  body.appendChild(list);

  for (const plugin of missing) body.appendChild(installRow(client, plugin));
  for (const note of (result && result.needs_restart) || []) {
    body.appendChild(el("p.sm.dim", { text: note }));
  }

  const m = modal({
    title: "Nearly there",
    body,
    footer: [el("button.btn.primary", { text: "Got it", onclick: () => m.close() })],
  });
}

/**
 * One missing plugin, and the button that installs it.
 *
 * This used to print `gmx plugin add camera` and stop there, which asks
 * somebody who has just picked Church service to go and find a terminal.
 * `plugin.add` is the same call that command makes, it works while the mixer
 * runs, and the listing is read again afterwards so the line says what
 * actually happened rather than what was hoped for.
 */
function installRow(client, plugin) {
  const note = el("span.sm.dim");
  const button = el("button.btn.primary", { text: `Install ${plugin.name} support` });
  const line = el("p.sm", {}, [
    el("span.dot.stalled"),
    " ",
    el("span", {
      text:
        `The ${plugin.name} plugin is not installed yet, so anything that needs it ` +
        `stays listed and does not start. `,
    }),
    button,
    note,
  ]);
  button.onclick = async () => {
    button.disabled = true;
    note.textContent = " Installing. This can take a minute.";
    try {
      const source = await pluginSourceFor(client, plugin.name);
      await client.call("plugin.add", { source });
    } catch (e) {
      errorToast(e, `Installing ${plugin.name}`);
      button.disabled = false;
      note.textContent = "";
      return;
    }
    const plugins = await listPlugins(client);
    note.textContent = hasPlugin(plugins, plugin.name)
      ? " Installed, and nothing restarted."
      : " Installed, but the mixer has not picked it up yet.";
    button.remove();
  };
  return line;
}

/** The OBS importer is `gmx import obs`; the page says where to point it. */
function importFromObs() {
  const body = el("div.col");
  body.appendChild(
    el("p", {
      text:
        "GodwinMix reads an OBS scene collection and keeps your scenes, their items and " +
        "their positions. The importer runs on the machine that has OBS on it.",
      style: { marginTop: "0" },
    })
  );
  body.appendChild(el("p.sm.dim", { text: "In a terminal, with the path to the collection:" }));
  body.appendChild(
    el("pre.welcome-code", {
      text: "gmx import obs ~/.config/obs-studio/basic/scenes/Untitled.json \\\n  --out godwinmix.scenes.json",
    })
  );
  body.appendChild(
    el("p.sm.dim", {
      text:
        "On Windows the collections are in %APPDATA%\\obs-studio\\basic\\scenes, and on " +
        "macOS in ~/Library/Application Support/obs-studio/basic/scenes. " +
        "docs/how-to/import-from-obs.md has the rest, including what does not come across.",
    })
  );
  const m = modal({
    title: "Import from OBS",
    body,
    footer: [el("button.btn.primary", { text: "Close", onclick: () => m.close() })],
  });
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
