// The "Nearly there" checklist, the restart bar and the OBS import report,
// against stub clients. No core needed.

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

export async function welcomeStepTests(test, eq, ok) {
  const { stepsOf, actionable, newContext, stepDone, checklist } = await import("../panels/welcome/checklist.js");
  const { leftLine, notesWorthShowing } = await import("../panels/welcome/after.js");

  test("a plan's checklist is used, and plain string steps become prose", () => {
    const structured = { checklist: [{ text: "Add it.", does: "add-key", target: "yt" }], steps: ["Add it."] };
    eq(stepsOf(structured)[0].does, "add-key");
    eq(stepsOf({ steps: ["Press a tile."] }), [{ text: "Press a tile." }]);
    ok(!actionable({ text: "Press a tile." }), "prose is not a button");
    ok(!actionable({ text: "x", does: "edit-toml", target: "a" }), "an unknown action is prose");
  });

  const state = {
    sources: [{ id: "cam" }],
    outputs: [{ id: "youtube", has_key: false }, { id: "local", has_key: true }],
    program: null,
    scene: null,
  };
  const ctx = newContext(state, { plugins: [{ name: "rtmp", installed: true }, { name: "ndi", installed: false }] });

  test("add-key ticks when the output gains a key, and a keyed one only after saving here", () => {
    const yt = { text: "k", does: "add-key", target: "youtube" };
    const local = { text: "k", does: "add-key", target: "local" };
    ok(!stepDone(yt, state, ctx), "youtube has no key yet");
    ok(stepDone(yt, { ...state, outputs: [{ id: "youtube", has_key: true }] }, ctx), "youtube now has one");
    ok(!stepDone(local, state, ctx), "a key it opened with is not the person's");
    ctx.saved.add("local");
    ok(stepDone(local, state, ctx), "saved from the list");
  });

  test("add-source ticks on a source added since the list opened, take while on air", () => {
    const add = { text: "a", does: "add-source", target: "cameras" };
    ok(!stepDone(add, state, ctx), "nothing new yet");
    ok(stepDone(add, { ...state, sources: [{ id: "cam" }, { id: "webcam" }] }, ctx), "a new one");
    const take = { text: "t", does: "take", target: "quad" };
    ok(!stepDone(take, state, ctx), "not on air");
    ok(stepDone(take, { ...state, scene: "quad" }, ctx), "the scene is on air");
  });

  test("install-plugin reads what the plan said was installed", () => {
    ok(stepDone({ text: "i", does: "install-plugin", target: "rtmp" }, state, ctx), "rtmp is here");
    ok(!stepDone({ text: "i", does: "install-plugin", target: "ndi" }, state, ctx), "ndi is not");
  });

  test("the checklist draws a button per step it can do and plain text for the rest", () => {
    const client = { state, call: async () => ({}) };
    const steps = [
      { text: "Add the YouTube stream key.", does: "add-key", target: "youtube" },
      { text: "Press the red button when the service starts." },
    ];
    const list = checklist(client, steps, newContext(state, {}), { install() {}, showPanel() {} });
    const rows = list.node.querySelectorAll("li");
    eq(rows.length, 2, "two rows");
    eq(rows[0].querySelector("button").textContent, "Add key");
    ok(!rows[1].querySelector("button"), "prose has no button");
    list.render({ ...state, outputs: [{ id: "youtube", has_key: true }] });
    ok(rows[0].classList.contains("done"), "ticked once keyed");
    eq(rows[0].querySelector("button").textContent, "Change");
  });

  test("the heading counts the steps in words", () => {
    eq(leftLine("Church service", 3), "Church service is set up. Three things left.");
    eq(leftLine("Classroom", 1), "Classroom is set up. One thing left.");
  });

  test("the notes drop what the install rows and the restart bar already say", () => {
    const plan = { sources: [{ id: "lyrics" }, { id: "cam", needs_plugin: "camera" }], outputs: [] };
    const result = {
      needs_restart: [
        "lyrics did not start: no wpesrc",
        "cam waits for the camera plugin, and starts once it is installed",
        "`canvas.width` was written to /x/godwinmix.toml and take effect on restart",
      ],
    };
    eq(notesWorthShowing(plan, result), ["lyrics did not start: no wpesrc"]);
  });

  await restartBarTests(test, eq, ok);
  await obsReportTests(test, eq, ok);
}

async function restartBarTests(test, eq, ok) {
  const bar = await import("../shell/restart-bar.js");
  test("the restart bar names the settings and cuts the core's reason to one sentence", () => {
    eq(bar.pendingLine(["Video bitrate"]), "1 setting takes effect when the mixer restarts: Video bitrate.");
    eq(bar.pendingLine(["a", "b", "c", "d", "e"]), "5 settings take effect when the mixer restarts: a, b, c and 2 more.");
    eq(bar.firstSentence("Started by hand, so it is running. Stop it with Ctrl+C."), "Started by hand, so it is running.");
  });

  const calls = [];
  const stub = (pending, possible) => ({
    call: async (method) => {
      calls.push(method);
      if (method === "config.get") return { needs_restart: pending };
      if (method === "core.info") return { restart: { possible, how: possible ? "supervised" : "none" } };
      if (method === "config.schema") return { properties: { "program.video_bitrate_kbps": { title: "Video bitrate" } } };
      if (method === "core.restart") return { restarting: false, message: "Nothing would start it again. Run this command." };
      return {};
    },
  });
  const node = () => document.querySelector(".restart-bar");

  await bar.checkRestart(stub(["program.video_bitrate_kbps"], true));
  test("a supervised mixer's bar offers Restart now, with the setting's title", () => {
    ok(node() && !node().hidden, "no bar");
    ok(node().textContent.includes("Video bitrate"), node().textContent);
    ok([...node().querySelectorAll("button")].some((b) => b.textContent === "Restart now"), "no Restart now");
  });

  await bar.checkRestart(stub(["program.video_bitrate_kbps"], false));
  test("a hand started mixer's bar says why in one sentence and offers no restart", () => {
    ok(!node().hidden, "the bar went away");
    ok(![...node().querySelectorAll("button")].some((b) => b.textContent === "Restart now"), "offered a restart it cannot do");
    const why = node().querySelector(".why").textContent;
    ok(why.startsWith("Nothing would start it again.") && !why.includes("command"), why);
  });

  node().querySelector('button[aria-label="Dismiss"]').click();
  await bar.checkRestart(stub(["program.video_bitrate_kbps"], false));
  test("a dismissed bar stays away for the same keys", () => {
    ok(node().hidden, "came back for the same keys");
  });
  await bar.checkRestart(stub(["canvas.width", "program.video_bitrate_kbps"], false));
  test("a new pending key brings the dismissed bar back", () => ok(!node().hidden, "stayed hidden"));
  await bar.checkRestart(stub([], false));
  test("the bar goes when nothing is pending", () => ok(node().hidden, "still up"));
  node().remove();
}

async function obsReportTests(test, eq, ok) {
  const { reportView, withoutFlags } = await import("../panels/welcome/obs-import.js");
  test("an importer line loses the command line flag the page cannot use", () => {
    eq(
      withoutFlags('the crop was measured against 1920x1080. Pass --source-size "NAME=WxH" to get it exact.'),
      "the crop was measured against 1920x1080."
    );
  });
  const rows = { installRow: (c, p) => Object.assign(document.createElement("p"), { className: "install", textContent: p.name }) };
  const view = reportView({}, {
    scenes: ["Main", "BRB"],
    items: 5,
    skipped: ["Game capture: not supported here"],
    sources_added: ["webcam"],
    sources_not_added: [{ id: "ndi-cam", reason: "needs the ndi plugin", plugin: "ndi" }],
    filters_duplicated: [{ filter: "Chroma", source: "webcam", obs_type: "chroma_key_filter_v2" }],
  }, rows);
  await wait(0);
  test("the OBS report says what came across, what did not and why, and offers the plugin", () => {
    const text = view.textContent;
    ok(text.includes("Imported 2 scenes with 5 items, and added 1 source."), text);
    ok(text.includes("Game capture: not supported here") && text.includes("ndi-cam: needs the ndi plugin"), text);
    eq([...view.querySelectorAll(".install")].map((n) => n.textContent), ["ndi"]);
    ok(!/gmx |\.toml|\.md|~\//.test(text), "a path or a command in the report");
  });
}
