// Add shows in bulk: paste a list or drop a CSV, fix it in a table, check
// it with show.add_many's dry run, then add. Opened from the wall and from
// File, New shows. Nothing is sent until Check, and nothing is made until
// Add; a row the station refuses says why under itself.

import { el } from "../../shell/dom.js";
import { sheet } from "./sheet.js";
import { modal } from "../../shell/modal.js";
import { errorToast, toast } from "../../shell/toast.js";
import { TEMPLATE, parse, toShow } from "./bulk-parse.js";
import { editTable, planBox } from "./bulk-table.js";

export function bulkAdd(client, opts = {}) {
  sheet("dialogs");
  const rows = [];
  const s = { answer: null, busy: false, table: null, formats: [] };
  const text = el("textarea.wl-paste", { rows: 6, spellcheck: "false", "aria-label": "Feeds, one per line or as CSV", placeholder: "udp://@239.1.1.1:5000\nudp://@239.1.1.2:5000\n\nor CSV: name,input,program,output,format" });
  const file = el("input", { type: "file", accept: ".csv,.tsv,.txt,text/csv,text/plain", hidden: true });
  const tableSlot = el("div.wl-bslot");
  const planSlot = el("div.wl-planslot");
  const count = el("span.wl-bcount");
  const checkB = el("button.btn", { type: "button", text: "Check", disabled: true });
  const addB = el("button.btn.primary", { type: "button", text: "Add shows", disabled: true });
  const intro = el("section.wl-bintro", {}, [
    el("p.wl-bhelp", { text: "One feed per line, or CSV with the columns name, input, program, output and format. Paste it here or drop a CSV file on the box. A spreadsheet's copied cells work too." }),
    text,
    el("div.wl-brow-actions", {}, [
      el("button.btn", { type: "button", text: "Read the list", onclick: () => handle.read() }),
      el("button.btn", { type: "button", text: "Choose a CSV file", onclick: () => file.click() }),
      el("button.btn.wl-ghost", { type: "button", text: "Fill in an example", onclick: () => handle.paste(TEMPLATE) }),
      file,
    ]),
  ]);
  const body = el("div.wl-bulk", {}, [intro, planSlot, tableSlot]);
  const dlg = modal({ title: "Add shows", body, wide: true, footer: [count, el("span.grow"), checkB, addB] });

  const changed = () => {
    s.answer = null;
    planSlot.replaceChildren();
    for (const r of rows) delete r.why;
    const n = rows.length;
    count.textContent = n ? `${n} ${n === 1 ? "show" : "shows"}` : "";
    checkB.disabled = !n || s.busy;
    addB.disabled = true;
    addB.textContent = "Add shows";
  };

  const drop = async (f) => handle.paste(await f.text());
  text.addEventListener("dragover", (e) => { e.preventDefault(); text.classList.add("over"); });
  text.addEventListener("dragleave", () => text.classList.remove("over"));
  text.addEventListener("drop", (e) => { e.preventDefault(); text.classList.remove("over"); const f = e.dataTransfer.files[0]; if (f) drop(f); });
  file.addEventListener("change", () => file.files[0] && drop(file.files[0]));
  checkB.addEventListener("click", () => handle.check());
  addB.addEventListener("click", () => handle.apply());
  client.call("rendition.presets", {}).then((p) => { s.formats = ((p && p.presets) || []).filter((x) => !x.ladder && x.id !== "copy").map((x) => x.id); }, () => {});

  const handle = {
    dialog: dlg, text, rows,
    paste(t) { text.value = t; return handle.read(); },
    read() {
      rows.splice(0, rows.length, ...parse(text.value));
      if (!rows.length) return toast({ text: "There is nothing to read yet. Paste a list of feeds into the box first." });
      s.table = editTable(rows, changed, s.formats);
      tableSlot.replaceChildren(s.table.node);
      changed();
    },
    async check() { return run(true); },
    async apply() { return run(false); },
  };

  async function run(dry) {
    if (s.busy || !rows.length) return;
    s.busy = true;
    checkB.disabled = addB.disabled = true;
    try {
      const answer = await client.call("show.add_many", { shows: rows.map(toShow), dry_run: dry });
      dry ? checked(answer) : added(answer);
    } catch (e) {
      errorToast(e, dry ? "The list could not be checked" : "The shows were not added");
      checkB.disabled = false;
    } finally {
      s.busy = false;
    }
  }

  function checked(answer) {
    s.answer = answer;
    for (const r of answer.refused || []) if (rows[r.index]) rows[r.index].why = r.why;
    s.table.draw();
    planSlot.replaceChildren(planBox(answer, rows.length));
    // On a phone the answer is below the table; bring it up where it can be read.
    if (planSlot.scrollIntoView) planSlot.scrollIntoView({ block: "nearest" });
    const n = (answer.added || []).length;
    checkB.disabled = false;
    addB.disabled = !n;
    addB.textContent = n === rows.length ? `Add ${n} ${n === 1 ? "show" : "shows"}` : `Add the ${n} that ${n === 1 ? "is" : "are"} ready`;
  }

  function added(answer) {
    const n = (answer.added || []).length;
    const refused = answer.refused || [];
    toast({ text: `Added ${n} ${n === 1 ? "show" : "shows"}.${refused.length ? ` ${refused.length} refused: ${refused[0].name}, ${refused[0].why}` : ""}` });
    if (opts.onDone) opts.onDone(answer);
    if (!refused.length) return dlg.close();
    const kept = refused.map((r) => ({ ...rows[r.index], why: r.why })).filter((r) => r.name !== undefined);
    rows.splice(0, rows.length, ...kept);
    s.table.draw();
    changed();
    for (const r of rows) r.why = refused.find((x) => x.name === r.name)?.why;
    s.table.draw();
  }

  return handle;
}
