// Recording uses the same public output methods as any other destination.
import { el } from "../../shell/dom.js";
import { toast, errorToast } from "../../shell/toast.js";

export const isRecording = (output) => output.type === "record/output";

/** The Record dialog, in record-start.js, fetched when Record is pressed. */
export function startRecording(client) {
  return import("./record-start.js").then((m) => m.startRecording(client));
}

export function recordingState(output) {
  if (output.state === "live") return { label: "Recording", dot: "live" };
  if (output.state === "connecting") return { label: "Preparing recording", dot: "connecting" };
  if (output.state === "reconnecting") return { label: "Restarting recording", dot: "connecting" };
  return { label: "Recording needs attention", dot: "failed" };
}

export function recordingRow(client, output) {
  const status = recordingState(output);
  let current = output;
  const dot = el("span.dot");
  const label = el("strong");
  const path = el("div.sm.dim", { style: { overflowWrap: "anywhere" } });
  const sent = el("div.sm.faint");
  const node = el("div.output-row.recording-row", {}, [
    el("div.row", {}, [
      dot,
      label,
      el("span.grow"),
      el("button.btn.danger", { text: "Stop recording", onclick: async (event) => {
        const button = event.currentTarget;
        button.disabled = true;
        try {
          await client.call("output.remove", { id: current.id });
          toast({ text: "Recording stopped. The mixer is finishing the file." });
        } catch (e) { button.disabled = false; errorToast(e, "Stop recording"); }
      } }),
    ]),
    status.dot === "failed" ? el("p.sm.dim", { text: "Open Alerts for the error, then stop this recording and start it again." }) : null,
    path,
    sent,
  ]);
  // Written into rather than rebuilt: the megabytes move several times a
  // second, and a Stop button remade that often cannot be pressed. See the
  // Outputs panel's `render`.
  const set = (el, key, value) => { if (el[key] !== value) el[key] = value; };
  const update = (next) => {
    current = next;
    const now = recordingState(next);
    set(dot, "className", "dot " + now.dot);
    set(label, "textContent", now.label);
    set(path, "textContent", next.recording_path || "Preparing a new file");
    set(sent, "textContent", `${((next.bytes_muxed || 0) / 1048576).toFixed(1)} MB sent to the file`);
  };
  return { node, update, shape: "recording:" + (status.dot === "failed" ? "failed" : "ok") };
}
