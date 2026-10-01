// Add shows in bulk and a direct show's detail against wall-stub.js: the
// dry run, the refused row, the add, and every call the detail sends.

import { wallStub } from "./wall-stub.js";
import { wait, until, sent, button, closeDialogs } from "./wall-help.js";

export async function bulkTests(test, eq, ok) {
  const stub = wallStub({ n: 5 });
  const { bulkAdd } = await import("../panels/wall/bulk.js");
  let done = 0;
  const b = bulkAdd(stub, { onDone: () => (done += 1) });
  b.dialog.el.querySelector(".wl-ghost").click();
  await wait(40);
  test("the example is three multicast feeds with UDP outputs, in the table to fix", () => {
    eq(b.rows.map((r) => r.input), ["udp://@239.1.1.1:5000", "udp://@239.1.1.2:5000", "udp://@239.1.1.3:5000"]);
    eq(b.dialog.el.querySelectorAll(".wl-brow").length, 3);
  });
  const cell = b.dialog.el.querySelector('.wl-brow[data-row="2"] [data-field="name"]');
  cell.value = "News 1";
  cell.dispatchEvent(new Event("input"));
  button(b.dialog.el, "Check").click();
  await until(() => b.dialog.el.querySelector(".wl-plan"));
  test("Check is a dry run of the whole list; a refused row says why under itself", () => {
    const p = sent(stub, "show.add_many")[0].params;
    eq([p.dry_run, p.shows.length, p.shows[2].name, p.shows[0].compositing], [true, 3, "News 1", false]);
    ok(b.dialog.el.querySelector(".wl-brow.refused"), "the row is marked");
    ok(/already a show called News 1/.test(b.dialog.el.querySelector(".wl-bwhy").textContent), "and says why");
    eq(button(b.dialog.el, "Add the 2 that are ready") ? "yes" : "no", "yes");
  });
  button(b.dialog.el, "Add the 2 that are ready").click();
  await until(() => sent(stub, "show.add_many").length > 1);
  await wait(60);
  test("Add applies it; what was refused stays in the table to fix", () => {
    eq(sent(stub, "show.add_many")[1].params.dry_run, false);
    eq(stub.shows.length, 7);
    eq(b.rows.map((r) => r.name), ["News 1"]);
    eq(done, 1);
  });
  closeDialogs();
}

export async function detailTests(test, eq, ok) {
  const stub = wallStub({ n: 5 });
  const { showDetail } = await import("../panels/wall/detail.js");
  const show = stub.shows[1];
  const d = showDetail(stub, show);
  const box = d.dialog.el;
  await wait(80);
  const backup = box.querySelector('[aria-label="Backup input address"]');
  backup.value = "srt://backup:9000";
  button(box, "Save input").click();
  await until(() => sent(stub, "show.set").length);
  test("Save input sends the input with its backup", () => eq(sent(stub, "show.set")[0].params, { id: show.id, input: { uri: show.input.uri, backup: { uri: "srt://backup:9000" } } }));

  button(box, "Turn off").click();
  await until(() => sent(stub, "show.output.set").length);
  test("an output turns off with show.output.set", () => eq(sent(stub, "show.output.set")[0].params, { show: show.id, id: "udp-out", enabled: false }));

  button(box, "Add output").click();
  await until(() => box.querySelector(".wl-dbox .rnd-card"));
  await wait(80);
  box.querySelector('.wl-dbox input[type=text]').value = "srt://cdn.example:7000";
  button(box.querySelector(".wl-dbox"), "Add output").click();
  await until(() => sent(stub, "show.output.add").length);
  test("Add output sends the address and the format chosen, copy by default", () => eq(sent(stub, "show.output.add")[0].params, { show: show.id, uri: "srt://cdn.example:7000", rendition: null }));

  box.querySelector('[aria-label="Black after, in seconds"]').value = "3";
  button(box, "Save alarms").click();
  await until(() => sent(stub, "show.set").length > 1);
  test("alarm thresholds go with show.set, in milliseconds", () => eq(sent(stub, "show.set")[1].params.alarms.black_ms, 3000));
  d.close();
}
