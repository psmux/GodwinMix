// The menu bar by mouse and by keyboard, and a project out of the live core
// and back in as a dry run.

import { menubar } from "../shell/menubar.js";
import { get as command } from "../shell/commands.js";
import { fileName, pageState } from "../shell/project.js";
import { connect } from "../client/index.js";

const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));

async function until(predicate, ms, what) {
  const started = Date.now();
  while (!predicate()) {
    if (Date.now() - started > ms) throw new Error(`timed out waiting for ${what}`);
    await tick(20);
  }
}

function key(target, name, opts = {}) {
  target.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true, ...opts }));
}

const drop = () => document.querySelector(".menubar-drop");

export async function menubarTests(test, eq, ok) {
  // A dialog an earlier suite left open holds Escape for itself; one Escape
  // closes every one of them, as it would for a person.
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await tick();
  const stub = { call: async () => ({}) };
  const wrap = menubar(stub);
  document.body.append(wrap);
  const bar = wrap.querySelector(".menubar");
  const titles = [...bar.querySelectorAll("[data-menu]")];

  test("the bar has the seven menus and registers the commands it adds", () => {
    eq(titles.map((t) => t.textContent), ["File", "Edit", "View", "Sources", "Scenes", "Outputs", "Help"]);
    eq(bar.getAttribute("role"), "menubar");
    for (const id of ["project.new", "project.open", "project.save", "view.studio", "output.record", "help.about"]) ok(command(id), `${id} is registered`);
    ok(typeof window.gmxMenu.run === "function", "the desktop app has a way in");
  });

  key(window, "F10");
  test("F10 puts the keyboard on File", () => eq(document.activeElement, titles[0]));
  const scrim = document.body.appendChild(Object.assign(document.createElement("div"), { className: "scrim" }));
  titles[0].blur();
  key(window, "F10");
  test("F10 does nothing while a dialog is up", () => ok(document.activeElement !== titles[0], "the bar took the keyboard from a dialog"));
  scrim.remove();
  key(window, "F10");
  key(titles[0], "ArrowRight");
  test("right arrow moves along the bar", () => eq(document.activeElement, titles[1]));
  key(titles[1], "ArrowDown");
  await until(() => drop() && drop().contains(document.activeElement), 3000, "the Edit menu to open with focus inside");
  const edit = drop();
  test("down arrow opens the menu with its first item focused", () => {
    eq(edit.getAttribute("aria-label"), "Edit");
    eq(titles[1].getAttribute("aria-expanded"), "true");
    ok(edit.querySelector("button"), "Edit has items");
  });
  key(edit, "ArrowRight");
  await until(() => drop() && drop().getAttribute("aria-label") === "View", 3000, "the View menu");
  test("right arrow inside a menu opens the next one, with ticks on what is showing", () => {
    ok(drop().querySelectorAll('[role="menuitemcheckbox"]').length >= 8, "panels and themes are check items");
    ok(drop().querySelector('[aria-checked="true"]'), "the current theme is ticked");
  });
  key(drop(), "Escape");
  await tick();
  test("Escape closes the menu and puts focus back on its title", () => {
    ok(!drop(), "no menu open");
    eq(document.activeElement, titles[2]);
    eq(titles[2].getAttribute("aria-expanded"), "false");
  });

  titles[4].click();
  await until(() => drop(), 3000, "the Scenes menu by mouse");
  test("a click opens a menu and a second click closes it", () => {
    eq(drop().getAttribute("aria-label"), "Scenes");
    ok([...drop().querySelectorAll("button")].some((b) => b.textContent.includes("Cut to black")), "Cut to black is in Scenes");
  });
  // A click right after a hover opened it is the same gesture; a second
  // click a moment later is a person closing it.
  await tick(450);
  titles[4].click();
  await tick(50);
  test("the second click closed it", () => ok(!drop(), "closed"));
  wrap.remove();

  test("a project is saved under its name with the project extension", () => {
    eq(fileName("Sunday service!"), "Sunday-service.gmxproject");
    eq(fileName(""), "project.gmxproject");
    const page = pageState();
    eq(page.version, 1);
    ok(page.settings && "producer" in page.settings, "the page's settings travel");
  });

  await liveProject(test, eq, ok);
}

async function liveProject(test, eq, ok) {
  const params = new URLSearchParams(location.search);
  if (params.get("live") === "0") return;
  let client;
  try {
    client = await connect({ token: params.get("token") });
    await until(() => client.state.connected, 5000, "the socket");
  } catch {
    return;
  }
  const file = await client.call("project.export", { name: "Harness", page: { version: 1 } });
  test("project.export answers one file with the parts of a project", () => {
    eq(file.format, "godwinmix.project");
    for (const part of ["settings", "sources", "outputs", "channels", "scenes", "media"]) ok(part in file, `the file has ${part}`);
  });
  const plan = await client.call("project.import", { file, mode: "merge" });
  test("opening it again is a dry run until asked, and says what it would do", () => {
    eq(plan.dry_run, true);
    ok(Array.isArray(plan.changes), "a list of changes");
  });
  let refused = null;
  try {
    await client.call("project.import", { file: { ...file, version: 99 } });
  } catch (e) {
    refused = e;
  }
  test("a file from a newer GodwinMix is refused with the reason", () => {
    ok(refused, "refused");
    ok(/format 99/.test(refused.message), refused && refused.message);
  });
  client.close && client.close();
}
