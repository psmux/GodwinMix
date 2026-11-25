// Usernames and passwords: the sign in page, Help > Address and sign in on a
// mixer with accounts, the Users card in Mixer settings against a stub
// client, and the socket that is told it was signed out.

import { signInForm } from "../shell/signin-page.js";
import { accountParts } from "../shell/signin.js";
import { usersCard } from "../shell/users-card.js";
import { newPasswordProblem } from "../shell/password.js";
import { shortDevice } from "../shell/users-rows.js";
import { RpcSocket } from "../client/rpc.js";
import { RpcError } from "../client/errors.js";
import { el } from "../shell/dom.js";

const tick = () => new Promise((r) => setTimeout(r, 0));
const button = (root, label) => [...root.querySelectorAll("button")].find((b) => b.textContent === label);

/** The runner's test() is synchronous; this awaits first and reports through it. */
async function later(test, name, fn) {
  let failure = null;
  try {
    await fn();
  } catch (e) {
    failure = e;
  }
  test(name, () => {
    if (failure) throw failure;
  });
}

/** A client whose answers are a table, recording every call. */
function stubClient(state) {
  const calls = [];
  return {
    calls,
    call: async (method, params) => {
      calls.push([method, params]);
      if (state.refuse) throw new RpcError(-32002, "user.list needs admin", { needed: "admin" });
      if (method === "account.get") return state.account;
      if (method === "user.list") return { users: state.users };
      if (method === "user.sessions") return { sessions: state.sessions };
      if (method === "user.add") {
        state.users.push({ name: params.name, role: params.role, disabled: false, sessions: 0, created: "2026-11-24T10:00:00Z" });
        state.account = { ...state.account, accounts: true };
        return { user: state.users.at(-1), sessions_ended: 0 };
      }
      return { sessions_ended: 1 };
    },
  };
}

export async function usersTests(test, eq, ok) {
  test("the sign in page asks for a username and password, with the token only under Advanced", () => {
    const { form, user, pass } = signInForm({ submit: async () => ({ ok: true }), useToken: () => {} });
    eq(user.getAttribute("autocomplete"), "username");
    eq(pass.getAttribute("autocomplete"), "current-password");
    eq(pass.type, "password");
    const outside = [...form.childNodes].filter((n) => !n.matches || !n.matches("details")).map((n) => n.textContent).join(" ");
    ok(!/token/i.test(outside), "no token outside Advanced: " + outside);
    ok(form.querySelector("details").textContent.includes("Use a token instead"), "a way to a token for those who need one");
  });

  await later(test, "a refused sign in says why, clears the password and keeps the username", async () => {
    const sent = [];
    const parts = signInForm({
      submit: async (u, p) => {
        sent.push([u, p]);
        return { ok: false, message: "That username and password do not match an account on this mixer." };
      },
      useToken: () => {},
    });
    document.body.append(parts.form);
    parts.form.dispatchEvent(new Event("submit", { cancelable: true }));
    await tick();
    ok(!parts.problem.hidden && parts.problem.textContent === "Type your username.", parts.problem.textContent);
    eq(sent.length, 0, "nothing sent without a username");
    parts.user.value = " Sam ";
    parts.pass.value = "wrong-password";
    parts.form.dispatchEvent(new Event("submit", { cancelable: true }));
    await tick();
    await tick();
    eq(JSON.stringify(sent), '[["Sam","wrong-password"]]', "the name is trimmed");
    ok(parts.problem.textContent.includes("do not match"), parts.problem.textContent);
    eq(parts.pass.value, "", "the password is cleared");
    eq(parts.user.value, " Sam ", "the username stays");
    parts.form.remove();
  });

  test("Use a token instead goes to the token box", () => {
    let asked = 0;
    const { form } = signInForm({ submit: async () => ({ ok: true }), useToken: () => (asked += 1) });
    button(form, "Use a token instead").click();
    eq(asked, 1);
  });

  test("Address and sign in names the account and offers Sign out and Change password", () => {
    const done = [];
    const actions = { signOut: () => done.push("out"), changePassword: () => done.push("password") };
    const root = el("div", {}, accountParts("http://192.0.2.1:8080", "desk-token-1234", { accounts: true, via: "session", user: "sam", role: "operate" }, actions));
    ok(root.textContent.includes("Signed in as sam, who can run the show."), root.textContent);
    button(root, "Sign out").click();
    button(root, "Change password").click();
    eq(done.join(","), "out,password");
    const details = root.querySelector("details");
    ok(details && !details.open && details.textContent.includes("Advanced"), "the token is under Advanced, closed");
    ok(!root.textContent.replace(details.textContent, "").includes("desk-token-1234"), "and nowhere else");
    const bare = el("div", {}, accountParts("http://192.0.2.1:8080", null, { accounts: true, via: "session", user: "sam", role: "view" }, actions));
    ok(!bare.querySelector("details"), "no token, no Advanced");
  });

  test("a new password needs eight characters, no edge spaces, and the same twice", () => {
    ok(newPasswordProblem("short").includes("8"));
    ok(newPasswordProblem(" padded-pass").includes("space"));
    ok(newPasswordProblem("long-enough", "different").includes("not the same"));
    eq(newPasswordProblem("long-enough", "long-enough"), "");
  });

  test("a browser's description reads as a person would say it", () => {
    eq(shortDevice("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Version/18.0 Mobile/15E148 Safari/604.1"), "Safari on iPhone or iPad");
    eq(shortDevice("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/142.0 Safari/537.36"), "Chrome on Mac");
    eq(shortDevice(""), "");
  });

  await later(test, "with no accounts the Users card explains, and the first user is an admin", async () => {
    const state = { account: { accounts: false, via: "token" }, users: [], sessions: [] };
    const client = stubClient(state);
    const card = usersCard(client, { base: "http://192.0.2.1:8080" });
    await card.refresh();
    ok(card.el.textContent.includes("Nobody signs in with a username yet"), card.el.textContent);
    const role = card.el.querySelector(".users-add select");
    ok(role.disabled && role.value === "admin", "the first account's role is fixed at admin");
    card.el.querySelector(".users-add input[type=text]").value = "owner";
    card.el.querySelector(".users-add input[type=password]").value = "a-long-password";
    button(card.el, "Add user").click();
    await tick();
    await tick();
    await tick();
    const add = client.calls.find(([m]) => m === "user.add");
    eq(JSON.stringify(add[1]), '{"name":"owner","role":"admin","password":"a-long-password"}');
    ok(card.el.querySelector('[data-user="owner"]'), "the new user is listed");
    ok(card.el.textContent.includes("asks everybody to sign in"), card.el.textContent);
  });

  await later(test, "the Users card lists people and sessions, and its buttons call the right methods", async () => {
    const state = {
      account: { accounts: true, via: "session", user: "boss", role: "admin" },
      users: [
        { name: "boss", role: "admin", disabled: false, sessions: 1, created: "2026-11-24T10:00:00Z" },
        { name: "cam-op", role: "operate", disabled: false, sessions: 2, created: "2026-11-24T10:00:00Z" },
      ],
      sessions: [
        { id: "s-aaa", user: "boss", created: "2026-11-24T10:00:00Z", expires: "2026-12-24T10:00:00Z", address: "192.0.2.10", device: "Chrome/142 Macintosh Mac OS X", current: true },
        { id: "s-bbb", user: "cam-op", created: "2026-11-24T11:00:00Z", expires: "2026-12-24T11:00:00Z", address: "192.0.2.11", device: "iPhone Safari/604", current: false },
      ],
    };
    const client = stubClient(state);
    const card = usersCard(client, { confirmRemove: () => true });
    await card.refresh();
    const me = card.el.querySelector('[data-user="boss"]');
    ok(me.textContent.includes("boss (you)") && !button(me, "Remove") && !button(me, "Disable"), "you cannot remove or disable yourself here");
    const op = card.el.querySelector('[data-user="cam-op"]');
    ok(op.textContent.includes("2 browsers signed in"), op.textContent);
    ok(card.el.querySelector('[data-session="s-aaa"]').textContent.includes("this browser"));
    const reads = ["account.get", "user.list", "user.sessions"];
    const last = () => client.calls.filter(([m]) => !reads.includes(m)).at(-1);
    const sent = async (b) => {
      b.click();
      await tick();
      await tick();
      return last();
    };
    eq(JSON.stringify(await sent(button(op, "Disable"))), '["user.update",{"name":"cam-op","disabled":true}]');
    eq(JSON.stringify(await sent(button(op, "Sign out everywhere"))), '["user.sign_out",{"name":"cam-op"}]');
    eq(JSON.stringify(await sent(button(card.el.querySelector('[data-session="s-bbb"]'), "Sign out"))), '["user.sign_out",{"session":"s-bbb"}]');
    eq(JSON.stringify(await sent(button(card.el.querySelector('[data-user="cam-op"]'), "Remove"))), '["user.remove",{"name":"cam-op"}]');
    const select = card.el.querySelector('[data-user="cam-op"] select');
    select.value = "view";
    select.dispatchEvent(new Event("change"));
    await tick();
    await tick();
    eq(JSON.stringify(last()), '["user.update",{"name":"cam-op","role":"view"}]');
    ok(!card.el.textContent.match(/\$argon2|password":/), "no hash anywhere");
  });

  await later(test, "somebody who is not an admin is told the card is an admin's", async () => {
    const card = usersCard(stubClient({ refuse: true }));
    await card.refresh();
    ok(card.el.textContent.includes("Only an admin can see and change the users."), card.el.textContent);
    ok(card.el.querySelector(".users-add").hidden, "no add form");
  });

  test("a socket closed with 4401 stops retrying and says the page was signed out", () => {
    const Real = globalThis.WebSocket;
    const made = [];
    globalThis.WebSocket = class {
      constructor(url) {
        this.url = url;
        made.push(this);
      }
      close() {}
    };
    try {
      let out = 0;
      const socket = new RpcSocket("ws://192.0.2.1:8080/rpc", { onSignedOut: () => (out += 1) });
      socket.open();
      made[0].onclose({ code: 4401, reason: "signed out", wasClean: true });
      eq(out, 1, "told once");
      ok(socket.closed, "and it will not retry");
    } finally {
      globalThis.WebSocket = Real;
    }
  });
}
