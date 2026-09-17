// Ctrl+K: every command, searchable.
//
// Ranking is deliberately dull: a title that starts with what was typed beats
// one that contains it, which beats a subsequence match. Nothing is fuzzy
// enough to surprise, which matters when one of the entries takes a source to
// air.

import { el, clear, on } from "./dom.js";
import { all } from "./commands.js";
import { modal, confirmModal } from "./modal.js";
import { errorToast } from "./toast.js";

export function openPalette() {
  const input = el("input", { type: "text", placeholder: "Type a command", "aria-label": "Command", autocomplete: "off" });
  const list = el("ul", { role: "listbox" });
  const box = el("div.box", {}, [input, list]);
  const scrim = el("div.palette", {
    onpointerdown: (e) => {
      if (e.target === scrim) close();
    },
  }, [box]);
  (document.querySelector(".slot-modal") || document.body).appendChild(scrim);

  let items = [];
  let cursor = 0;

  function close() {
    off();
    scrim.remove();
  }

  function draw() {
    items = rank(all(), input.value);
    cursor = Math.min(cursor, Math.max(0, items.length - 1));
    clear(list);
    if (!items.length) {
      list.appendChild(el("li.dim", { text: "No command matches." }));
      return;
    }
    items.forEach((cmd, i) => {
      const disabled = cmd.enabled && !cmd.enabled();
      list.appendChild(
        el("li" + (i === cursor ? ".on" : ""), {
          role: "option",
          "aria-selected": i === cursor ? "true" : "false",
          style: disabled ? { opacity: "0.45" } : null,
          onpointerdown: (e) => {
            e.preventDefault();
            cursor = i;
            go();
          },
        }, [
          el("span.grow", { text: cmd.title }),
          cmd.key ? el("span.id", { text: cmd.key }) : null,
          el("span.id", { text: cmd.method || cmd.group }),
        ])
      );
    });
  }

  async function go() {
    const cmd = items[cursor];
    if (!cmd) return;
    if (cmd.enabled && !cmd.enabled()) return;
    close();
    try {
      await cmd.run();
    } catch (e) {
      errorToast(e, cmd.title);
    }
  }

  const off = on(window, "keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      cursor = Math.min(cursor + 1, items.length - 1);
      draw();
      scrollTo();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      cursor = Math.max(cursor - 1, 0);
      draw();
      scrollTo();
    } else if (e.key === "Enter") {
      e.preventDefault();
      go();
    }
  }, true);

  function scrollTo() {
    const node = list.children[cursor];
    if (node && node.scrollIntoView) node.scrollIntoView({ block: "nearest" });
  }

  on(input, "input", () => {
    cursor = 0;
    draw();
  });
  draw();
  input.focus();
  return { close };
}

/** Exported so the tests can check the ordering without opening a window. */
export function rank(cmds, query) {
  const q = String(query || "").trim().toLowerCase();
  if (!q) return cmds.slice().sort(byGroupThenTitle);
  const scored = [];
  for (const cmd of cmds) {
    const hay = (cmd.title + " " + (cmd.method || "") + " " + cmd.group).toLowerCase();
    const title = cmd.title.toLowerCase();
    let score = -1;
    if (title.startsWith(q)) score = 0;
    else if (hay.startsWith(q)) score = 1;
    else if (title.includes(q)) score = 2;
    else if (hay.includes(q)) score = 3;
    else if (subsequence(hay, q)) score = 4;
    if (score >= 0) scored.push([score, cmd]);
  }
  scored.sort((a, b) => a[0] - b[0] || byGroupThenTitle(a[1], b[1]));
  return scored.map(([, cmd]) => cmd);
}

function byGroupThenTitle(a, b) {
  return a.group.localeCompare(b.group) || a.title.localeCompare(b.title);
}

function subsequence(hay, needle) {
  let i = 0;
  for (const ch of hay) {
    if (ch === needle[i]) i += 1;
    if (i === needle.length) return true;
  }
  return false;
}

/**
 * A form for one protocol method, from its params schema. This is what makes a
 * method with no button still usable, and it is how a plugin's new command is
 * reachable the day it ships.
 */
export function methodForm(client, method, SchemaForm) {
  const form = new SchemaForm(paramsSchema(method), {});
  const out = el("pre.sm", { style: { whiteSpace: "pre-wrap", margin: "var(--gap) 0 0", maxHeight: "40vh", overflow: "auto" } });
  const send = el("button.btn.primary", { text: "Call" });
  const m = modal({
    title: method.name,
    body: el("div", {}, [method.summary ? el("p.dim.sm", { text: method.summary, style: { marginTop: 0 } }) : null, form.el, out]),
    footer: [el("button.btn", { text: "Close", onclick: () => m.close() }), send],
  });
  send.onclick = async () => {
    // The protocol marks its own dangerous methods. core.shutdown is one of
    // them, it takes no parameters, and without this it was one click from
    // the palette: the programme off air with nothing asked.
    if (method.destructive) {
      const ok = await confirmModal(
        `${method.name} is destructive. ${method.summary || ""}`.trim(),
        "Call it"
      );
      if (!ok) return;
    }
    send.disabled = true;
    try {
      const result = await client.call(method.name, form.read());
      out.textContent = JSON.stringify(result, null, 2);
    } catch (e) {
      out.textContent = `${e.code}  ${e.message}`;
    }
    send.disabled = false;
  };
  return m;
}

/**
 * The params schema for one method, ready for `SchemaForm`.
 *
 * Most methods declare their params as a bare `$ref` into the document's
 * `$defs`, and `SchemaForm` only walks `properties`, so it drew an empty form
 * and sent `{}` for every one of them. `addProtocolCommands` hangs the
 * document's `$defs` on each method so the reference can be followed here,
 * and leaves it for the nested ones to resolve against too.
 */
export function paramsSchema(method) {
  const params = method.params || { type: "object", properties: {} };
  const defs = method.$defs || params.$defs || {};
  let schema = params;
  if (typeof params.$ref === "string" && params.$ref.startsWith("#/$defs/")) {
    schema = defs[params.$ref.slice("#/$defs/".length)] || { type: "object", properties: {} };
  }
  return Object.assign({}, schema, { $defs: defs });
}
