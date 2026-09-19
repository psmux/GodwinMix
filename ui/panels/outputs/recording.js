// Recording uses the same public output methods as any other destination.
import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";

export const isRecording = (output) => output.type === "record/output";

export function startRecording(client) {
  const folder = el("input", { type: "text", placeholder: "Videos/GodwinMix in the mixer's home folder", autocomplete: "off" });
  const format = el("select", {}, [el("option", { value: "mp4", text: "MP4 (fragmented)" }), el("option", { value: "mkv", text: "Matroska" })]);
  const error = el("p", { role: "alert", hidden: true });
  const start = el("button.btn.primary", { text: "Start recording" });
  const dialog = modal({ title: "Record the programme", body: el("div.col", {}, [
    el("p.dim", { text: "The file is saved on the mixer, at the same quality as the stream. Each start creates a new file." }),
    el("label.col", {}, [el("span", { text: "Folder on the mixer" }), folder]),
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
  folder.focus();
}

export function recordingState(output) {
  if (output.state === "live") return { label: "Recording", dot: "live" };
  if (output.state === "connecting") return { label: "Preparing recording", dot: "connecting" };
  if (output.state === "reconnecting") return { label: "Restarting recording", dot: "connecting" };
  return { label: "Recording needs attention", dot: "failed" };
}

export function recordingRow(client, output) {
  const status = recordingState(output);
  return el("div.output-row.recording-row", {}, [
    el("div.row", {}, [
      el("span.dot." + status.dot),
      el("strong", { text: status.label }),
      el("span.grow"),
      el("button.btn.danger", { text: "Stop recording", onclick: async (event) => {
        const button = event.currentTarget;
        button.disabled = true;
        try {
          await client.call("output.remove", { id: output.id });
          toast({ text: "Recording stopped. The mixer is finishing the file." });
        } catch (e) { button.disabled = false; errorToast(e, "Stop recording"); }
      } }),
    ]),
    status.dot === "failed" ? el("p.sm.dim", { text: "Open Alerts for the error, then stop this recording and start it again." }) : null,
    el("div.sm.dim", { text: output.recording_path || "Preparing a new file", style: { overflowWrap: "anywhere" } }),
    el("div.sm.faint", { text: `${((output.bytes_muxed || 0) / 1048576).toFixed(1)} MB sent to the file` }),
  ]);
}
