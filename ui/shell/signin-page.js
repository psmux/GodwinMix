// The sign in page, for a mixer that has accounts: a username and a password
// in place of the token box. Loaded only by a page the mixer refused.
//
// A real form, so a browser's password manager offers to save and fill it.
// Tokens are still there for whoever needs one, behind a quiet link.

import { el, on } from "./dom.js";
import { modal } from "./modal.js";
import { signIn } from "../client/auth.js";

/**
 * The form on its own, for the page and for the tests.
 *
 * @param {{submit: (user: string, password: string) => Promise<{ok: boolean, message?: string}>,
 *          useToken: () => void, reason?: string}} opts
 */
export function signInForm(opts) {
  const user = el("input", { type: "text", name: "username", autocomplete: "username", autocapitalize: "none", spellcheck: "false", required: true });
  const pass = el("input", { type: "password", name: "password", autocomplete: "current-password", required: true });
  const problem = el("p.sm.signin-problem", { role: "alert", hidden: true, style: { color: "var(--bad)" } });
  const go = el("button.btn.primary", { type: "submit", text: "Sign in" });
  const form = el("form.form.signin-form", { method: "post", action: "#", novalidate: true }, [
    el("p.dim", { style: { marginTop: "0" }, text: opts.reason || "This mixer asks who you are. Sign in with the username and password an admin gave you." }),
    el("div.field", {}, [el("label", {}, [el("span.lbl", { text: "Username" }), user])]),
    el("div.field", {}, [el("label", {}, [el("span.lbl", { text: "Password" }), pass])]),
    problem,
    el("div.row", {}, [go]),
    el("p.sm.dim", { text: "Forgot your password? An admin of this mixer can set a new one in Mixer settings > Users." }),
    el("details.sm.signin-advanced", {}, [
      el("summary.dim", { text: "Advanced" }),
      el("p.sm.dim", { text: "Agents, Bitfocus Companion and scripts sign in with a token instead. If you have one for a browser, use it here." }),
      el("button.btn.sm", { type: "button", text: "Use a token instead", onclick: () => opts.useToken() }),
    ]),
  ]);
  on(form, "submit", async (e) => {
    e.preventDefault();
    const name = user.value.trim();
    if (!name || !pass.value) {
      say(!name ? "Type your username." : "Type your password.");
      (name ? pass : user).focus();
      return;
    }
    go.disabled = true;
    go.textContent = "Signing in…";
    const answer = await opts.submit(name, pass.value);
    go.disabled = false;
    go.textContent = "Sign in";
    if (answer.ok) return;
    pass.value = "";
    say(answer.message);
    pass.focus();
  });
  function say(text) {
    problem.textContent = text;
    problem.hidden = !text;
  }
  return { form, user, pass, problem };
}

let asking = null;

/**
 * Ask for a username and password until the mixer takes them.
 * @returns {Promise<"session"|"token"|null>} how the page is now signed in
 */
export function askToSignIn(base, reason) {
  if (asking) return asking;
  asking = new Promise((resolve) => {
    let answer = null;
    const parts = signInForm({
      reason,
      submit: async (user, password) => {
        const got = await signIn(base, user, password);
        if (got.ok) {
          answer = "session";
          m.close();
        }
        return got;
      },
      useToken: () => {
        answer = "token";
        m.close();
      },
    });
    const m = modal({
      title: "Sign in to GodwinMix",
      body: parts.form,
      onClose: () => {
        asking = null;
        resolve(answer);
      },
    });
    parts.user.focus();
  });
  return asking;
}
