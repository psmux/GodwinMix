// Import from OBS: drop the scene collection on the dialog, or choose it.
//
// The page reads the file in the browser and sends its text to
// `scene.import.obs` with `add_sources`, so the collection never has to be on
// the mixer's machine and nobody types a path. The answer says what came
// across, what did not and why, and a source that waits on a plugin gets the
// same install row the preset checklist uses.

import { el, clear, on } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { errorToast } from "../../shell/toast.js";

/** Past this it is not a scene collection; the core refuses at 8 MB anyway. */
const MAX_BYTES = 8 * 1024 * 1024;

/**
 * @param {object} client
 * @param {{installRow: (client, plugin) => Node}} rows
 */
export function openObsImport(client, rows) {
  const result = el("div.col.obs-result");
  const input = el("input", { type: "file", accept: ".json,application/json", hidden: true });
  const zone = el("div.obs-drop", { tabindex: "0", role: "button" }, [
    el("strong", { text: "Drop your OBS scene collection here" }),
    el("span.sm.dim", { text: "or press to choose it" }),
  ]);
  const body = el("div.col", {}, [
    el("p", {
      text: "Your scenes, their items and their positions come across, and the sources they use are added.",
      style: { marginTop: "0" },
    }),
    el("p.sm.dim", { text: "In OBS, Scene Collection, then Export, saves the collection as a file." }),
    zone,
    input,
    result,
  ]);
  const take = (file) => file && importFile(client, file, { zone, result, rows });
  zone.onclick = () => input.click();
  on(zone, "keydown", (e) => (e.key === "Enter" || e.key === " ") && input.click());
  on(input, "change", () => take(input.files[0]));
  dropTarget(zone, (file) => take(file));
  const m = modal({
    title: "Import from OBS",
    body,
    footer: [el("button.btn.primary", { text: "Done", onclick: () => m.close() })],
  });
  return m;
}

/** The zone takes the drop itself, so the window's own drop handler never sees it. */
function dropTarget(zone, onFile) {
  const stop = (e) => {
    e.preventDefault();
    e.stopPropagation();
  };
  on(zone, "dragenter", (e) => { stop(e); zone.classList.add("over"); });
  on(zone, "dragover", stop);
  on(zone, "dragleave", () => zone.classList.remove("over"));
  on(zone, "drop", (e) => {
    stop(e);
    zone.classList.remove("over");
    document.body.classList.remove("dropping");
    onFile(e.dataTransfer && e.dataTransfer.files[0]);
  });
}

async function importFile(client, file, ui) {
  clear(ui.result);
  if (file.size > MAX_BYTES) {
    ui.result.appendChild(el("p.sm", { text: `${file.name} is too big to be a scene collection. Choose the .json OBS exported.` }));
    return;
  }
  ui.zone.classList.add("busy");
  try {
    const content = await file.text();
    const report = await client.call("scene.import.obs", { content, add_sources: true });
    ui.result.appendChild(reportView(client, report, ui.rows));
  } catch (e) {
    errorToast(e, `Importing ${file.name}`);
  } finally {
    ui.zone.classList.remove("busy");
  }
}

/** The answer, in the order a person wants it: what came, what did not, why. */
export function reportView(client, report, rows) {
  const box = el("div.col.sm");
  const scenes = report.scenes || [];
  const added = report.sources_added || [];
  box.appendChild(el("p", { text: summary(scenes.length, report.items || 0, added.length) }));
  if (scenes.length) box.appendChild(el("p.dim", { text: "Scenes: " + scenes.join(", ") }));
  if (added.length) box.appendChild(el("p.dim", { text: "Sources added: " + added.join(", ") }));
  const left = (report.sources_not_added || []).map((s) => `${s.id}: ${s.reason}`);
  const skipped = (report.skipped || []).concat(left);
  if (skipped.length) {
    box.appendChild(el("p", { text: "Not brought across:" }));
    box.appendChild(el("ul.obs-skipped", {}, skipped.map((line) => el("li", { text: line }))));
  }
  for (const f of report.filters_duplicated || []) {
    box.appendChild(el("p.dim", { text: `The ${f.filter} filter on ${f.source} was copied onto each place it is used.` }));
  }
  const plugins = [...new Set((report.sources_not_added || []).map((s) => s.plugin).filter(Boolean))];
  for (const name of plugins) box.appendChild(rows.installRow(client, { name }));
  return box;
}

function summary(scenes, items, sources) {
  const n = (count, one, many) => `${count} ${count === 1 ? one : many}`;
  return `Imported ${n(scenes, "scene", "scenes")} with ${n(items, "item", "items")}, and added ${n(sources, "source", "sources")}.`;
}
