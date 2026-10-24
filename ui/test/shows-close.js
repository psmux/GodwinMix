// Closing a show from its tab, as quitting closes the app: the × and a
// middle click, at once with nothing live, the question when something is,
// what each answer sends, the last open tab, and opening a closed show
// again from File. Each case gets a row of its own, as in shows-switch.js.

import { showStub, sundayShows } from "./shows-stub.js";

const press = (node, type, button = 0) => node.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, button, detail: 1 }));

export async function closeTests(test, eq, ok, { row, until, wait, dialogs, button, closeAll, sent }) {
  const actions = await import("../shell/show-actions.js");
  const closing = await import("../shell/show-close.js");
  const went = [];
  actions.nav.go = (url) => went.push(url);
  const toasts = () => [...document.querySelectorAll(".toast")].map((t) => t.textContent).join(" ");
  const fresh = async (opts) => {
    went.length = 0;
    closeAll();
    const stub = showStub(opts);
    const node = await row(stub);
    return { stub, node, tab: (id) => node.querySelector(`[data-show="${id}"]`), x: (id) => node.querySelector(`[data-show="${id}"] .showtab-x`) };
  };

  let r = await fresh();
  test("every tab has a × to close it, hidden from a screen reader, which has Close show in the menu", () => {
    eq([...r.node.querySelectorAll(".showtab")].map((t) => !!t.querySelector(".showtab-x")), [true, true, true]);
    eq(r.x("kids").getAttribute("aria-hidden"), "true");
  });
  r.x("loop").click();
  await until(() => sent(r.stub, "show.close").length);
  await wait(80);
  test("× on a show with nothing live closes it at once, with no question", () => {
    eq(sent(r.stub, "show.close")[0].params, { id: "loop", keep_running: false });
    eq(dialogs().length, 0);
  });
  test("× never switches to the tab it closes", () => {
    eq(went.length, 0);
    eq(r.tab("main").getAttribute("aria-selected"), "true");
  });
  test("a closed show leaves the tab row and nothing of it is removed", () => {
    ok(!r.tab("loop"), "no tab");
    eq(sent(r.stub, "show.remove").length, 0);
    ok(r.stub.shows.some((s) => s.id === "loop" && s.closed), "still on the station, closed");
  });
  r.node.remove();

  r = await fresh();
  press(r.tab("kids"), "mousedown", 1);
  press(r.tab("kids"), "auxclick", 1);
  await until(() => dialogs().length);
  const kidsAsk = dialogs().at(-1);
  test("a middle click on a live show's tab asks first, naming what it sends", () => {
    ok(/Kids room is still running/.test(kidsAsk.textContent), kidsAsk.textContent);
    ok(/Streaming to hall-screen, live/.test(kidsAsk.textContent), kidsAsk.textContent);
  });
  test("the question offers to stop it, to keep it running, or to cancel, with Cancel chosen", () => {
    eq([...kidsAsk.querySelectorAll("footer button")].map((b) => b.textContent), ["Stop it and close", "Close and keep it running", "Cancel"]);
    ok(button(kidsAsk, "Stop it and close").classList.contains("danger"), "stopping is drawn as the dangerous one");
    eq(document.activeElement && document.activeElement.textContent, "Cancel");
  });
  button(kidsAsk, "Cancel").click();
  await wait(80);
  test("Cancel leaves every show as it was", () => {
    eq(sent(r.stub, "show.close").length, 0);
    eq([...r.node.querySelectorAll(".showtab")].length, 3);
    eq(went.length, 0);
  });
  r.x("kids").click();
  await until(() => dialogs().length);
  button(dialogs().at(-1), "Close and keep it running").click();
  await until(() => sent(r.stub, "show.close").length);
  await wait(80);
  test("Close and keep it running closes the tab and leaves the show on air", () => {
    eq(sent(r.stub, "show.close")[0].params, { id: "kids", keep_running: true });
    ok(/Kids room closed, and still running/.test(toasts()), toasts());
    eq(went.length, 0);
  });
  r.node.remove();

  r = await fresh();
  r.x("main").click();
  await until(() => dialogs().length);
  const mainAsk = dialogs().at(-1);
  test("the page's own show on air: on air, each stream and the recording are named", () => {
    for (const line of ["On air: Worship", "Streaming to YouTube, live", "Streaming to Facebook, live", "Recording"]) ok(mainAsk.textContent.includes(line), line);
  });
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await wait(80);
  test("Escape is Cancel", () => {
    eq(dialogs().length, 0);
    eq(sent(r.stub, "show.close").length, 0);
  });
  r.x("main").click();
  await until(() => dialogs().length);
  button(dialogs().at(-1), "Stop it and close").click();
  await until(() => went.length);
  test("Stop it and close on the page's own show asks over the next show's line, then the page moves there", () => {
    eq(r.stub.links.map((l) => [l.id, l.closed]), [["kids", true]]);
    eq(r.stub.links[0].calls.map((c) => [c.method, c.params]), [["show.close", { id: "main", keep_running: false }]]);
    ok(/[?&]show=kids\b/.test(went[0] || ""), went[0]);
  });
  r.node.remove();

  r = await fresh();
  const own = r.tab("main");
  own.querySelector(".showtab-x").dispatchEvent(new MouseEvent("dblclick", { bubbles: true, detail: 2 }));
  await wait(120);
  test("a double click on the × renames nothing", () => ok(!r.node.querySelector(".showtab-input"), "no rename box"));
  r.node.remove();

  const shows = sundayShows().map((s) => (s.id === "main" ? s : { ...s, closed: true }));
  r = await fresh({ shows });
  test("closed shows have no tab", () => eq([...r.node.querySelectorAll(".showtab")].map((t) => t.dataset.show), ["main"]));
  r.x("main").click();
  await wait(150);
  test("in a browser, × on the last open tab closes nothing and says the station keeps running", () => {
    eq(sent(r.stub, "show.close").length, 0);
    ok(/only open show.*leaves the station and the show running/.test(toasts()), toasts());
    eq(went.length, 0);
  });
  closing.shell.desktop = true;
  r.x("main").click();
  await until(() => went.length);
  closing.shell.desktop = false;
  test("in the desktop app, × on the last open tab is the app's own Quit", () => {
    eq(went, ["godwinmix://quit"]);
    eq(sent(r.stub, "show.close").length, 0);
  });

  const file = await import("../shell/show-file.js");
  await file.switchShow(r.stub);
  await until(() => document.querySelector(".menu"));
  const labels = [...document.querySelector(".menu").querySelectorAll("button")].map((b) => b.firstChild.textContent);
  test("File, Switch show lists the closed shows too, and says they are closed", () => eq(labels, ["✓ Sunday service, on air", "Kids room, closed, running", "Overnight loop, closed"]));
  closeAll();
  went.length = 0;
  try { localStorage.setItem(closing.LAST, "kids"); } catch { /* the menu is asked instead */ }
  await file.reopenShow(r.stub);
  await until(() => went.length);
  test("Reopen closed show opens the one closed last and goes there", () => {
    eq(sent(r.stub, "show.open")[0].params, { id: "kids" });
    ok(/[?&]show=kids\b/.test(went[0] || ""), went[0]);
  });
  r.node.remove();
  closeAll();
  actions.nav.go = (url) => location.assign(url);
}
