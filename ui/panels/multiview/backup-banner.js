// Under the programme: every source that is down or on its backup, with its
// clock and the button to bring it back. The tile says the same, but the
// programme is where an operator is looking when a camera drops.
//
// Empty, and with no timer, while every source is fine.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { nameOf } from "../sources/local.js";
import { backupLine, namesIn } from "../sources/backup-line.js";

/** The sources whose backup has something to say on the programme. */
export function standingIn(state) {
  return ((state && state.sources) || []).filter((s) => s.backup && s.backup.phase && s.backup.phase !== "ready");
}

export class BackupBanner {
  constructor(client) {
    this.client = client;
    this.lines = new Map();
    this.node = el("div.backup-banner", { hidden: true, "aria-live": "polite" });
  }

  render(state) {
    const list = standingIn(state);
    const names = namesIn(state, nameOf);
    const keep = new Set(list.map((s) => s.id));
    for (const [id, line] of this.lines) {
      if (keep.has(id)) continue;
      line.node.remove();
      this.lines.delete(id);
    }
    for (const source of list) {
      let line = this.lines.get(source.id);
      if (!line) {
        line = backupLine({ call: (method, id) => this.call(method, id) });
        this.lines.set(source.id, line);
        this.node.appendChild(line.node);
      }
      line.sync(source, names);
    }
    this.node.hidden = list.length === 0;
  }

  call(method, id) {
    this.client.call(method, { id }).catch((e) => errorToast(e, "Backup"));
  }
}
