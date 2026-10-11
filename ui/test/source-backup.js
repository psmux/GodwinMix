// A source's backup, as an operator meets it: the line on its tile and under
// the programme while a camera is down, the buttons that bring it back, the
// offer of a backup on a tile that went down with none. The settings section
// and the programme list are source-backup-form.js. Against stand in clients,
// so no source has to fail for real.

import { buildTile, syncTile } from "../panels/sources/tile.js";
import { backupView, clock, secondsLeft, BackupClock } from "../panels/sources/backup-status.js";
import { wantsBackupHint } from "../panels/sources/backup-slot.js";

const NAMES = { "cam-main": "Cam main", "cam-wide": "Cam wide" };
export const nameFor = (id) => NAMES[id] || id;
export const RULE = { backup: "cam-wide", after_secs: 3, transition: "fade", return: "auto", return_after_secs: 5 };
export const MAIN = { id: "cam-main", name: "Cam main", uri: "test://ball", state: "live", has_video: true, has_audio: false, on_loss: RULE };
export const WIDE = { id: "cam-wide", name: "Cam wide", uri: "test://smpte", state: "live", has_video: true, has_audio: false };
export const at = (phase, extra) => ({ ...MAIN, backup: { phase, ...extra } });
const tileDeps = (log) => ({
  audio: { bindFader() {} }, scrub: {}, onMute() {}, meters: false,
  onGear: (id, section) => log.push(["gear", id, section]),
  onBackup: (method, id) => log.push([method, id]),
});
const labels = (node) => [...node.querySelectorAll("button")].map((b) => b.textContent);

export function standIn(sources, fail) {
  const client = {
    sent: [],
    state: { sources },
    call: async (method, params) => {
      client.sent.push([method, params]);
      if (fail) throw Object.assign(new Error(fail), { code: -32602 });
      return {};
    },
  };
  return client;
}

export const settle = (ms = 0) => new Promise((r) => setTimeout(r, ms));
export async function loaded(slot) {
  for (let i = 0; i < 200 && !slot.part; i += 1) await settle(10);
  if (!slot.part) throw new Error("the backup line never loaded");
}

export async function sourceBackupTests(test, eq, ok) {
  test("each phase reads as a sentence with names, not ids", () => {
    eq(backupView(at("lost", { switch_in_ms: 2000, lost_ms: 1000 }), nameFor).text, "Cam main lost, backup in 2 s");
    eq(backupView(at("on_backup", { showing: "cam-wide", lost_ms: 12000 }), nameFor).text, "Cam main lost 0:12, showing Cam wide");
    eq(backupView(at("returning", { showing: "cam-wide", return_in_ms: 3000 }), nameFor).text, "Cam main is back, returning in 3 s");
    eq(backupView(at("main_back", { showing: "cam-wide" }), nameFor).text, "Cam main is back, Cam wide is still on");
    eq(backupView(at("ready", {}), nameFor), null, "a source that is fine has a line");
  });

  test("a camera that never connected says so, not that it was lost", () => {
    eq(backupView(at("lost", { switch_in_ms: 2000, never_live: true }), nameFor).text, "Cam main has not connected yet, backup in 2 s");
    eq(backupView(at("on_backup", { showing: "cam-wide", lost_ms: 16000, never_live: true }), nameFor).text,
      "Cam main has not connected yet, showing Cam wide");
    eq(backupView(at("on_backup", { lost_ms: 16000, never_live: true }), nameFor).text,
      "Cam main has not connected yet, and its backup is down too");
  });

  test("the clocks run down from the snapshot by the time since it came", () => {
    eq(backupView(at("lost", { switch_in_ms: 2500 }), nameFor, 1000).text, "Cam main lost, backup in 2 s");
    eq(backupView(at("lost", { switch_in_ms: 2500 }), nameFor, 4000).text, "Cam main lost, switching to the backup");
    eq(backupView(at("on_backup", { showing: "cam-wide", lost_ms: 12000 }), nameFor, 50000).text, "Cam main lost 1:02, showing Cam wide");
    eq(backupView(at("returning", { return_in_ms: 3000 }), nameFor, 2500).text, "Cam main is back, returning in 1 s");
    eq([clock(0), clock(59999), clock(3725000)], ["0:00", "0:59", "1:02:05"]);
    eq(secondsLeft(500, 2000), 0);
  });

  test("a snapshot that has not changed keeps its clock running", () => {
    let now = 1000;
    const c = new BackupClock(() => now);
    const lost = at("lost", { switch_in_ms: 3000 });
    eq(c.elapsed(lost), 0);
    now = 2500;
    eq(c.elapsed({ ...lost }), 1500, "a redraw restarted the countdown");
    eq(c.elapsed(at("on_backup", { showing: "cam-wide", lost_ms: 3000 })), 0, "a new phase kept the old clock");
  });

  test("the buttons are the ones the moment calls for", () => {
    eq(backupView(at("lost", { switch_in_ms: 1 }), nameFor).actions, []);
    eq(backupView(at("on_backup", { showing: "cam-wide" }), nameFor).actions, []);
    eq(backupView(at("returning", { return_in_ms: 1 }), nameFor).actions.map((a) => [a.method, a.label]),
      [["source.backup.return", "Return now"], ["source.backup.stay", "Stay on backup"]]);
    eq(backupView(at("main_back", {}), nameFor).actions.map((a) => [a.method, a.label]), [["source.backup.return", "Back to Cam main"]]);
  });

  test("a backup that is down too is said out loud", () => {
    const view = backupView(at("on_backup", { lost_ms: 4000, warning: "cam-wide has no picture either, so the programme holds black." }), nameFor);
    eq(view.text, "Cam main lost 0:04, and its backup is down too");
    ok(view.warning.includes("no picture either"));
    const ready = backupView(at("ready", { warning: "cam-wide could not stand in: it has no picture." }), nameFor);
    ok(ready && ready.warning, "a warning while ready was dropped");
  });

  // The line loads the first time a tile has something to say.
  const log = [];
  const tile = buildTile(MAIN, tileDeps(log));
  syncTile(tile, MAIN, { names: nameFor });
  await settle(30);
  const nothingLoaded = !tile.backup.part && tile.backup.node.textContent === "";
  syncTile(tile, at("returning", { showing: "cam-wide", return_in_ms: 3000 }), { names: nameFor });
  await loaded(tile.backup);
  test("the tile shows the line and its buttons call the core, not a take", () => {
    ok(nothingLoaded, "a live source with a quiet rule fetched the backup line");
    const node = tile.backup.node;
    ok(!tile.backup.part.node.hidden);
    ok(node.textContent.startsWith("Cam main is back, returning in 3 s"), node.textContent);
    eq(labels(node), ["Return now", "Stay on Cam wide"]);
    let reachedTile = false;
    tile.node.addEventListener("click", () => (reachedTile = true));
    node.querySelectorAll("button")[1].click();
    eq(log, [["source.backup.stay", "cam-main"]]);
    ok(!reachedTile, "the click went on to the tile, which takes the source");
    syncTile(tile, at("main_back", { showing: "cam-wide" }), { names: nameFor });
    eq(labels(node), ["Back to Cam main"]);
    syncTile(tile, at("ready", {}), { names: nameFor });
    ok(tile.backup.part.node.hidden, "the line stayed after the source went back");
  });

  const bare = { ...WIDE, on_loss: undefined };
  const offers = [];
  const down = buildTile(bare, tileDeps(offers));
  syncTile(down, { ...bare, state: "stalled" }, { names: nameFor });
  await loaded(down.backup);
  test("a source that goes down with no backup offers one", () => {
    ok(!wantsBackupHint(bare), "a live source was offered a backup");
    ok(wantsBackupHint({ ...bare, state: "stalled" }));
    ok(wantsBackupHint({ ...bare, state: "failed" }));
    ok(!wantsBackupHint({ ...MAIN, state: "stalled" }), "a source with a rule was offered another");
    ok(!wantsBackupHint({ ...bare, state: "stalled", type: "text/source" }));
    // A camera that failed before its first frame has no video yet, and is
    // the camera that most needs one; a microphone has no picture to lose.
    ok(wantsBackupHint({ ...bare, state: "failed", has_video: false, has_audio: false }), "a camera that never connected was not offered one");
    ok(!wantsBackupHint({ ...bare, state: "stalled", has_video: false, has_audio: true }), "a microphone was offered a backup");
    ok(!down.backup.part.node.hidden);
    ok(down.backup.node.textContent.startsWith("No backup set"));
    down.backup.node.querySelector("button").click();
    eq(offers, [["gear", "cam-wide", "backup"]]);
    syncTile(down, bare, { names: nameFor });
    ok(down.backup.part.node.hidden, "the offer stayed after the source came back");
  });
}
