// Bringing a scene collection across from OBS Studio, from the page.
//
// It used to print `gmx import obs ~/.config/obs-studio/...` and stop, which
// is a graphical importer that cannot import anything. Now the browser reads
// the file the person picked and `scene.import.obs` takes its text, so the
// collection can come off a laptop that is not the machine running the mixer.
//
// That method adds scenes and not sources: a scene refers to a source by id,
// and whether this machine can actually open that camera is not something an
// import can know. So the second half of this file walks the report it
// answers with and offers to add each one, saying plainly which cannot be
// added here rather than leaving an empty scene for somebody to find on air.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";

/** The Import from OBS tile. */
export function importFromObs(client) {
  const file = el("input", { type: "file", accept: ".json,application/json" });
  const note = el("p.sm.dim", {
    text:
      "Export it from OBS with Scene Collection, then Export. On Windows the collections " +
      "are under %APPDATA%\\obs-studio\\basic\\scenes, on macOS under ~/Library/Application " +
      "Support/obs-studio/basic/scenes, and on Linux under ~/.config/obs-studio/basic/scenes.",
  });
  const go = el("button.btn.primary", { text: "Import", disabled: true });
  file.onchange = () => {
    go.disabled = !(file.files && file.files.length);
  };

  const body = el("div.col", {}, [
    el("p", {
      text:
        "GodwinMix keeps your scenes, the items in them and where they sit. Pick the " +
        "collection file and it comes across.",
      style: { marginTop: "0" },
    }),
    el("div.form", {}, [el("div.field", {}, [el("label", {}, [el("span.lbl", { text: "Collection file" }), file])])]),
    note,
  ]);

  const m = modal({
    title: "Import from OBS",
    body,
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), go],
  });

  go.onclick = async () => {
    const chosen = file.files && file.files[0];
    if (!chosen) return;
    go.disabled = true;
    note.textContent = "Reading the collection.";
    let text;
    try {
      text = await chosen.text();
    } catch (e) {
      errorToast(e, "Reading the collection");
      go.disabled = false;
      return;
    }
    let report;
    try {
      report = await client.call("scene.import.obs", { json: text, name: chosen.name });
    } catch (e) {
      errorToast(e, "Importing the collection");
      note.textContent = "";
      go.disabled = false;
      return;
    }
    m.close();
    showSources(client, report);
  };
  return m;
}

/**
 * The second half: the sources those scenes refer to.
 *
 * A row per source, with Add beside the ones this build can open and the
 * reason beside the ones it cannot. "Add all" is the common case, because
 * somebody importing a collection wants all of it.
 */
function showSources(client, report) {
  const scenes = (report && report.scenes) || [];
  const rows = candidates(report);
  const body = el("div.col");
  body.appendChild(
    el("p", {
      text: `${scenes.length} scene(s) came across, with ${(report && report.items) || 0} item(s) in them.`,
      style: { marginTop: "0" },
    })
  );
  for (const line of (report && report.skipped) || []) {
    body.appendChild(el("p.sm.dim", { text: line }));
  }

  const list = el("ol.wizard-list");
  const adders = [];
  for (const row of rows) {
    const made = sourceRow(client, row);
    adders.push(made);
    list.appendChild(made.el);
  }
  if (rows.length) {
    body.appendChild(
      el("p.sm", {
        text:
          "The scenes refer to these sources. Nothing shows a picture until they are " +
          "added, because only this machine knows whether it can open them.",
      })
    );
    body.appendChild(list);
  }

  const addable = adders.filter((a) => a.addable);
  const all = el("button.btn.primary", {
    text: `Add all ${addable.length}`,
    onclick: async () => {
      all.disabled = true;
      for (const a of addable) await a.add();
      all.remove();
    },
  });
  const footer = [el("button.btn", { text: "Done", onclick: () => m.close() })];
  if (addable.length > 1) footer.push(all);
  const m = modal({ title: "What came across", body, footer, wide: rows.length > 0 });
  return m;
}

/**
 * Every source in the report, with what this machine can do about it.
 *
 * Two lists have to be read together. `add_sources` is the ones the import
 * turned into something addable, each already in the shape `source.add`
 * takes. `source_report` is every OBS source with what became of it, so the
 * ones that are not in the first list can be named with the reason instead of
 * disappearing.
 */
function candidates(report) {
  const addable = (report && report.add_sources) || [];
  const byId = new Map(addable.map((s) => [s.id, s]));
  const detailed = (report && report.source_report) || [];
  if (detailed.length) {
    return detailed.map((s) => {
      const add = byId.get(s.id) || null;
      return {
        name: s.obs_name || s.obs_type || "",
        type: (add && add.type) || s.type || s.obs_type || "",
        add: add && add.uri ? add : null,
        why: reasonFor(s, add),
      };
    });
  }
  return addable.map((s) => ({ name: s.name || s.id, type: s.type || "", add: s, why: "" }));
}

/**
 * Why a source cannot be added on this machine, or "" when it can.
 *
 * A capture device is the one that matters. OBS was reading a camera by a
 * name that means something on the machine OBS ran on and nothing here, so
 * the row says which camera rather than leaving a scene silently black.
 */
function reasonFor(s, add) {
  const outcome = String((s && s.outcome) || "");
  if (outcome === "skipped") {
    return s.reason || `nothing in this build opens an OBS ${s.obs_type || "source"}`;
  }
  if (outcome === "needs_plugin") {
    return `it needs the ${s.plugin} plugin, which is not installed here`;
  }
  if (!add || !add.uri) {
    return `this machine has no ${s.obs_type || "device"} called ${s.obs_name || "that"}, so it has to be picked here`;
  }
  return "";
}

/** One source, and the button that adds it. */
function sourceRow(client, row) {
  const addable = !!row.add && !row.why;
  const dot = el("span.dot." + (addable ? "idle" : "stalled"));
  const line = el("div.sm.dim", { text: row.why || row.type });
  const button = el("button.btn", { text: "Add" });
  const body = el("div.grow", {}, [el("strong", { text: row.name || "(unnamed)" }), line]);
  const node = el("li.wizard-row", {}, [dot, body]);
  if (addable) body.appendChild(el("div.row.wizard-form", {}, [button]));

  const add = async () => {
    if (!addable || button.disabled) return;
    button.disabled = true;
    try {
      await client.call("source.add", request(row));
    } catch (e) {
      errorToast(e, `Adding ${row.name}`);
      button.disabled = false;
      return;
    }
    button.remove();
    dot.className = "dot live";
    line.textContent = "Added.";
    toast({ text: `${row.name} added.` });
  };
  button.onclick = add;
  return { el: node, addable, add };
}

/**
 * What `source.add` is sent for one row.
 *
 * The import already worked out the id, the kind and whatever the kind needs,
 * so this only flattens it: `params` is a free table on the request and the
 * plugin reads it back out untouched.
 */
function request(row) {
  const s = row.add;
  return Object.assign({ id: s.id, name: s.name, uri: s.uri, type: s.type }, s.params || {});
}
