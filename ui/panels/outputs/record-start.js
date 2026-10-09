// Record: the dialog that asks where the file goes, then starts it through
// output.add like any other destination.
import { el, coarse } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast } from "../../shell/toast.js";

export function startRecording(client) {
  const folder = el("input", { type: "text", placeholder: "Videos/GodwinMix in the mixer's home folder", autocomplete: "off" });
  const choose = el("button.btn", { text: "Choose", title: "Walk the folders on the mixer", onclick: async () => {
    const { pickFolder } = await import("../../shell/folder-picker.js");
    const chosen = await pickFolder(client, { start: folder.value.trim(), title: "Where recordings go" });
    if (chosen) folder.value = chosen;
  } });
  // The default written out, so Start works on the first press and the person
  // sees where the file will go. The recorder makes the folder if it is new.
  client.call("path.list", {}).then((home) => {
    const sep = home.path.includes("\\") ? "\\" : "/";
    if (!folder.value) folder.value = [home.path, "Videos", "GodwinMix"].join(sep);
  }).catch(() => {});
  const format = el("select", {}, [el("option", { value: "mp4", text: "MP4 (fragmented)" }), el("option", { value: "mkv", text: "Matroska" })]);
  const error = el("p", { role: "alert", hidden: true });
  const start = el("button.btn.primary", { text: "Start recording" });
  const dialog = modal({ title: "Record the programme", body: el("div.col", {}, [
    el("p.dim", { text: "The file is saved on the mixer, at the same quality as the stream. Each start creates a new file." }),
    el("label.col", {}, [el("span", { text: "Folder on the mixer" }), el("div.row", {}, [el("span.grow", {}, [folder]), choose])]),
    el("label.col", {}, [el("span", { text: "File format" }), format]), error,
  ]), footer: [el("button.btn", { text: "Cancel", onclick: () => dialog.close() }), start] });
  start.onclick = async () => {
    start.disabled = true;
    error.hidden = true;
    try {
      const params = { format: format.value };
      if (folder.value.trim()) params.directory = folder.value.trim();
      await client.call("output.add", { id: `recording-${Date.now().toString(36)}`, uri: "record://programme", type: "record/output", params });
      dialog.close();
      toast({ text: "Recording requested. Outputs shows the file and its status." });
    } catch (e) {
      error.textContent = e.message || String(e);
      error.hidden = false;
      start.disabled = false;
    }
  };
  if (!coarse()) folder.focus();
}
