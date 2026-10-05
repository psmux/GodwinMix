// Who else is operating this mixer: the count in the header and the list
// behind it.
//
// The core sends `event/presence.changed` only to a client that subscribed to
// it, so the page asks for `presence.*` once, here, for as long as it is open.
// Every piece that shows presence (this button, the composer's marker) reads
// the one list kept here.

import { el } from "./dom.js";
import { modal } from "./modal.js";
import { others, nameOf, sentence } from "../kits/protocol/presence.js";

const state = { client: null, list: { clients: [] }, listeners: new Set() };

/** Follow presence for this client. Safe to call from every piece that shows it. */
export function watchPresence(client) {
  // A page on a core with no /rpc (the legacy adapter, a test's stand in)
  // has nobody to report, and the button simply stays hidden.
  if (state.client === client || !client || typeof client.listen !== "function") return;
  state.client = client;
  client.listen("presence.*");
  client.on("event", ({ name, params }) => {
    if (name === "presence.changed") publish(params);
  });
  // The subscription sends the list as soon as it is in force; this covers a
  // page that started watching after that.
  Promise.resolve(client.opened && client.opened())
    .then(() => client.call("presence.list", {}))
    .then(publish)
    .catch(() => {});
}

function publish(list) {
  state.list = list && Array.isArray(list.clients) ? list : { clients: [] };
  for (const fn of state.listeners) fn(state.list);
}

/** Called now and on every change. Returns the removal function. */
export function onPresence(fn) {
  state.listeners.add(fn);
  fn(state.list);
  return () => state.listeners.delete(fn);
}

/** Tell everybody which scene this page is editing, or null for none. */
export function tellEditing(client, scene) {
  return client.call("presence.set", { scene: scene || null }).catch(() => {});
}

/** The header button: hidden while nobody else is here. */
export function presenceButton(client) {
  watchPresence(client);
  const button = el("button.btn.presence", { hidden: true, onclick: () => showList() });
  onPresence((list) => {
    const here = others(list);
    button.hidden = here.length === 0;
    button.textContent = here.length === 1 ? "1 other here" : `${here.length} others here`;
    button.title = here.length ? `Also operating this mixer: ${sentence(here)}` : "";
  });
  return button;
}

function showList() {
  const rows = (state.list.clients || []).map((c) =>
    el("div.row", {}, [
      el("span.grow", { text: c.you ? `${nameOf(c)} (this page)` : nameOf(c) }),
      el("span.dim", { text: c.scene ? `editing ${sceneName(c.scene)}` : "" }),
    ])
  );
  modal({ title: "Who is here", body: el("div.col.sm", {}, rows) });
}

/** A scene's name, from whoever holds the scene document on this page. */
function sceneName(id) {
  const name = state.nameScene && state.nameScene(id);
  return name || "a scene";
}

/** The scene session says how to turn a scene id into its name. */
export function nameScenesWith(fn) {
  state.nameScene = fn;
}
