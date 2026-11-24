// Mixer settings > Users: who can sign in with a username and password, who
// is signed in now, and the buttons to change either.
//
// Accounts are optional. With none, the card says what adding one changes
// and offers to add the first, which has to be an admin. With some, the page
// asks everybody to sign in, and tokens stay for agents and Companion.

import { el, clear } from "./dom.js";
import { userRow, sessionRow, roleSelect } from "./users-rows.js";
import { passwordField, newPasswordProblem, askNewPassword } from "./password.js";
import { signIn } from "../client/auth.js";
import { CODES } from "../client/errors.js";

const NONE_YET =
  "Nobody signs in with a username yet: this mixer uses its token. Add a user to give people a username and password instead. The first one has to be an admin, and from then on the page asks everybody to sign in. Agents and Companion keep using tokens.";
const SOME =
  "People sign in to this mixer's page with a username and password. Watch only sees the show; Run the show can take, cut and change sources; Admin can also change these settings and the users.";

/**
 * @param client  the page's client
 * @param {{base?: string, confirmRemove?: (name: string) => boolean, toast?: (text: string) => void}} opts
 */
export function usersCard(client, opts = {}) {
  const base = opts.base || location.origin;
  const confirmRemove = opts.confirmRemove || ((name) => window.confirm(`Remove ${name}? They are signed out everywhere and cannot sign in again.`));
  const tell = opts.toast || (() => {});
  const intro = el("p.dim", { style: { marginTop: "0" } });
  const list = el("div.col.users-list");
  const sessions = el("div.col.users-sessions");
  const status = el("p.sm", { role: "status" });
  const form = addForm(add);
  const signedIn = el("div.col", {}, [el("h4", { text: "Signed in now" }), sessions]);
  const root = el("details.users-card", { open: true }, [
    el("summary", { text: "Users and sign in" }),
    el("div.col", {}, [intro, list, form.el, signedIn, status]),
  ]);
  let account = { accounts: false };

  const act = {
    role: (name, role) => change("user.update", { name, role }, `${name} can now ${role === "view" ? "watch only" : role === "admin" ? "do everything" : "run the show"}.`),
    disable: (name, disabled) => change("user.update", { name, disabled }, disabled ? `${name} is disabled and signed out.` : `${name} can sign in again.`),
    remove: (name) => confirmRemove(name) && change("user.remove", { name }, `${name} is removed.`),
    signOutUser: (name) => change("user.sign_out", { name }, `${name} is signed out everywhere.`),
    signOutSession: (session) => change("user.sign_out", { session }, "That browser is signed out."),
    password: (name) => askNewPassword({
      title: `New password for ${name}`,
      text: `${name} is signed out everywhere and signs in again with this password. Give it to them yourself.`,
      save: (password) => client.call("user.reset_password", { name, password }).then(() => null, (e) => e.message || String(e)),
      done: () => refresh().then(() => say(`${name} has a new password.`)),
    }),
  };

  function say(text) {
    status.textContent = text || "";
  }

  async function change(method, params, done) {
    say("");
    try {
      await client.call(method, params);
      await refresh();
      say(done);
    } catch (e) {
      say(e.message || String(e));
      await refresh().catch(() => {});
    }
  }

  async function add(name, role, password) {
    const first = !account.accounts;
    await client.call("user.add", { name, role, password });
    // The first account closes a mixer that asked for nothing. This page
    // signs in as it, so the person adding it is not shut out of their own
    // page on the next reload.
    if (first && account.via === "open") await signIn(base, name, password);
    await refresh();
    say(first ? `Added ${name}. From now on the page asks everybody to sign in.` : `Added ${name}.`);
    if (first) tell("This mixer now asks people to sign in with a username and password.");
  }

  async function refresh() {
    let users, live;
    try {
      [account, users, live] = await Promise.all([
        client.call("account.get", {}),
        client.call("user.list", {}),
        client.call("user.sessions", {}),
      ]);
    } catch (e) {
      clear(list);
      intro.textContent = e && e.code === CODES.NO_SCOPE ? "Only an admin can see and change the users." : (e && e.message) || String(e);
      form.el.hidden = true;
      signedIn.hidden = true;
      return;
    }
    intro.textContent = account.accounts ? SOME : NONE_YET;
    form.first(!account.accounts);
    form.el.hidden = false;
    clear(list).append(...users.users.map((u) => userRow(u, account.user, act)));
    clear(sessions).append(...live.sessions.map((s) => sessionRow(s, act)));
    if (live.sessions.length > 1) {
      sessions.append(el("div.row", {}, [el("button.btn.sm", { text: "Sign out everybody", onclick: () => change("user.sign_out", {}, "Everybody is signed out.") })]));
    }
    signedIn.hidden = !live.sessions.length;
  }

  return { el: root, refresh, act };
}

/** The add a user row. `first(true)` fixes the role at Admin. */
function addForm(add) {
  const name = el("input", { type: "text", placeholder: "username", autocomplete: "off", autocapitalize: "none", spellcheck: "false", "aria-label": "Username" });
  const role = roleSelect("operate");
  const pass = passwordField("Password", "new-password");
  const problem = el("p.sm", { role: "alert", hidden: true, style: { color: "var(--bad)" } });
  const go = el("button.btn.primary", {
    text: "Add user",
    onclick: async () => {
      const why = !name.value.trim() ? "Type a username." : newPasswordProblem(pass.input.value);
      problem.hidden = !why;
      problem.textContent = why;
      if (why) return;
      go.disabled = true;
      try {
        await add(name.value.trim().toLowerCase(), role.value, pass.input.value);
        name.value = "";
        pass.input.value = "";
      } catch (e) {
        problem.hidden = false;
        problem.textContent = e.message || String(e);
      }
      go.disabled = false;
    },
  });
  const wrap = el("div.col.users-add", {}, [
    el("h4", { text: "Add a user" }),
    el("div.row", {}, [name, role]),
    pass.wrap,
    problem,
    el("div.row", {}, [go]),
  ]);
  return {
    el: wrap,
    first(yes) {
      role.disabled = yes;
      if (yes) role.value = "admin";
    },
  };
}
