// The transcode profiles: the built in ones, which are read only, and the
// ones saved on this station, which can be changed, copied and removed.
// A change to a saved profile changes every channel that takes it. A
// removal the station refuses because channels take the profile asks
// which profile they move to, and removes it with move_to.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast } from "../../shell/toast.js";
import { sheet } from "./sheet.js";
import { profileWords, readSetup, slug } from "./setup.js";
import { choice, field, said, text, whyBox } from "./form.js";
import { STARTER, customForm } from "./conv.js";

export async function manageProfiles(client, opts = {}) {
  sheet("headend");
  sheet("setup");
  let setup = opts.setup || (await readSetup(client));
  const list = el("div.hs-profiles");
  const pane = el("div.hs-ppane", { hidden: true });
  const why = whyBox();
  const m = modal({ title: "Transcode profiles", wide: true, body: el("div.hs-pman", {}, [el("p.hs-note.dim", { text: "A profile is one conversion that many channels take. Change a saved one and every channel that takes it follows." }), why.node, list, pane]), footer: [el("span.grow"), el("button.btn", { type: "button", "data-p": "new", text: "New profile" }), el("button.btn", { type: "button", text: "Close", onclick: () => m.close() })] });
  m.el.classList.add("hs-dialog");

  const users = (id) => (setup.channels || []).filter((e) => e.channel.profile === id).map((e) => e.channel.name);

  function draw() {
    const card = (p) => el(`div.hs-pcard${p.builtin ? ".builtin" : ""}`, { "data-id": p.id }, [
      el("div.hs-ptop", {}, [el("strong", { text: p.name || p.id }), el("span.hs-flag", { text: p.builtin ? "Built in" : "Saved" }), el("code.dim", { text: p.id })]),
      el("p.hs-note", { text: profileWords(p) }),
      el("p.hs-note.dim", { text: users(p.id).length ? `Taken by ${users(p.id).length}: ${users(p.id).slice(0, 6).join(", ")}${users(p.id).length > 6 ? " and more" : ""}` : "No channel takes it." }),
      el("div.hs-row", {}, [
        p.builtin ? null : el("button.btn", { type: "button", "data-p": "edit", text: "Edit" }),
        el("button.btn", { type: "button", "data-p": "copy", text: "Duplicate" }),
        p.builtin ? null : el("button.btn.he-danger", { type: "button", "data-p": "remove", text: "Remove" }),
      ]),
    ]);
    const built = setup.profiles.filter((p) => p.builtin);
    const saved = setup.profiles.filter((p) => !p.builtin);
    list.replaceChildren(
      el("h4.hs-sub", { text: "Saved on this station" }),
      saved.length ? el("div.hs-pgrid", {}, saved.map(card)) : el("p.hs-note.dim", { text: "None yet. New profile makes one, or Duplicate one of the built in ones below." }),
      el("h4.hs-sub", { text: "Built in" }),
      el("div.hs-pgrid", {}, built.map(card)),
    );
  }

  async function reload() {
    setup = await readSetup(client);
    draw();
    if (opts.onChange) opts.onChange(setup);
  }

  /** The editor for one profile, new or saved, in the pane. */
  function edit(p, isNew) {
    const taken = new Set(setup.profiles.map((x) => x.id));
    const name = text(p.name || "", "720p for the lobby", "Profile name");
    const id = text(p.id || "", "lobby-720p", "Profile id", { readonly: isNew ? null : true });
    const form = customForm(p, setup.encoders);
    const err = whyBox();
    const save = el("button.btn.primary", { type: "button", text: isNew ? "Save the profile" : "Save" });
    const cancel = el("button.btn", { type: "button", text: "Cancel" });
    name.addEventListener("input", () => { if (isNew && !id.dataset.typed) id.value = slug(name.value).toLowerCase(); });
    id.addEventListener("input", () => { id.dataset.typed = "1"; });
    pane.replaceChildren(
      el("h4.hs-sub", { text: isNew ? "New profile" : `Edit ${p.name || p.id}` }),
      !isNew && users(p.id).length ? el("p.hs-warn", { text: `${users(p.id).length} ${users(p.id).length === 1 ? "channel takes" : "channels take"} this profile, and each one's show is made again with the change.` }) : null,
      el("div.hs-grid", {}, [field("Name", name), field("Id", id, isNew ? "Letters, digits, - _ and ." : "An id stays once saved.")]),
      form.node, err.node, el("div.hs-row.hs-end", {}, [cancel, save]),
    );
    pane.hidden = false;
    list.hidden = true;
    cancel.onclick = () => { pane.hidden = true; list.hidden = false; };
    save.onclick = async () => {
      const want = { id: id.value.trim(), name: name.value.trim() || id.value.trim(), ...form.read() };
      if (!/^[A-Za-z0-9._-]{1,64}$/.test(want.id)) return err.say("Give the profile an id of letters, digits, - _ and . only.");
      if (isNew && taken.has(want.id)) return err.say(`There is already a profile ${want.id}. Pick another id.`);
      save.disabled = true;
      try {
        const got = await client.call("headend.profile.save", { profile: want });
        toast({ text: `${want.name} is saved.${(got.channels || []).length ? ` ${got.channels.length} ${got.channels.length === 1 ? "channel follows" : "channels follow"} it.` : ""}` });
        pane.hidden = true;
        list.hidden = false;
        await reload();
      } catch (e) {
        err.say(said(e));
      }
      save.disabled = false;
    };
  }

  /** Remove; refused when channels take it, then asks where they move. */
  async function remove(p, moveTo) {
    why.say([]);
    try {
      await client.call("headend.profile.remove", moveTo ? { id: p.id, move_to: moveTo } : { id: p.id });
      toast({ text: moveTo ? `${p.name || p.id} is removed, and its channels take ${moveTo}.` : `${p.name || p.id} is removed.` });
      await reload();
    } catch (e) {
      const using = (e && e.data && e.data.channels) || [];
      if (!using.length || moveTo) return why.say(said(e));
      moveBox(p, using, said(e));
    }
  }

  function moveBox(p, using, message) {
    const card = list.querySelector(`.hs-pcard[data-id="${CSS.escape(p.id)}"]`);
    const others = setup.profiles.filter((x) => x.id !== p.id).map((x) => [x.id, x.name || x.id]);
    const to = choice(others, "copy", "Move the channels to");
    const box = el("div.hs-warn.hs-move", {}, [
      el("p", { text: message }),
      el("p", { text: `${using.length === 1 ? "That channel takes" : `Those ${using.length} channels take`} another profile first. Pick which, and the profile is removed.` }),
      el("div.hs-row", {}, [field("Move them to", to), el("button.btn.he-danger", { type: "button", text: "Move them and remove", onclick: () => remove(p, to.value) })]),
    ]);
    const old = card && card.querySelector(".hs-move");
    if (old) old.remove();
    if (card) card.append(box);
    else why.node.after(box);
  }

  m.el.addEventListener("click", (e) => {
    const b = e.target.closest("[data-p]");
    if (!b) return;
    const p = setup.profiles.find((x) => x.id === (b.closest("[data-id]") || {}).dataset?.id);
    if (b.dataset.p === "new") return edit(structuredClone(STARTER), true);
    if (!p) return;
    if (b.dataset.p === "edit") return edit(p, false);
    if (b.dataset.p === "copy") return edit({ ...structuredClone(p), id: `${p.id}-copy`, name: `${p.name || p.id} (copy)`, builtin: false }, true);
    if (b.dataset.p === "remove") return remove(p);
  });

  draw();
  return { dialog: m, reload, remove };
}
