// One guest in the Guests panel: their picture, where they are, and what the
// operator can do with them. Every button is one call of the public protocol.

import { el } from "../../shell/dom.js";
import { confirmModal } from "../../shell/modal.js";
import { openLink } from "./share.js";

const WORDS = {
  invited: ["Invited", "The link has not been opened yet"],
  joining: ["Joining", "Their browser is connecting"],
  live: ["Live", "Their camera and microphone are in the mixer"],
  gone: ["Gone", "They left or lost the network; the same link brings them back"],
};

export function guestCard(client, guest, talk, fail) {
  const call = (method, params) => client.call(method, params).catch(fail);
  const img = el("img.guests-still", { alt: "" });
  const blank = el("div.guests-blank.sm.dim", { text: "No picture yet" });
  const state = el("span.guests-state.sm");
  const name = el("strong.ellipsis", { text: guest.name });
  const expires = el("span.sm.dim.ellipsis");
  const mute = el("button.btn", { type: "button", onclick: () => call("source.audio.set", { id: guest.source, muted: !card.muted }) });
  const take = el("button.btn", { type: "button", text: "Take", title: "Put this guest on air, full screen", onclick: () => call("program.take", { source: guest.source }) });
  const link = el("button.btn", { type: "button", text: "Link", title: "The link and the code to send them again", onclick: () => call("guest.invite.link", { id: guest.id }).then((l) => l && openLink(l, guest.name)) });
  const revoke = el("button.btn.danger", { type: "button", text: "Revoke", onclick: () => letGo(client, guest, fail) });
  const card = {
    muted: false,
    stillUrl: "",
    root: el("div.guests-card", { "data-guest": guest.id }, [
      el("div.guests-picture", {}, [img, blank]),
      el("div.col.guests-about", {}, [
        el("div.row", {}, [state, name]),
        expires,
        el("div.row.guests-buttons", {}, [take, mute, talk.button(guest.id, "Talk"), link, revoke]),
      ]),
    ]),
    update(g, source) {
      guest = g;
      const [word, why] = WORDS[g.state] || [g.state, ""];
      state.textContent = word;
      state.title = why;
      state.dataset.state = g.state;
      name.textContent = g.name;
      expires.textContent = `Link works until ${new Date(g.expires).toLocaleString()}`;
      const here = g.state === "live" || g.state === "joining";
      take.disabled = !here;
      this.paintMute(source);
      if (!here) this.clearStill();
    },
    paintMute(source) {
      this.muted = !!(source && source.muted);
      mute.textContent = this.muted ? "Unmute" : "Mute";
      mute.disabled = !source;
      mute.setAttribute("aria-pressed", this.muted ? "true" : "false");
    },
    async still(source) {
      this.paintMute(source);
      try {
        const res = await fetch(client.snapshotUrl(guest.source, 320));
        if (!res.ok) return;
        const url = URL.createObjectURL(await res.blob());
        if (this.stillUrl) URL.revokeObjectURL(this.stillUrl);
        this.stillUrl = url;
        img.src = url;
        blank.hidden = true;
      } catch {
        /* the last picture stays */
      }
    },
    clearStill() {
      if (this.stillUrl) URL.revokeObjectURL(this.stillUrl);
      this.stillUrl = "";
      img.removeAttribute("src");
      blank.hidden = false;
    },
  };
  return card;
}

async function letGo(client, guest, fail) {
  const sure = await confirmModal(`Revoke ${guest.name}'s invite? Their camera and their return end at once, and the link stops working.`, "Revoke");
  if (sure) await client.call("guest.invite.revoke", { id: guest.id }).catch(fail);
}
