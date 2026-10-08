// Stop recording, from the header's REC button and Outputs > Stop recording,
// so a recording can be stopped whichever panel happens to be on screen. It
// asks once, then removes each recording with output.remove, the same call
// the Stop recording button on the Outputs panel's row makes.
import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";

export const recordings = (state) => ((state && state.outputs) || []).filter((o) => o.type === "record/output");

export function stopRecording(client) {
  const running = recordings(client.state);
  if (!running.length) {
    toast({ text: "Nothing is recording. Outputs > Record starts a recording." });
    return null;
  }
  const many = running.length > 1;
  const stop = el("button.btn.danger", { text: many ? `Stop ${running.length} recordings` : "Stop recording" });
  const dialog = modal({
    title: "Stop recording",
    body: el("div.col", {}, [
      el("p", { text: many ? "Stopping finishes these files and keeps them on the mixer:" : "Stopping finishes this file and keeps it on the mixer:" }),
      ...running.map((o) => el("p.sm.dim", { style: { overflowWrap: "anywhere", margin: "0" }, text: o.recording_path || "Preparing a new file" })),
      el("p.sm.dim", { text: "Allow a few seconds before moving it. Record starts a new file." }),
    ]),
    footer: [el("button.btn", { text: "Keep recording", onclick: () => dialog.close() }), stop],
  });
  stop.onclick = async () => {
    stop.disabled = true;
    try {
      for (const o of running) await client.call("output.remove", { id: o.id });
      dialog.close();
      toast({ text: "Recording stopped. The mixer is finishing the file." });
    } catch (e) {
      stop.disabled = false;
      errorToast(e, "Stop recording");
    }
  };
  return dialog;
}
