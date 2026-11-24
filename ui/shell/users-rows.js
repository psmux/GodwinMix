// The rows of the Users card: one per account, one per signed in browser.
// Drawing only; what a button does is the card's.

import { el } from "./dom.js";

export const ROLES = [
  ["view", "Watch only"],
  ["operate", "Run the show"],
  ["admin", "Admin"],
];

/** A role picker, with `role` chosen. */
export function roleSelect(role, onchange) {
  const select = el("select", { "aria-label": "Role", onchange: onchange ? () => onchange(select.value) : null },
    ROLES.map(([value, label]) => el("option", { value, text: label, selected: value === role })));
  return select;
}

/**
 * One account. `me` is the name this page is signed in as, so the person
 * cannot remove or disable the account they are using by accident.
 */
export function userRow(user, me, act) {
  const mine = user.name === me;
  const sessions = user.sessions === 1 ? "1 browser signed in" : `${user.sessions} browsers signed in`;
  const state = user.disabled ? "Disabled" : sessions;
  return el("div.row.users-row", { "data-user": user.name }, [
    el("div.col.grow", {}, [
      el("strong", { text: mine ? `${user.name} (you)` : user.name }),
      el("span.sm.dim", { text: state }),
    ]),
    roleSelect(user.role, (role) => act.role(user.name, role)),
    el("button.btn.sm", { text: "New password", onclick: () => act.password(user.name) }),
    user.sessions ? el("button.btn.sm", { text: "Sign out everywhere", onclick: () => act.signOutUser(user.name) }) : null,
    mine ? null : el("button.btn.sm", { text: user.disabled ? "Enable" : "Disable", onclick: () => act.disable(user.name, !user.disabled) }),
    mine ? null : el("button.btn.sm", { text: "Remove", onclick: () => act.remove(user.name) }),
  ]);
}

/** One signed in browser or phone. */
export function sessionRow(session, act) {
  const seen = session.last_seen ? `, last seen ${when(session.last_seen)}` : "";
  const where = [session.address, shortDevice(session.device)].filter(Boolean).join(", ");
  return el("div.row.users-session", { "data-session": session.id }, [
    el("div.col.grow", {}, [
      el("span", { text: session.current ? `${session.user} (this browser)` : session.user }),
      el("span.sm.dim", { text: `${where || "unknown device"}. Signed in ${when(session.created)}${seen}.` }),
    ]),
    el("button.btn.sm", { text: "Sign out", onclick: () => act.signOutSession(session.id) }),
  ]);
}

/** "Chrome on Mac" rather than a whole user agent string. */
export function shortDevice(agent) {
  if (!agent) return "";
  const browser = /Edg\//.test(agent) ? "Edge" : /Firefox\//.test(agent) ? "Firefox" : /Chrome\//.test(agent) ? "Chrome" : /Safari\//.test(agent) ? "Safari" : /GodwinMix/.test(agent) ? "GodwinMix" : "";
  const system = /iPhone|iPad/.test(agent) ? "iPhone or iPad" : /Android/.test(agent) ? "Android" : /Mac OS X/.test(agent) ? "Mac" : /Windows/.test(agent) ? "Windows" : /Linux/.test(agent) ? "Linux" : "";
  if (browser && system) return `${browser} on ${system}`;
  return browser || system || agent.slice(0, 40);
}

function when(text) {
  const t = Date.parse(text);
  if (Number.isNaN(t)) return text;
  return new Date(t).toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
}
