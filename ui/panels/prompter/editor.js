// The script being written: its title and its words, saved on the station.

import { el } from "../../shell/dom.js";
import { confirmModal } from "../../shell/modal.js";

const HINT = "A line that starts with # is a marker the presenter can jump to. *word* is italic, **word** is bold. Short lines read best.";

export function editor(client, { saved, removed, failed }) {
  let current = null;
  const title = el("input.grow", { type: "text", placeholder: "Title, for example Evening news", "aria-label": "Title" });
  const text = el("textarea.prompter-text", { rows: 12, spellcheck: true, "aria-label": "Script", placeholder: "# Opening\nGood evening and welcome." });
  const save = el("button.btn.primary", { text: "Save", onclick: () => run(false) });
  const saveLoad = el("button.btn", { text: "Save and put on the prompter", onclick: () => run(true) });
  const remove = el("button.btn.danger", { text: "Remove", onclick: () => drop() });
  const note = el("p.sm.dim", { text: HINT, style: { margin: 0 } });

  async function run(andLoad) {
    try {
      const params = { title: title.value, text: text.value };
      if (current) params.id = current.id;
      const script = await client.call("prompter.script.save", params);
      current = script;
      if (andLoad) await client.call("prompter.load", { id: script.id });
      paint();
      saved(script);
    } catch (e) {
      failed(e);
    }
  }

  async function drop() {
    if (!current) return;
    if (!(await confirmModal(`Remove the script ${current.title}? A prompter showing it goes blank.`, "Remove"))) return;
    try {
      await client.call("prompter.script.remove", { id: current.id });
      current = null;
      removed();
    } catch (e) {
      failed(e);
    }
  }

  function paint() {
    remove.disabled = !current;
    note.textContent = current ? `Saved as ${current.id}. ${HINT}` : HINT;
  }

  const root = el("div.col", {}, [el("div.row", {}, [title]), text, note, el("div.row.wrap", {}, [save, saveLoad, el("span.grow"), remove])]);
  paint();
  return {
    root,
    id: () => (current ? current.id : ""),
    show(script) {
      current = script;
      title.value = script ? script.title : "";
      text.value = script ? script.text : "";
      paint();
    },
  };
}
