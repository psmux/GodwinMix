// The command registry: one list that the palette, the keyboard map and the
// context menus all read.
//
// A command is an id, a title, an optional shortcut and a function. Panels add
// theirs when they connect and take them away when they disconnect, so the
// palette never offers something that is no longer on screen. `core.api`, where
// the mixer publishes one, is folded in as well, so every method in the
// protocol is reachable by name even when no panel has a button for it.

const commands = new Map();
const listeners = new Set();

export function onCommandsChanged(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

function changed() {
  for (const fn of listeners) fn();
}

/**
 * @param {{id: string, title: string, group?: string, key?: string,
 *          run: (arg?: any) => any, enabled?: () => boolean, hidden?: boolean}} cmd
 * @returns {() => void} removal
 */
export function register(cmd) {
  if (!cmd || !cmd.id || typeof cmd.run !== "function") throw new Error("a command needs an id and a run function");
  commands.set(cmd.id, Object.assign({ group: "General" }, cmd));
  changed();
  return () => {
    commands.delete(cmd.id);
    changed();
  };
}

/** Register several and get one removal for the lot. */
export function registerAll(list) {
  const offs = list.filter(Boolean).map(register);
  return () => offs.forEach((off) => off());
}

export function all() {
  return [...commands.values()].filter((c) => !c.hidden);
}

export function get(id) {
  return commands.get(id) || null;
}

export async function run(id, arg) {
  const cmd = commands.get(id);
  if (!cmd) return false;
  if (cmd.enabled && !cmd.enabled()) return false;
  await cmd.run(arg);
  return true;
}

/**
 * Every method the core publishes, as a command that opens a small form.
 *
 * This is what makes the palette honest: it lists what the protocol has, not
 * what someone remembered to add a button for. A core with no `core.api` simply
 * contributes nothing here and the palette shows the panels' commands only.
 */
export async function addProtocolCommands(client, openMethodForm) {
  let api;
  try {
    api = await client.call("core.api", {});
  } catch {
    return () => {};
  }
  const methods = (api && api.methods) || [];
  // The document's shared definitions. Nearly every method points its params
  // at one of these rather than spelling them out, so the form needs them to
  // draw anything at all.
  const defs = (api && api.$defs) || {};
  const offs = [];
  for (const m of methods) {
    const id = "protocol:" + m.name;
    if (commands.has(id)) continue;
    offs.push(
      register({
        id,
        title: m.title || m.name,
        group: "Protocol",
        detail: m.summary || "",
        method: m.name,
        run: () => openMethodForm(Object.assign({ $defs: defs }, m)),
      })
    );
  }
  return () => offs.forEach((off) => off());
}
