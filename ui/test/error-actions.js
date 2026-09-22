// The error toast's buttons and the confirm round trip, against a fake
// transport. What is checked is what a person sees and what the page sends:
// the button the core named, the call that follows it, the refused call going
// again, the wait before "Try again", and no method name on screen.

import { Client, RpcError, CODES } from "../client/index.js";
import { Store } from "../client/store.js";
import { errorToast, errorText, alertButtons, clearToasts } from "../shell/toast.js";
import { pluginState } from "../client/kinds.js";

/** A transport that answers from a table of functions and records every call. */
function fakeTransport(answers) {
  const sent = [];
  return {
    sent,
    name: "fake",
    subscribe: () => Promise.resolve({}),
    call: async (method, params) => {
      sent.push([method, params]);
      const answer = answers[method];
      if (!answer) return {};
      return answer(params, sent.filter(([m]) => m === method).length);
    },
  };
}

const toasts = () => [...document.querySelectorAll(".toasts .toast")];
const button = (label) =>
  toasts().flatMap((t) => [...t.querySelectorAll("button")]).find((b) => b.textContent === label);

function until(predicate, what, ms = 3000) {
  const started = Date.now();
  return new Promise((resolve, reject) => {
    const tick = () => {
      if (predicate()) return resolve();
      if (Date.now() - started > ms) return reject(new Error(`timed out waiting for ${what}`));
      setTimeout(tick, 20);
    };
    tick();
  });
}

export async function errorActionTests(test, eq, ok) {
  clearToasts();

  // An exec source refused, allowed from the toast, and added in the same press.
  const allowed = { on: false };
  const t1 = fakeTransport({
    "source.add": () => {
      if (allowed.on) return { id: "cmd" };
      throw new RpcError(CODES.WRONG_STATE, "command sources are switched off.", {
        method: "source.add",
        retryable: true,
        action: { kind: "set-config", label: "Allow command sources", key: "security.allow_exec_sources", value: true, applies: "live" },
      });
    },
    "config.set": (p) => {
      allowed.on = p.values["security.allow_exec_sources"] === true;
      return { applied: ["security.allow_exec_sources"], needs_restart: [] };
    },
  });
  const c1 = new Client(t1, new Store());
  const refused = await c1.call("source.add", { id: "cmd", uri: "exec:sleep 30" }).catch((e) => e);
  errorToast(refused, "Add source");
  test("a refusal with an action gets its button, and the text names no method", () => {
    ok(button("Allow command sources"), "no button for the action");
    ok(!toasts()[0].textContent.includes("source.add"), toasts()[0].textContent);
    ok(toasts()[0].textContent.startsWith("Add source: command sources"), toasts()[0].textContent);
  });
  button("Allow command sources").click();
  await until(() => t1.sent.filter(([m]) => m === "source.add").length === 2, "the add to go again");
  test("pressing it sets the key and sends the refused call again", () => {
    eq(t1.sent[1], ["config.set", { values: { "security.allow_exec_sources": true } }]);
    eq(t1.sent[2], ["source.add", { id: "cmd", uri: "exec:sleep 30" }]);
  });
  clearToasts();

  // A restart key: saved, and the restart offered when the mixer can do one.
  const t2 = fakeTransport({
    "config.set": () => ({ applied: [], needs_restart: ["multiview.enabled"] }),
    "core.info": () => ({ restart: { possible: true, how: "supervised" } }),
    "core.restart": () => ({ restarting: true, message: "The mixer is restarting." }),
  });
  const c2 = new Client(t2, new Store());
  const off = new RpcError(CODES.NOT_FOUND, "the multiview is switched off.", {
    action: { kind: "set-config", label: "Turn the multiview on", key: "multiview.enabled", value: true, applies: "restart" },
  });
  Object.defineProperty(off, "client", { value: c2 });
  errorToast(off, "Multiview");
  button("Turn the multiview on").click();
  await until(() => button("Restart now"), "the restart offer");
  button("Restart now").click();
  await until(() => toasts().some((t) => t.textContent.includes("The mixer is restarting.")), "the restart to be said");
  test("a restart key says so and the restart button calls core.restart", () => {
    eq(t2.sent.map(([m]) => m), ["config.set", "core.info", "core.restart"]);
  });
  clearToasts();

  // Confirm required: asked once, sent again with the token, answered to the caller.
  const t3 = fakeTransport({
    "source.remove": (p) => {
      if (p.confirm === "cfm-1") return { removed: "cam1" };
      throw new RpcError(CODES.CONFIRM_REQUIRED, "source.remove is destructive and token 'desk' is set to confirm = required.", { confirm_token: "cfm-1", expires_in_ms: 30000 });
    },
  });
  const c3 = new Client(t3, new Store());
  const asked = [];
  c3.confirm = async (ask) => (asked.push(ask.method), true);
  const answer = await c3.call("source.remove", { id: "cam1" });
  test("a confirm required call is asked about and goes again with the token", () => {
    eq(asked, ["source.remove"]);
    eq(answer, { removed: "cam1" });
    eq(t3.sent[1], ["source.remove", { id: "cam1", confirm: "cfm-1" }]);
  });
  c3.confirm = async () => false;
  const declined = await c3.call("source.remove", { id: "cam1" }).catch((e) => e);
  errorToast(declined, "Remove");
  test("a no to the confirmation is not an error toast", () => {
    ok(declined.declined, "not marked as declined");
    eq(toasts().length, 0);
  });

  // A hold with a wait: "Try again" is disabled until it has passed.
  const t4 = fakeTransport({
    "program.take": (p, n) => {
      if (n > 1) return { program: p.source };
      throw new RpcError(CODES.SAFETY, "wait 0.3 s and take again.", { retry_after_ms: 300, method: "program.take" });
    },
  });
  const c4 = new Client(t4, new Store());
  errorToast(await c4.call("program.take", { source: "cam2" }).catch((e) => e), "Take");
  test("a refusal with a wait offers Try again, disabled for the wait", () => {
    ok(button("Try again"), "no Try again");
    ok(button("Try again").disabled, "enabled before the wait");
  });
  await until(() => button("Try again") && !button("Try again").disabled, "the wait to pass");
  button("Try again").click();
  await until(() => t4.sent.length === 2, "the take to go again");
  test("Try again sends the same call", () => eq(t4.sent[1], ["program.take", { source: "cam2" }]));
  clearToasts();

  test("a missing scope is said without the method name", () => {
    const e = new RpcError(CODES.NO_SCOPE, "config.set needs the 'admin' scope and this token holds read.", { method: "config.set", needed: "admin" });
    const text = errorText(e, "Settings");
    ok(!text.includes("config.set"), text);
    ok(text.includes("admin"), text);
  });

  test("an alert that says restart the mixer carries the restart button", () => {
    const buttons = alertButtons(c2, { severity: "error", message: "restart the mixer when you can.", action: { kind: "restart", label: "Restart the mixer" } });
    eq(buttons.map((b) => b.label), ["Restart the mixer"]);
    eq(alertButtons(c2, { severity: "error", message: "x", action: { kind: "launch-rockets", label: "Go" } }), []);
  });

  // A want made before the socket is open used to subscribe at once, be
  // refused with "not connected to the mixer yet", and log a warning; the
  // open then subscribed anyway. Now nothing is sent until the open.
  const asks = [];
  const early = new Client({ name: "fake", subscribe: (spec) => (asks.push(spec), Promise.resolve({})) }, new Store());
  const held = early.want("multiview", { fps: 8, width: 640 });
  await new Promise((r) => setTimeout(r, 80));
  test("nothing subscribes before the socket is open", () => eq(asks.length, 0));
  early.store.patch({ connected: true });
  await early.resubscribe();
  test("the open subscribes with what was wanted before it", () => {
    eq(asks.length, 1);
    eq(asks[0].ext, { multiview: { fps: 8, width: 640 } });
  });
  held.release();

  test("a plugin that is installed and switched off is not offered for install", () => {
    const plugins = [{ name: "camera", enabled: false }, { name: "screen", problem: "no manifest" }, { name: "audio-device" }];
    eq(pluginState(plugins, "camera"), "disabled");
    eq(pluginState(plugins, "screen"), "problem");
    eq(pluginState(plugins, "audio-device"), "ready");
    eq(pluginState(plugins, "ndi"), "absent");
  });
}
