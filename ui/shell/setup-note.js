// What the page says while the mixer sets a piece up for itself.
//
// The first web page, or the first camera, needs something the mixer builds
// or installs once (`setup.changed`, see docs/reference/setup.md). This keeps
// one note per piece on screen while it runs, with the core's own sentence,
// and swaps it for a short "ready" or for the failure and its button. A
// failure that only the operating system can fix carries a command, which is
// shown with a copy button rather than described.

import { toast } from "./toast.js";

const notes = new Map(); // piece -> the running note's close function

/** Wire the notes to a client. Called once by the shell. */
export function setupNotes(client) {
  return client.on("setup", (s) => show(client, s));
}

function show(client, s) {
  if (!s || !s.piece) return;
  const running = notes.get(s.piece);
  if (s.state === "running") {
    if (running && running.setText) return running.setText(s.message);
    notes.set(s.piece, toast({ text: s.message, ms: 30 * 60 * 1000 }));
    return;
  }
  if (running) running();
  notes.delete(s.piece);
  if (s.state === "ready") return toast({ text: s.message, ms: 6000 });
  if (s.state === "failed" || s.state === "unavailable") toast({ kind: "error", text: s.message, ms: 60000, actions: buttons(client, s) });
}

/** Try again, and a command to copy when the fix is the machine's. */
export function buttons(client, s) {
  const out = [];
  const a = s.action || {};
  if (a.command) out.push({ label: "Copy the command", run: () => copy(a.command) });
  if (a.kind === "setup") out.push({ label: a.label || "Try again", run: () => client.call("setup.start", { piece: a.piece || s.piece }) });
  return out;
}

/** Put a command on the clipboard, and say so either way. */
export async function copy(command) {
  try {
    await navigator.clipboard.writeText(command);
    toast({ text: `Copied: ${command}` });
  } catch {
    toast({ text: `Run this in a terminal: ${command}`, ms: 30000 });
  }
}
