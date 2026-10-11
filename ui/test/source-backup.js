// A source's backup, as an operator meets it: the line on its tile and under
// the programme while a camera is down, the buttons that bring it back, the
// offer of a backup on a tile that went down with none, and the settings
// section building the right `source.on_loss.set`. Against stand in clients,
// so no source has to fail for real.

import { buildTile, syncTile } from "../panels/sources/tile.js";
import { backupView, clock, secondsLeft, wantsBackupHint, BackupClock } from "../panels/sources/backup-status.js";
import { ruleValues, ruleRequest, backupChoices, backupWarning, NOTHING } from "../panels/sources/backup-rule.js";
import { backupSection } from "../panels/sources/backup-form.js";
import { BackupBanner, standingIn } from "../panels/multiview/backup-banner.js";

const NAMES = { "cam-main": "Cam main", "cam-wide": "Cam wide" };
const nameFor = (id) => NAMES[id] || id;
const RULE = { backup: "cam-wide", after_secs: 3, transition: "fade", return: "auto", return_after_secs: 5 };
const MAIN = { id: "cam-main", name: "Cam main", uri: "test://ball", state: "live", has_video: true, has_audio: false, on_loss: RULE };
const WIDE = { id: "cam-wide", name: "Cam wide", uri: "test://smpte", state: "live", has_video: true, has_audio: false };
const at = (phase, extra) => ({ ...MAIN, backup: { phase, ...extra } });
const tileDeps = (log) => ({
  audio: { bindFader() {} }, scrub: {}, onMute() {}, meters: false,
  onGear: (id, section) => log.push(["gear", id, section]),
  onBackup: (method, id) => log.push([method, id]),
});
const labels = (node) => [...node.querySelectorAll("button")].map((b) => b.textContent);

function standIn(sources, fail) {
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

const settle = () => new Promise((r) => setTimeout(r, 0));

export async function sourceBackupTests(test, eq, ok) {
  test("each phase reads as a sentence with names, not ids", () => {
    eq(backupView(at("lost", { switch_in_ms: 2000, lost_ms: 1000 }), nameFor).text, "Cam main lost, backup in 2 s");
    eq(backupView(at("on_backup", { showing: "cam-wide", lost_ms: 12000 }), nameFor).text, "Cam main lost 0:12, showing Cam wide");
    eq(backupView(at("returning", { showing: "cam-wide", return_in_ms: 3000 }), nameFor).text, "Cam main is back, returning in 3 s");
    eq(backupView(at("main_back", { showing: "cam-wide" }), nameFor).text, "Cam main is back, Cam wide is still on");
    eq(backupView(at("ready", {}), nameFor), null, "a source that is fine has a line");
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

  test("the tile shows the line and its buttons call the core, not a take", () => {
    const log = [];
    const tile = buildTile(MAIN, tileDeps(log));
    ok(tile.backup.node.hidden, "a live source shows a backup line");
    syncTile(tile, at("returning", { showing: "cam-wide", return_in_ms: 3000 }), { names: nameFor });
    ok(!tile.backup.node.hidden);
    ok(tile.backup.node.textContent.startsWith("Cam main is back, returning in 3 s"));
    eq(labels(tile.backup.node), ["Return now", "Stay on backup"]);
    let reachedTile = false;
    tile.node.addEventListener("click", () => (reachedTile = true));
    tile.backup.node.querySelectorAll("button")[1].click();
    eq(log, [["source.backup.stay", "cam-main"]]);
    ok(!reachedTile, "the click went on to the tile, which takes the source");
    syncTile(tile, at("main_back", { showing: "cam-wide" }), { names: nameFor });
    eq(labels(tile.backup.node), ["Back to Cam main"]);
  });

  test("a source that goes down with no backup offers one", () => {
    const bare = { ...WIDE, on_loss: undefined };
    ok(!wantsBackupHint(bare), "a live source was offered a backup");
    ok(wantsBackupHint({ ...bare, state: "stalled" }));
    ok(wantsBackupHint({ ...bare, state: "failed" }));
    ok(!wantsBackupHint({ ...MAIN, state: "stalled" }), "a source with a rule was offered another");
    ok(!wantsBackupHint({ ...bare, state: "stalled", type: "text/source" }));
    const log = [];
    const tile = buildTile(bare, tileDeps(log));
    syncTile(tile, { ...bare, state: "stalled" }, { names: nameFor });
    ok(!tile.backup.node.hidden);
    ok(tile.backup.node.textContent.startsWith("No backup set"));
    tile.backup.node.querySelector("button").click();
    eq(log, [["gear", "cam-wide", "backup"]]);
    syncTile(tile, bare, { names: nameFor });
    ok(tile.backup.node.hidden, "the offer stayed after the source came back");
  });

  test("the settings build the request the core takes", () => {
    eq(ruleValues({ id: "x" }), { backup: "", after_secs: 3, transition: "fade", auto: true, return_after_secs: 5 });
    eq(ruleValues({ ...MAIN, on_loss: { ...RULE, transition: "dissolve", return: "manual" } }).transition, "fade");
    const values = { backup: "cam-wide", after_secs: "2", transition: "wipe", auto: true, return_after_secs: "8" };
    eq(ruleRequest(WIDE, { ...values, backup: "cam-main" }), { method: "source.on_loss.set",
      params: { id: "cam-wide", backup: "cam-main", after_secs: 2, transition: "wipe", return: "auto", return_after_secs: 8 } });
    eq(ruleRequest(MAIN, { ...values, auto: false, after_secs: "" }).params,
      { id: "cam-main", backup: "cam-wide", after_secs: 3, transition: "wipe", return: "manual" });
    eq(ruleRequest(MAIN, { ...values, backup: "" }), { method: "source.on_loss.clear", params: { id: "cam-main" } });
    eq(ruleRequest(WIDE, { ...values, backup: "" }), null, "Nothing on a source with no rule sent something");
    eq(backupChoices(MAIN, [MAIN, WIDE], (s) => s.name), [["", NOTHING], ["cam-wide", "Cam wide"]]);
    eq(backupWarning(MAIN, "cam-wide", [MAIN, { ...WIDE, state: "stalled" }], (s) => s.name),
      "Cam wide has no picture right now, so it could not stand in until it does.");
  });

  const client = standIn([{ ...WIDE, on_loss: undefined }, MAIN]);
  const section = backupSection(client, { ...WIDE, on_loss: undefined });
  document.body.appendChild(section.node);
  const restHidden = section.node.querySelector("label.inline").parentNode.hidden;
  section.boxes.backup.value = "cam-main";
  section.boxes.backup.dispatchEvent(new Event("change"));
  await settle();
  const saved = section.node.querySelector("[role=status]").textContent;
  const refused = standIn([WIDE, MAIN], "cam-main backs up cam-wide already, so this would loop: cam-wide, cam-main, cam-wide.");
  const loop = backupSection(refused, { ...WIDE, on_loss: undefined });
  loop.boxes.backup.value = "cam-main";
  loop.boxes.backup.dispatchEvent(new Event("change"));
  await settle();
  const said = loop.node.querySelector("[role=status]").textContent;
  section.node.remove();
  document.querySelectorAll(".toast").forEach((t) => t.remove());
  test("picking a backup in the settings saves it at once and says what it means", () => {
    ok(restHidden, "the timings showed with no backup picked");
    eq(client.sent, [["source.on_loss.set", { id: "cam-wide", backup: "cam-main", after_secs: 3, transition: "fade", return: "auto", return_after_secs: 5 }]]);
    eq(saved, "Saved. If Cam wide goes down, Cam main goes on after 3 s. It switches back by itself.");
    ok(said.includes("would loop"), "the core's reason was not shown: " + said);
  });

  test("the programme lists only the sources that are down or on a backup", () => {
    const state = { sources: [WIDE, at("ready", {}), { ...at("on_backup", { showing: "cam-wide", lost_ms: 5000 }), id: "cam-two", name: "Cam two" }] };
    eq(standingIn(state).map((s) => s.id), ["cam-two"]);
    const banner = new BackupBanner(standIn(state.sources));
    banner.render(state);
    ok(!banner.node.hidden);
    eq(banner.node.textContent, "Cam two lost 0:05, showing Cam wide");
    banner.render({ sources: [WIDE] });
    ok(banner.node.hidden && !banner.node.children.length, "the line outlived the outage");
  });
}
