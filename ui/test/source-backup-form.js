// A source's backup, the other half: the settings section that sets one and
// the list under the programme. The fixtures are source-backup.js's.

import { ruleValues, ruleRequest, backupChoices, backupWarning, NOTHING } from "../panels/sources/backup-rule.js";
import { backupSection } from "../panels/sources/backup-form.js";
import { lazyBanner } from "../panels/sources/backup-slot.js";
import { BackupBanner, standingIn } from "../panels/multiview/backup-banner.js";
import { MAIN, WIDE, RULE, at, standIn, settle, loaded } from "./source-backup.js";

/** Pick a backup in a fresh section and read what it says after. */
async function pick(client, backup) {
  const section = backupSection(client, { ...WIDE, on_loss: undefined });
  document.body.appendChild(section.node);
  const restHidden = section.node.querySelector("label.inline").parentNode.hidden;
  section.boxes.backup.value = backup;
  section.boxes.backup.dispatchEvent(new Event("change"));
  await settle();
  const said = section.node.querySelector("[role=status]").textContent;
  section.node.remove();
  document.querySelectorAll(".toast").forEach((t) => t.remove());
  return { restHidden, said };
}

export async function sourceBackupFormTests(test, eq, ok) {
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
  const saved = await pick(client, "cam-main");
  const refused = await pick(standIn([WIDE, MAIN], "cam-main backs up cam-wide already, so this would loop: cam-wide, cam-main, cam-wide."), "cam-main");
  test("picking a backup in the settings saves it at once and says what it means", () => {
    ok(saved.restHidden, "the timings showed with no backup picked");
    eq(client.sent, [["source.on_loss.set", { id: "cam-wide", backup: "cam-main", after_secs: 3, transition: "fade", return: "auto", return_after_secs: 5 }]]);
    eq(saved.said, "Saved. If Cam wide goes down, Cam main goes on after 3 s. It switches back by itself.");
    ok(refused.said.includes("would loop"), "the core's reason was not shown: " + refused.said);
  });

  test("the programme lists only the sources that are down or on a backup", () => {
    const state = { sources: [WIDE, at("ready", {}), { ...at("on_backup", { showing: "cam-wide", lost_ms: 5000 }), id: "cam-two", name: "Cam two" }] };
    eq(standingIn(state).map((s) => s.id), ["cam-two"]);
    const banner = new BackupBanner(standIn(state.sources));
    banner.sync(state);
    ok(!banner.node.hidden);
    eq(banner.node.textContent, "Cam two lost 0:05, showing Cam wide");
    banner.sync({ sources: [WIDE] });
    ok(banner.node.hidden && !banner.node.children.length, "the line outlived the outage");
  });

  const slot = lazyBanner(standIn([]));
  slot.sync({ sources: [WIDE, MAIN] });
  await settle(30);
  const idle = !slot.part;
  slot.sync({ sources: [WIDE, at("lost", { switch_in_ms: 2000 })] });
  await loaded(slot);
  test("the programme fetches its list only once a source is down", () => {
    ok(idle, "the list was fetched with every source fine");
    ok(slot.node.textContent.startsWith("Cam main lost, backup in 2 s"), slot.node.textContent);
  });
}
