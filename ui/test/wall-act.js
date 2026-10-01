// The wall's clicks and keys against wall-stub.js: sorting, the filter,
// grouping, the cursor, acknowledging, opening a show, the compositing
// switch and its refusal, and a station without a show list or show.stats.

import { wallStub } from "./wall-stub.js";
import { wait, until, sent, key, dialogs, closeDialogs, openOn } from "./wall-help.js";

export async function actTests(test, eq, ok) {
  const stub = wallStub({ n: 40 });
  const view = await openOn(stub);
  const rows = () => [...view.root.querySelectorAll(".wl-row")];
  view.root.querySelector('[data-sort="name"]').click();
  await wait(40);
  test("a click on a header sorts by it, and again the other way", () => eq(rows()[0].dataset.id, "arts-1"));
  view.root.querySelector('[data-sort="name"]').click();
  await wait(40);
  test("the second click reverses it", () => eq(rows()[0].dataset.id, "weather-2"));

  const filter = view.root.querySelector(".wl-filter");
  filter.value = "multicast kids";
  filter.dispatchEvent(new Event("input"));
  await wait(40);
  test("the filter keeps what matches every word", () => eq(rows().map((r) => r.dataset.id).sort(), ["kids-1", "kids-2"]));
  filter.value = "";
  filter.dispatchEvent(new Event("input"));
  view.root.querySelector('[data-sort="alarms"]').click();
  view.root.querySelector('[data-top="group"]').click();
  await wait(40);
  test("group by state puts a band over each group", () => eq([...view.root.querySelectorAll(".wl-band > span:first-child")].map((b) => b.textContent).slice(0, 2), ["In alarm", "Warning"]));
  view.root.querySelector('[data-top="group"]').click();
  await wait(40);

  const scroll = view.root.querySelector(".wl-scroll");
  scroll.focus();
  key(scroll, "ArrowDown");
  key(scroll, "ArrowDown");
  await wait(40);
  const second = view.items[1].show.id;
  test("arrow keys move the cursor down the rows", () => {
    eq(view.cursor, second);
    ok(view.root.querySelector(`#wl-${second}`).classList.contains("cursor"), "drawn as the cursor");
  });
  const flashing = () => view.root.querySelectorAll(`#wl-${second} .wl-alarm.flash`).length;
  const was = flashing();
  key(scroll, "a");
  await wait(40);
  test("A acknowledges the row's alarms: they stop flashing and stay listed", () => {
    ok(was > 0, "it was flashing");
    eq(flashing(), 0);
    ok(view.root.querySelector(`#wl-${second} .wl-alarm`), "still listed");
  });

  key(scroll, "Enter");
  await until(() => dialogs().length);
  test("Enter on a direct show opens its detail", () => eq(dialogs().at(-1).querySelector("h2").textContent, stub.shows.find((s) => s.id === second).name));
  closeDialogs();

  const id = view.items[3].show.id;
  view.root.querySelector(`#wl-${id} [data-act=mix]`).click();
  await until(() => sent(stub, "show.set").length);
  test("the switch on a row sends show.set with compositing", () => eq(sent(stub, "show.set")[0].params, { id, compositing: true }));
  const sw = () => view.root.querySelector(`#wl-${id} [data-act=mix]`);
  await until(() => sw().getAttribute("aria-busy") === "true");
  test("while the switch's task runs, the row says it is switching and a click does nothing", () => {
    eq([sw().getAttribute("aria-busy"), sw().textContent], ["true", "Switching"]);
    ok([...document.querySelectorAll(".toast")].some((t) => /Switching .* to mixed/.test(t.textContent)), "said it started");
    sw().click();
    eq(sent(stub, "show.set").length, 1);
  });
  await until(() => sw().getAttribute("aria-busy") === "false");
  test("once task.get says it is done, the row is mixed and says so", () => {
    ok(sent(stub, "task.get").length >= 1, "it read the task");
    eq([sw().getAttribute("aria-checked"), sw().textContent], ["true", "Mixed"]);
    ok([...document.querySelectorAll(".toast")].some((t) => /is mixed now/.test(t.textContent)), "said it landed");
  });
  const show = stub.shows.find((s) => s.id === id);
  show.scenes_in_use = 3;
  await wait(60);
  view.root.querySelector(`#wl-${id} [data-act=mix]`).click();
  await until(() => sent(stub, "show.set").length > 1);
  await wait(60);
  test("a refusal puts the switch back and says why", () => {
    eq(view.root.querySelector(`#wl-${id} [data-act=mix]`).getAttribute("aria-checked"), "true");
    ok([...document.querySelectorAll(".toast")].some((t) => /cannot go direct/.test(t.textContent)), "said");
  });
  view.close();

  const bare = wallStub({ noShows: true, n: 1 });
  const { toggleWall } = await import("../panels/wall/view.js");
  const lone = toggleWall(bare);
  await until(() => !lone.root.querySelector(".wl-empty").hidden && lone.data.loaded);
  test("a station without a show list says so plainly", () => ok(/no list of shows/.test(lone.root.querySelector(".wl-empty").textContent), lone.root.querySelector(".wl-empty").textContent));
  lone.close();

  const older = wallStub({ n: 3 });
  const inner = older.call;
  older.call = async (m, p) => {
    if (m !== "show.stats") return inner(m, p);
    older.calls.push({ method: m, params: p });
    throw Object.assign(new Error("there is no method 'show.stats'."), { code: -32004, data: { kind: "method", id: m } });
  };
  const wave3 = toggleWall(older);
  await until(() => wave3.root.querySelector(".wl-row"));
  const pics = older.thumbAsked.length;
  await wait(2300);
  test("a station from before show.stats is asked once, and for no pictures after", () => {
    eq(sent(older, "show.stats").length, 1);
    eq(older.thumbAsked.length, pics);
  });
  wave3.close();
}
