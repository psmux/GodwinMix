// The Mixer tab in Settings: the mixer's own configuration, as a form.
//
// Everything on the other two tabs is a preference belonging to this browser.
// This one is the mixer's `godwinmix.toml`, read with `config.get` and written
// with `config.set`, because the rule is that somebody using the GUI never
// opens that file.
//
// Two things make the tab different from an ordinary form. Most keys do not
// take effect until the mixer restarts, so a save has to report what is on now
// and what is waiting, and the waiting part has to stay visible afterwards.
// And restarting is only possible where something is supervising the core, so
// the button that offers it appears only when the core says it is supervised.
//
// Loaded with `import()` from `settings.js`, so a volunteer who never opens
// this tab never fetches it.

import { el, clear, on } from "./dom.js";
import { confirmModal } from "./modal.js";
import { toast } from "./toast.js";

/// The tables, in the order an operator would read them, with the heading and
/// the sentence that goes under it.
export const GROUPS = [
  ["program", "Programme", "What leaves the mixer. Match the bitrate to the upload you really have."],
  ["safety", "Safety", "The rules in front of every take. These are the only ones that apply the moment you save."],
  ["multiview", "Multiview", "The operator's mosaic. Every watcher pays for it, so it is deliberately modest."],
  ["snapshot", "Stills", "Single frames cut out of the mosaic, for an agent or a thumbnail."],
  ["stall", "Stalled sources", "What the supervisor does with a source that has stopped delivering."],
  ["browser", "Web pages", "Pages drawn over the video."],
  ["canvas", "Canvas", "The size and rate everything is normalised to. Fixed while the mixer runs."],
];

/// The canvas cannot be changed under a running programme, by design: the
/// output encoder is started once and never restarted. The tab shows it, and
/// warns before letting anyone touch it.
export const READ_ONLY = ["canvas"];

/**
 * A `config.get` answer, grouped the way the form draws it.
 *
 * Pure, so the grouping is testable without a mixer. A table with no keys in
 * the answer is left out entirely rather than drawn empty, which is what
 * happens on a core built without one of them.
 */
export function groupKeys(doc) {
  const keys = (doc && doc.keys) || [];
  const out = [];
  for (const [table, title, hint] of GROUPS) {
    const mine = keys.filter((k) => k.table === table);
    if (mine.length) out.push({ table, title, hint, keys: mine, readOnly: READ_ONLY.includes(table) });
  }
  // A table the core knows about and this page does not is still worth
  // showing: a form that silently drops a setting is worse than an ugly one.
  const known = new Set(GROUPS.map(([table]) => table));
  for (const key of keys) {
    if (known.has(key.table)) continue;
    let group = out.find((g) => g.table === key.table);
    if (!group) {
      group = { table: key.table, title: key.table, hint: "", keys: [], readOnly: false };
      out.push(group);
    }
    group.keys.push(key);
  }
  return out;
}

/** Every key the file has moved on from and the running mixer has not. */
export function pendingKeys(doc) {
  return ((doc && doc.keys) || []).filter((k) => k.source === "pending restart").map((k) => k.key);
}

/**
 * What to send to `config.set`: only the fields whose value actually moved.
 *
 * `read` is a map of key to the value the control holds now. Sending a key
 * that has not changed is harmless, the core answers `unchanged`, but it makes
 * the answer unreadable and it writes a line into the operator's file for no
 * reason.
 */
export function changesFrom(keys, read) {
  const changes = {};
  for (const info of keys) {
    if (!(info.key in read)) continue;
    const wanted = read[info.key];
    if (wanted === null || wanted === undefined) continue;
    if (same(wanted, info.file_value === undefined ? info.value : info.file_value)) continue;
    changes[info.key] = wanted;
  }
  return changes;
}

function same(a, b) {
  if (typeof a === "number" && typeof b === "number") return a === b;
  return JSON.stringify(a) === JSON.stringify(b);
}

/** "6000 kbit/s" or just "true", for the line under a field. */
export function describeDefault(info) {
  const unit = info.unit ? ` ${info.unit}` : "";
  const timing = info.timing === "hot" ? "applies at once" : "needs a restart";
  return `Default ${info.default}${unit}. ${timing}.`;
}

// ------------------------------------------------------------------ the tab

/**
 * Build the Mixer tab. Returns a node straight away and fills it in when the
 * core answers, so the tab is never blank and never blocks the dialog.
 */
export function mixerTab(client) {
  const root = el("div.form");
  const body = el("div");
  const status = el("div.hint", { text: "Reading the mixer's settings..." });
  root.appendChild(status);
  root.appendChild(body);

  // What each control holds right now, by key. Read on save.
  const read = {};
  let doc = null;

  async function load() {
    try {
      doc = await client.call("config.get", {});
    } catch (e) {
      clear(body);
      status.textContent = `The mixer would not say what its settings are: ${e.message || e}`;
      return;
    }
    draw();
  }

  function draw() {
    clear(body);
    for (const key of Object.keys(read)) delete read[key];
    const pending = pendingKeys(doc);
    status.textContent = pending.length
      ? `${pending.length} setting${pending.length === 1 ? " is" : "s are"} saved and waiting for a restart.`
      : `Read from ${doc.path || "this mixer"}.`;

    for (const group of groupKeys(doc)) {
      body.appendChild(el("div.group-title", { text: group.title }));
      if (group.hint) body.appendChild(el("div.hint", { text: group.hint }));
      const locked = group.readOnly && !unlocked.has(group.table);
      for (const info of group.keys) body.appendChild(fieldFor(info, locked));
      if (group.readOnly) body.appendChild(changeButton(group));
    }
    body.appendChild(footer());
  }

  const unlocked = new Set();

  function changeButton(group) {
    if (unlocked.has(group.table)) {
      return el("div.hint", {
        text: "These are open for editing. They take effect only after the mixer restarts, and every source is rebuilt when it does.",
      });
    }
    return el("button.btn", {
      text: `Change the ${group.title.toLowerCase()}`,
      onclick: async () => {
        const ok = await confirmModal(
          "The canvas is what every source, every graphic and the output encoder are built around. " +
            "Changing it needs the mixer restarted, and the programme is off air while that happens. " +
            "Do it before you go live, not during. Open these fields anyway?",
          "Open them"
        );
        if (!ok) return;
        unlocked.add(group.table);
        draw();
      },
    });
  }

  function fieldFor(info, locked) {
    const control = controlFor(info, locked, (value) => {
      read[info.key] = value;
    });
    const shown = info.file_value === undefined ? info.value : info.file_value;
    read[info.key] = shown;
    const note = [describeDefault(info)];
    if (info.source === "pending restart") {
      note.push(`Saved as ${JSON.stringify(info.file_value)}; the mixer is still on ${JSON.stringify(info.value)}.`);
    }
    const row = el("div.field", {}, [
      el("label", {}, [el("span.lbl", { text: label(info) }), control]),
      el("span.hint", { text: `${info.about} ${note.join(" ")}` }),
    ]);
    if (info.source === "pending restart") row.classList.add("pending");
    return row;
  }

  function footer() {
    const save = el("button.btn.primary", { text: "Save", onclick: () => onSave(false) });
    const saveRestart = el("button.btn", { text: "Apply and restart", onclick: () => onSave(true) });
    const row = el("div.row", {}, [save]);
    // Only where the core says something would start it again. Anywhere else
    // the button would be a promise this mixer cannot keep.
    if (doc.supervised) row.appendChild(saveRestart);
    else if (pendingKeys(doc).length) {
      row.appendChild(
        el("span.hint", {
          text: "Restart the mixer yourself to pick these up: nothing here is supervising it.",
        })
      );
    }
    return row;
  }

  async function onSave(thenRestart) {
    const changes = changesFrom(doc.keys, read);
    if (!Object.keys(changes).length && !thenRestart) {
      toast({ text: "Nothing changed." });
      return;
    }
    let answer;
    try {
      answer = await client.call("config.set", { changes });
    } catch (e) {
      toast({ text: `Not saved: ${e.message || e}`, kind: "error" });
      return;
    }
    toast({ text: saidWhat(answer) });
    if (thenRestart) {
      try {
        await client.call("core.restart", {});
        toast({ text: "The mixer is restarting. The page will reconnect on its own." });
        return;
      } catch (e) {
        toast({ text: `Saved, but it will not restart: ${e.message || e}`, kind: "error" });
      }
    }
    await load();
  }

  load();
  return root;
}

/** The sentence a save answers with. Exported because it is worth testing. */
export function saidWhat(answer) {
  const on = (answer.applied || []).length;
  const waiting = (answer.needs_restart || []).length;
  if (!on && !waiting) return "Nothing changed.";
  if (!waiting) return `${on} setting${on === 1 ? "" : "s"} in force now.`;
  if (!on) return `${waiting} setting${waiting === 1 ? "" : "s"} saved, waiting for a restart.`;
  return `${on} in force now, ${waiting} waiting for a restart.`;
}

/** A readable name out of a dotted key: `min_hold_ms` becomes "Min hold". */
export function label(info) {
  const last = info.key.split(".").slice(1).join(" ");
  const words = last
    .replace(/_/g, " ")
    .replace(/\b(ms|secs|kbps|fps)\b/g, "")
    .replace(/\s+/g, " ")
    .trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

function controlFor(info, locked, onChange) {
  if (info.kind === "boolean") {
    const input = el("input", { type: "checkbox", checked: !!valueOf(info), style: { width: "auto" } });
    input.disabled = locked;
    on(input, "change", () => onChange(input.checked));
    return input;
  }
  if (info.kind === "integer") {
    const input = el("input", { type: "number", value: String(valueOf(info)) });
    input.disabled = locked;
    on(input, "change", () => {
      const n = Number(input.value);
      if (Number.isFinite(n)) onChange(Math.round(n));
    });
    return input;
  }
  const input = el("input", { type: "text", value: String(valueOf(info) ?? "") });
  input.disabled = locked;
  on(input, "change", () => onChange(input.value));
  return input;
}

/// What the field shows: the file's value where one is waiting, because that
/// is what the operator last asked for and what a restart would give them.
function valueOf(info) {
  return info.file_value === undefined ? info.value : info.file_value;
}
