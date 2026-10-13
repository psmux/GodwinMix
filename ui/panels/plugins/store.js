// What every button on the Plugins page does, kept in one place so a card in
// the Featured row, the same card in the grid and the detail view all show
// the same install as it runs, and the same note when it ends.
//
// An install never waits on the page: the core answers `plugin.add` with a
// task at once and does the download, the venv and the start on its own
// threads, never the programme's. The page only reads the task.

import { perform } from "./act.js";
import { primary } from "./model.js";
import { busyLabel, goButton, paint, buttonsFor, noteFor } from "./install.js";
import { arrive, canOpen, openPlugin } from "./ready.js";

/**
 * @param {object} client
 * @param {{reload: () => Promise<object[]>, leave: () => void, arrive?: Function}} hooks
 *   reload reads the gallery again and draws it; leave closes the page so
 *   what Open shows can be seen
 */
export function makeStore(client, hooks) {
  const notes = new Map();
  const busy = new Map();
  const land = hooks.arrive || arrive;

  /**
   * The primary button for one plugin, as it stands now.
   *
   * @param {(done, act) => void} after  told what happened, once it has
   */
  function button(p, extra = "", after = () => {}) {
    const note = notes.get(p.name);
    let go = primary(p, canOpen(p));
    if (note && !note.ok && go.act === "add") go = { label: "Try again", act: "add" };
    const node = goButton(p, go, async () => after(await press(p, go.act), go.act), extra);
    if (busy.has(p.name)) paint(node, busy.get(p.name));
    return node;
  }

  function repaint(name) {
    for (const b of buttonsFor(document, name)) paint(b, busy.get(name));
  }

  /** Install, Update, Remove or Open, and what to say when it is done. */
  async function press(p, act) {
    if (act === "open") return openPlugin(client, p, hooks.leave);
    if (busy.has(p.name)) return null;
    notes.delete(p.name);
    busy.set(p.name, busyLabel(null, act, p));
    repaint(p.name);
    const done = await perform(client, p, act, () => {}, (task) => {
      busy.set(p.name, busyLabel(task, act, p));
      repaint(p.name);
    });
    if (done.ok && act !== "remove") done.text = await ready(p, act, done.text);
    busy.delete(p.name);
    notes.set(p.name, done);
    await hooks.reload();
    return done;
  }

  /**
   * Find its panels and open the first; after an install, say what is ready
   * to use. An update keeps its own words.
   */
  async function ready(p, act, text) {
    busy.set(p.name, { label: "Opening", pct: 100 });
    repaint(p.name);
    const ids = await land(p).catch(() => []);
    if (act !== "add") return text;
    if (ids.length) return "Ready. Its panel is open. Nothing restarted.";
    if ((p.kinds || []).includes("source")) return "Ready. Press Open to add it as a source.";
    return text;
  }

  return { button, press, note: (name) => noteFor(notes.get(name)), notes, busy };
}
