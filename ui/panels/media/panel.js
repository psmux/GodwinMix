// The media library and the ad break.
//
// Every behaviour the single page had is here: the listing with its web safe
// and mute chips, convert with a progress bar, upload one file at a time with a
// progress line, add a clip as a source, a cue in seconds, roll and end early,
// and a custom path for anything not in the library.

import { el, clear, on, fmtBytes } from "../../shell/dom.js";
import { errorToast, toast } from "../../shell/toast.js";
import { confirmModal } from "../../shell/modal.js";
import { registerAll } from "../../shell/commands.js";
import { shell } from "../../shell/shell.js";
import { fmtPosition } from "../../shell/fader.js";

const AD_KEY = "gmx.adUri";
const REFRESH_MS = 500;

class MediaPanel extends HTMLElement {
  static get panel() {
    return { id: "core/media", title: "Media", slots: ["footer"], tag: "gmx-media" };
  }

  setClient(client) {
    this.client = client;
  }

  connectedCallback() {
    if (this.built) return;
    this.built = true;
    this.items = [];
    try {
      this.selected = localStorage.getItem(AD_KEY) || "";
    } catch {
      this.selected = "";
    }

    this.list = el("div.col.pad", { style: { maxHeight: "220px", overflow: "auto" } });
    this.count = el("span.sm.dim");
    this.hint = el("div.sm.faint.pad");
    this.cue = el("input", { type: "number", min: "0", max: "600", value: "0", style: { width: "5em" }, title: "Seconds from now" });
    this.custom = el("input", { type: "text", placeholder: "Or a path the mixer can read" });
    this.file = el("input", { type: "file", accept: "video/*,.mkv,.ts,.flv", multiple: true, hidden: true });
    on(this.file, "change", () => this.upload(this.file.files));

    this.append(
      el("div.row.pad", {}, [
        el("strong", { text: "Media" }),
        this.count,
        el("span.grow"),
        el("button.btn", { text: "Rescan", onclick: () => this.load() }),
        el("button.btn", { text: "Upload", onclick: () => this.file.click() }),
      ]),
      this.list,
      el("div.row.pad", {}, [
        el("span.sm.dim", { text: "Ad break in" }),
        this.cue,
        el("span.sm.dim", { text: "s" }),
        el("button.btn.primary", { text: "Roll ad", onclick: () => this.roll() }),
        el("button.btn", { text: "End early", onclick: () => this.endAd() }),
        el("span.grow"),
      ]),
      el("details.pad", {}, [el("summary.sm.dim", { text: "Custom path" }), this.custom]),
      this.hint,
      this.file
    );

    this.offs = [
      this.client.on("media-changed", () => this.loadSoon()),
      this.client.onRender(() => this.paintAd()),
      registerAll([
        { id: "media.upload", title: "Upload a clip", group: "Media", run: () => this.file.click() },
        { id: "adbreak.start", title: "Roll the ad break", group: "Media", run: () => this.roll() },
        { id: "adbreak.end", title: "End the ad break", group: "Media", run: () => this.endAd() },
      ]),
    ];
    // The shell hands dropped files here rather than guessing what they are for.
    shell.onFiles = (files) => this.upload(files);
    this.load();
  }

  setWorkspaceActive(active) {
    this.workspaceActive = active;
    if (!active) {
      clearTimeout(this.soon);
      this.soon = null;
      return;
    }
    this.paintAd();
    this.load();
  }

  disconnectedCallback() {
    for (const off of this.offs || []) off();
    this.offs = [];
    if (this.soon) clearTimeout(this.soon);
    if (shell.onFiles) shell.onFiles = null;
  }

  loadSoon() {
    if (this.workspaceActive === false) return;
    // A conversion sends one event per percent. Coalesce.
    if (this.soon) clearTimeout(this.soon);
    this.soon = setTimeout(() => this.load(), REFRESH_MS);
  }

  async load() {
    if (this.workspaceActive === false) return;
    try {
      const listing = await this.client.call("media.list", {});
      this.items = listing.items || [];
      this.dir = listing.dir;
      this.error = listing.error;
      this.render();
    } catch (e) {
      this.list.textContent = "";
      this.list.appendChild(el("p.dim.sm", { text: e.message, style: { margin: 0 } }));
    }
  }

  render() {
    this.count.textContent = this.items.length ? String(this.items.length) : "";
    clear(this.list);
    if (!this.items.length) {
      this.list.appendChild(
        el("p.dim.sm", { text: this.error || `Nothing in ${this.dir || "the media directory"}. Drop a file on the window to upload one.`, style: { margin: 0 } })
      );
      return;
    }
    for (const item of this.items) this.list.appendChild(this.row(item));
  }

  row(item) {
    const converting = item.conversion && item.conversion.state === "running";
    const failed = item.conversion && item.conversion.state === "failed";
    const node = el("div.row", {
      title: item.path + (failed ? "\n" + item.conversion.error : ""),
      style: { cursor: "pointer", background: this.selected === item.path ? "var(--raise)" : "" },
      onclick: () => this.select(item),
      ondblclick: () => {
        this.select(item);
        this.roll();
      },
    }, [
      el("span.ellipsis.grow", { text: item.name }),
      item.web_safe || item.converted_path ? el("span.pill", { text: "web", style: { color: "var(--ok)" } }) : null,
      item.has_audio === false ? el("span.pill", { text: "mute", style: { color: "var(--warn)" } }) : null,
      failed ? el("span.pill", { text: "convert failed", style: { color: "var(--bad)" } }) : null,
      el("span.num.sm.faint", { text: item.duration_ms ? fmtPosition(item.duration_ms) : fmtBytes(item.size_bytes) }),
      converting
        ? el("span.sm.dim", { text: `converting ${Math.round((item.conversion.progress || 0) * 100)}%` })
        : el("button.btn.icon", { text: "Add as source", "data-nodrag": "", onclick: (e) => {
            e.stopPropagation();
            this.addAsSource(item);
          } }),
      !item.web_safe && !item.converted_path && !converting
        ? el("button.btn.icon", {
            text: "Convert",
            title: (item.reasons || []).join("; "),
            onclick: (e) => {
              e.stopPropagation();
              e.currentTarget.disabled = true;
              this.client.call("media.convert", { name: item.name }).catch((err) => errorToast(err, "Convert"));
            },
          })
        : null,
      el("button.btn.icon.danger", {
        text: "×",
        title: "Delete",
        onclick: async (e) => {
          e.stopPropagation();
          if (!(await confirmModal(`Delete "${item.name}" from the library?`, "Delete"))) return;
          this.client.call("media.remove", { name: item.name }).catch((err) => errorToast(err, "Delete"));
        },
      }),
    ]);
    return node;
  }

  select(item) {
    this.selected = item.converted_path || item.path;
    try {
      localStorage.setItem(AD_KEY, this.selected);
    } catch {
      /* the choice lasts the session */
    }
    this.custom.value = this.selected;
    this.render();
  }

  addAsSource(item) {
    this.client
      .call("source.add", { uri: item.converted_path || item.path, name: item.name })
      .then(() => toast({ text: `${item.name} added.` }))
      .catch((e) => errorToast(e, "Add as source"));
  }

  async roll() {
    const uri = (this.custom.value || this.selected || "").trim();
    if (!uri) {
      toast({ kind: "warning", text: "Pick a clip first, or type a path." });
      return;
    }
    const delay = Number(this.cue.value) || 0;
    const params = { uri };
    if (delay > 0) params.at_running_time_ms = (this.client.state.running_time_ms || 0) + delay * 1000;
    try {
      await this.client.call("adbreak.start", params);
      toast({ text: delay > 0 ? `Ad break in ${delay}s.` : "Ad break rolling." });
    } catch (e) {
      errorToast(e, "Roll ad");
    }
  }

  endAd() {
    this.client.call("adbreak.end", {}).catch((e) => errorToast(e, "End ad"));
  }

  paintAd() {
    if (this.workspaceActive === false) return;
    const ad = this.client.state.ad;
    this.hint.textContent = ad
      ? ad.on_air
        ? `On air: ${ad.uri}${ad.return_to ? `, then back to ${ad.return_to}` : ""}`
        : `Cued: ${ad.uri}`
      : "";
  }

  /** One file at a time: the same machine is encoding a live programme. */
  async upload(files) {
    const list = [...(files || [])];
    if (!list.length) return;
    for (let i = 0; i < list.length; i += 1) {
      const file = list[i];
      this.hint.textContent = `Uploading ${file.name} (${i + 1} of ${list.length})…`;
      try {
        await this.client.upload(file.name, file, (fraction) => {
          this.hint.textContent = `Uploading ${file.name}: ${Math.round(fraction * 100)}%`;
        });
      } catch (e) {
        // Carry on past one bad file rather than abandoning the batch.
        this.hint.textContent = `${file.name} did not upload: ${e.message}`;
        continue;
      }
    }
    this.hint.textContent = "Uploaded.";
    setTimeout(() => {
      if (this.hint.textContent === "Uploaded.") this.hint.textContent = "";
    }, 3000);
    this.load();
  }
}

customElements.define("gmx-media", MediaPanel);
window.godwinmixPanels.push(MediaPanel);
export default MediaPanel;
