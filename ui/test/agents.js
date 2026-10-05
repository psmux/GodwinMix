// Help > Connect an AI agent, against a fake client: the installed tools come
// first, Set up shows the files before writing anything, and the second press
// writes them and says how to start the tool.

import { openAgents, ordered, setups, STARTERS } from "../shell/agents.js";

function fakeClient() {
  const sent = [];
  const plan = (params) => ({
    tool: params.tool, name: "opencode", scope: "user", applied: !params.dry_run,
    would_change: true, dry_run: params.dry_run || undefined,
    writes: [{ path: "/home/ana/.config/opencode/opencode.json", action: "merge", what: "the godwinmix MCP server", backup: params.dry_run ? undefined : "/home/ana/.config/opencode/opencode.json.before-godwinmix" }],
    start: "In a terminal, run `opencode`, then ask it something.",
    prompt: "Make a lower third for Ana Silva, Producer, in blue, and put it on air.",
  });
  return {
    sent,
    call: async (method, params) => {
      sent.push([method, params]);
      if (method === "core.info") return { executable: "/opt/godwinmix/godwinmix" };
      if (method === "agent.tools") return [
        { tool: "opencode", name: "opencode", installed: true },
        { tool: "pi", name: "pi", installed: true },
        { tool: "claude", name: "Claude Code", installed: false },
      ];
      if (method === "agent.setup") return plan(params);
      return {};
    },
  };
}

const until = (predicate, what, ms = 2000) => new Promise((resolve, reject) => {
  const started = Date.now();
  const tick = () => {
    if (predicate()) return resolve();
    if (Date.now() - started > ms) return reject(new Error(`timed out waiting for ${what}`));
    setTimeout(tick, 20);
  };
  tick();
});

const buttons = (root) => [...root.querySelectorAll("button")];
const press = (root, label) => buttons(root).find((b) => b.textContent === label).click();

export async function agentsTests(test, eq, ok) {
  test("installed tools come first, and every tool is still offered", () => {
    const all = ordered(setups("g"), [{ tool: "pi", installed: true }, { tool: "claude", installed: false }]);
    eq(all[0].id, "pi");
    ok(all[0].installed, "pi is marked installed");
    eq(all.length, setups("g").length);
    ok(STARTERS.some((s) => s.includes("studio set")), "a virtual set is one of the starters");
  });

  const client = fakeClient();
  const dialog = await openAgents(client);
  const root = dialog.el;
  await test("the first tab is the first installed tool", () => {
    ok(buttons(root).some((b) => b.textContent === "opencode (installed)"), "opencode is marked installed");
  });
  press(root, "Set up opencode");
  await until(() => root.textContent.includes("Write these files"), "the dry run");
  await test("Set up shows the files and writes nothing", () => {
    const setups = client.sent.filter(([m]) => m === "agent.setup");
    eq(setups.length, 1);
    eq(setups[0][1].dry_run, true);
    ok(root.textContent.includes("opencode.json"), "the file is named");
  });
  press(root, "Write these files");
  await until(() => root.textContent.includes("is set up"), "the write");
  await test("the second press writes and says how to start it", () => {
    const last = client.sent.filter(([m]) => m === "agent.setup").pop();
    ok(!last[1].dry_run, "the write is not a dry run");
    ok(root.textContent.includes("run `opencode`"), "how to start it");
    ok(root.textContent.includes("before-godwinmix"), "where the old file went");
  });
  dialog.close();
}
