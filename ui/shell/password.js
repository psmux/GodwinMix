// Password dialogs: a person changing their own, and an admin setting a new
// one for somebody who forgot theirs. Loaded on first use.

import { el, on } from "./dom.js";
import { modal } from "./modal.js";
import { toast } from "./toast.js";

const MIN = 8;

/** A password box with a Show toggle. */
export function passwordField(label, autocomplete) {
  const input = el("input", { type: "password", autocomplete, spellcheck: "false" });
  const show = el("button.btn.sm", {
    type: "button",
    text: "Show",
    onclick: () => {
      const hidden = input.type === "password";
      input.type = hidden ? "text" : "password";
      show.textContent = hidden ? "Hide" : "Show";
    },
  });
  const wrap = el("div.field", {}, [el("label", {}, [el("span.lbl", { text: label }), el("div.row", {}, [input, show])])]);
  return { wrap, input };
}

/** Why a new password would be refused, before asking the mixer. */
export function newPasswordProblem(password, again) {
  if (password.length < MIN) return `Use at least ${MIN} characters. A few words together are easy to remember.`;
  if (password.trim() !== password) return "Take out the space at the start or the end.";
  if (again !== undefined && again !== password) return "The two passwords are not the same.";
  return "";
}

/**
 * Ask for a new password, twice, and hand it to `save`, which returns null
 * on success or the words to show.
 */
export function askNewPassword({ title, text, save, done }) {
  const one = passwordField("New password", "new-password");
  const two = passwordField("The same again", "new-password");
  const problem = el("p.sm", { role: "alert", hidden: true, style: { color: "var(--bad)" } });
  const say = (t) => {
    problem.textContent = t;
    problem.hidden = !t;
  };
  const go = el("button.btn.primary", {
    text: "Save",
    onclick: async () => {
      const why = newPasswordProblem(one.input.value, two.input.value);
      if (why) return say(why);
      go.disabled = true;
      const refused = await save(one.input.value);
      go.disabled = false;
      if (refused) return say(refused);
      m.close();
      if (done) done();
    },
  });
  on(two.input, "keydown", (e) => {
    if (e.key === "Enter") go.click();
  });
  const m = modal({
    title,
    body: el("div.form", {}, [el("p.dim", { style: { marginTop: "0" }, text }), one.wrap, two.wrap, problem]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), go],
  });
  one.input.focus();
  return m;
}

/** Help > Address and sign in > Change password. */
export function openChangePassword(client) {
  const current = passwordField("Current password", "current-password");
  const m = askNewPassword({
    title: "Change your password",
    text: "Every other browser or phone signed in as you is signed out. This one stays signed in.",
    save: async (password) => {
      try {
        await client.call("account.change_password", { current: current.input.value, password });
        return null;
      } catch (e) {
        return e.message || String(e);
      }
    },
    done: () => toast({ text: "Your password is changed." }),
  });
  m.body.querySelector(".form").insertBefore(current.wrap, m.body.querySelector(".form .field"));
  current.input.focus();
  return m;
}
