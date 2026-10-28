// Import a Flussonic config: pasted, or a file on the station's machine.
// One of the ways to a headend's channels, beside adding one by hand and
// scanning a feed; each stream becomes a channel the station keeps.
// The report comes first (headend.import with dry_run, the default): every
// stream as the show it becomes, what would be refused, every line not
// carried. Make the shows sends the same config with dry_run false.
// Opened from the headend view's band and from File > Import a Flussonic
// config.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { errorToast, toast } from "../../shell/toast.js";
import { sheet } from "./sheet.js";
import { notesList, streamTable, summary } from "./import-report.js";

const ONCE = "The import reads the config once and does not follow it. To follow a file and answer Flussonic's URLs and API as well, start the station with godwinmix headend --flussonic FILE.";

/** The params of headend.import from what was typed, or `{error}`. */
export function importParams(how, text, path, dry) {
  if (how === "path") {
    const p = String(path || "").trim();
    return p ? { path: p, dry_run: dry } : { error: "Give the path of the config on the station's machine." };
  }
  return String(text || "").trim() ? { text, dry_run: dry } : { error: "Paste the config first." };
}

export async function importFlussonic(client, opts = {}) {
  sheet("headend");
  sheet("tiles");
  let following = opts.following;
  if (following === undefined) following = await client.call("headend.status", {}).catch(() => null);
  let how = "text";
  const text = el("textarea.he-imp-text", { rows: 10, spellcheck: false, "aria-label": "The Flussonic config", placeholder: "http 8090;\nstream CH117 {\n  input mpts-udp://eth1@239.10.0.1:2001?program=7;\n  transcoder vb=1500k size=1280x720 ab=128k;\n}" });
  const path = el("input", { type: "text", spellcheck: false, autocomplete: "off", "aria-label": "Path on the station", placeholder: "/etc/flussonic/flussonic.conf" });
  const tab = (id, label) => el("button.btn.he-tog", { type: "button", "data-how": id, "aria-pressed": String(id === how), text: label });
  const tabs = el("div.he-imp-tabs", { role: "group", "aria-label": "Where the config is" }, [tab("text", "Paste it"), tab("path", "A file on the station")]);
  const pathField = el("label.field", { hidden: true }, [el("span.lbl", { text: "Path on the station's machine" }), path, el("span.hint", { text: "Read by the station, so it is a path on that machine, not this one." })]);
  const ask = el("div.he-imp-ask", {}, [
    el("p.dim.sm", { style: { marginTop: "0" }, text: "Each stream becomes a channel the station keeps: its input, one HLS output with MPEG-TS and CMAF open to the local network, and the transcoder line as its conversion. You can edit every one of them here afterwards, like a channel added by hand." }),
    following && following.following ? el("p.he-imp-warn", { text: `This station follows ${following.file}. Channels imported here are kept by the station and are not written into that file. The file's own streams can be edited on the channel list too, which takes each one out of the file's hands.` }) : null,
    tabs, el("div.field", {}, [text]), pathField,
    el("p.he-calm", { text: ONCE }),
  ]);
  const shown = el("div.he-imp-report", { hidden: true });
  const read = el("button.btn.primary", { type: "button", text: "Read it" });
  const back = el("button.btn", { type: "button", text: "Back", hidden: true });
  const make = el("button.btn.primary", { type: "button", hidden: true });
  const close = el("button.btn", { type: "button", text: "Cancel" });
  const m = modal({ title: "Import a Flussonic config", wide: true, body: el("div.he-imp", {}, [ask, shown]), footer: [el("span.grow"), close, back, read, make] });
  close.onclick = () => m.close();

  tabs.addEventListener("click", (e) => {
    const b = e.target.closest("[data-how]");
    if (!b) return;
    how = b.dataset.how;
    for (const t of tabs.children) t.setAttribute("aria-pressed", String(t.dataset.how === how));
    text.closest(".field").hidden = how !== "text";
    pathField.hidden = how !== "path";
  });

  const step = (report) => {
    ask.hidden = !!report;
    shown.hidden = !report;
    read.hidden = !!report;
    back.hidden = !report || !report.dry_run;
    const n = report ? (report.shows.added || []).length : 0;
    make.hidden = !report || !report.dry_run;
    make.disabled = !n;
    make.textContent = n === 1 ? "Make 1 show" : `Make ${n} shows`;
    close.textContent = report && !report.dry_run ? "Close" : "Cancel";
    if (report) shown.replaceChildren(el("p.he-imp-sum", { text: `${report.source}: ${summary(report)}` }), streamTable(report), notesList(report));
  };

  async function send(dry, button) {
    const params = importParams(how, text.value, path.value, dry);
    if (params.error) return toast({ kind: "warning", text: params.error });
    button.disabled = true;
    try {
      const report = await client.call("headend.import", params);
      step(report);
      if (!dry) toast({ text: summary(report) });
      if (!dry && opts.onDone) opts.onDone(report);
    } catch (e) {
      errorToast(e, dry ? "The config was not read" : "The shows were not made");
    }
    button.disabled = false;
  }

  read.onclick = () => send(true, read);
  make.onclick = () => send(false, make);
  back.onclick = () => step(null);
  return m;
}
