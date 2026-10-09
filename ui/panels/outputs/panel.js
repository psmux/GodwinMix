// Outputs: where the programme is going, and whether it is arriving.
//
// The buffer depth and the reconnect count are the two numbers that tell an
// operator whether a wobbly link is coping, so both are on the row rather than
// behind a hover.
//
// The state is spelled out in words a volunteer can act on. "Reconnecting" and
// a count of twelve is the truth, but "Needs a stream key" is the truth *and*
// the next step, and it is the one that used to end with somebody being told
// to open a TOML file on the machine. `has_key` on the record is what makes
// the difference readable here without the key ever reaching this page.

import { el, clear } from "../../shell/dom.js";
import { confirmModal } from "../../shell/modal.js";
import { errorToast, toast } from "../../shell/toast.js";
import { registerAll } from "../../shell/commands.js";
import { settings } from "../../shell/settings.js";
import { startRecording, recordingRow, recordingState, isRecording } from "./recording.js";
// Destination setup is needed only when adding or editing an output.
import { lazyAction } from "../../shell/lazy-action.js";
import { addChannel } from "../channels/entry.js";
import { failureLabel, failureAdvice, failureOf } from "./failure.js";
const addDestination = lazyAction(() => import("./destination.js").then(m => m.addDestination), "Add destination");
const editDestination = lazyAction(() => import("./destination.js").then(m => m.editDestination), "Edit destination");
// Resources, plan lines and the HLS card: see views.js.
const views = () => import("./views.js");

/**
 * How often a row's numbers are read back from the core while the panel is on
 * screen and has something to put them in.
 */
const REFRESH_MS = 1000;

/** The state of one destination, in words that say what to do about it. */
export function stateLabel(output) {
  if (output.state === "live") return "Live";
  // Stopped by a person with output.stop: nothing is sent, the key is kept.
  if (output.state === "stopped") return "Stopped";
  // Ahead of the connection state on purpose: a destination still carrying a
  // preset's placeholder is not going to connect, and "Reconnecting, attempt
  // 47" tells nobody why.
  if (output.has_key === false) return "Needs a stream key";
  // What went wrong, when the core said, ahead of how many goes it has had.
  const failed = failureLabel(output);
  if (failed) return failed;
  switch (output.state) {
    case "connecting":
      return "Connecting";
    case "reconnecting":
      return `Reconnecting, attempt ${output.reconnects || 1}`;
    case "failed":
      return "Not sending";
    default:
      return output.state || "";
  }
}

/** Goes before the count stops being the news. The `own` policy settles at
    one every two seconds, so this is about twenty seconds of trying. */
export const ADVICE_AFTER = 10;

/**
 * "Reconnecting, attempt 152" is the truth and it is not the next step. A
 * destination that has never answered is a wrong address or a server that is
 * not running, and both are fixed away from this page.
 */
export function stalledAdvice(output) {
  // The core's own reason, when it gave one, says more than a count can.
  if (output.state === "stopped") return "Nothing is sent. The address and stream key are kept, so Start streaming sends again.";
  const said = failureAdvice(output);
  if (said) return said;
  if (output.has_key === false) return "";
  if (output.state !== "reconnecting") return "";
  if ((output.reconnects || 0) < ADVICE_AFTER) return "";
  // The core cuts an address off after the host and puts an ellipsis there,
  // which reads badly in front of a full stop. The host is the part worth
  // naming anyway: it is what nothing answered at.
  const host = String(output.uri_host || "").replace(/\/?…$/, "");
  const where = host ? ` at ${host}` : "";
  return `Nothing answered${where}. Check the address and that the server is up.`;
}

/** Which dot a destination gets. A missing key is a fault, not a warning. */
export function dotClass(output) {
  if (output.state === "live") return "live";
  if (output.state === "stopped") return "stopped";
  if (output.has_key === false) return "failed";
  // Trying again after a refusal is not on its way up; it is failing.
  if (failureOf(output)) return "failed";
  if (output.state === "connecting" || output.state === "reconnecting") return "connecting";
  return "failed";
}

class OutputsPanel extends HTMLElement {
  static get panel() {
    return { id: "core/outputs", title: "Outputs", slots: ["footer"], tag: "gmx-outputs" };
  }

  setClient(client) {
    this.client = client;
  }

  connectedCallback() {
    if (this.built) return;
    this.built = true;
    this.count = el("span.sm.dim");
    this.list = el("div.col.pad");
    const tab = (id, kids) => el("button", { role: "tab", "aria-selected": String(id < 1), onclick: () => this.show(id ? "resources" : "outputs") }, kids);
    this.tabs = el("div.out-tabs", { role: "tablist" }, [tab(0, [el("strong", { text: "Outputs " }), this.count]), tab(1, ["Resources"])]);
    this.append(
      el("div.row.pad.out-head", {}, [
        this.tabs,
        el("span.grow"),
        el("button.btn", { text: "Record", onclick: () => startRecording(this.client) }),
        el("button.btn", { text: "Add destination", onclick: () => this.add() }),
        el("button.btn.out-add-channel", { text: "Add Channel", onclick: () => addChannel(this.client) }),
      ]),
      this.list
    );
    this.offs = [
      this.client.onRender((s) => this.render(s)),
      registerAll([
        { id: "output.add-destination", title: "Add a destination", group: "Outputs", run: () => this.add() },
      ]),
    ];
    this.render(this.client.state);
  }

  disconnectedCallback() {
    for (const off of this.offs || []) off();
    this.offs = [];
    this.follow(false);
    this.stopViews();
  }

  setWorkspaceActive(active) {
    this.workspaceActive = active;
    if (active) this.show(this.view || "outputs");
    else {
      this.follow(false);
      this.stopViews();
    }
  }

  show(view) {
    if (view === "outputs" && !this.resources) return this.render(this.client.state);
    return views().then((m) => m.show(this, view));
  }

  stopViews() {
    if (this.unplan || this.resources) views().then((m) => m.stopAll(this));
  }

  /**
   * Keep the numbers on the rows moving.
   *
   * A status is sent when something changes, and these numbers change while
   * nothing does: a recording's megabytes climb and a link's buffer breathes.
   * So a row stood at whatever the last status said, which for a recording is
   * "0.0 MB sent to the file" for as long as it runs. Read them back on a
   * clock instead, while there is a row to put them in and the panel is on
   * screen, and stop the moment either stops being true.
   */
  follow(wanted) {
    if (!wanted || this.workspaceActive === false) {
      if (this.timer) clearInterval(this.timer);
      this.timer = null;
      return;
    }
    if (!this.timer) this.timer = setInterval(() => this.refresh(), REFRESH_MS);
  }

  /** One read, and never two at once: a slow core must not grow a queue. */
  async refresh() {
    if (this.refreshing) return;
    this.refreshing = true;
    try {
      await this.client.refreshOutputs();
    } catch {
      // The rows keep the numbers they have. A core that is not answering is
      // already said in the header, and a toast a second would be its own bug.
    } finally {
      this.refreshing = false;
    }
  }

  add() {
    return addDestination(this.client);
  }

  /**
   * Rows are kept and written into, never rebuilt while they stand.
   *
   * This used to clear the list and make every row again on each render, and
   * a render comes with every status the core sends: about thirty a second
   * with one destination reconnecting. A click is a press and a release on the
   * same element, and a person holds a button for a tenth of a second, so the
   * button was gone before the release and Edit, Reconnect and Remove did
   * nothing at all. A scripted click, which takes no time, always worked,
   * which is how every test passed. A row is made again only when what it is
   * changes: see `shape`.
   */
  render(s) {
    if (this.workspaceActive === false) return;
    const outputs = s.outputs || [];
    this.count.textContent = outputs.length ? String(outputs.length) : "";
    if (this.view === "resources") return;
    this.follow(outputs.length > 0);
    if (outputs.length ? !this.unplan : this.unplan) views().then((m) => m.followPlan(this, outputs.length > 0));
    this.rows = this.rows || new Map();
    if (!outputs.length) {
      if (this.rows.size || !this.list.firstChild) {
        this.rows.clear();
        clear(this.list);
        this.list.appendChild(
          el("p.dim.sm", {
            text: "Nothing is being sent anywhere. Add a destination to go live.",
            style: { margin: 0 },
          })
        );
      }
      return;
    }
    if (!this.rows.size) clear(this.list);
    const wanted = new Set(outputs.map((o) => o.id));
    for (const [id, row] of this.rows) {
      if (wanted.has(id)) continue;
      row.node.remove();
      this.rows.delete(id);
    }
    let before = this.list.firstChild;
    for (const output of outputs) {
      let row = this.rows.get(output.id);
      if (!row || row.shape !== shape(output)) {
        const made = this.row(output);
        // The row being replaced may be the one the next insert is placed
        // before; once it is out of the list, the new one takes its place.
        if (row && before === row.node) before = made.node;
        if (row) row.node.replaceWith(made.node);
        row = made;
        this.rows.set(output.id, row);
      }
      row.update(output);
      if (row.node !== before) this.list.insertBefore(row.node, before);
      before = row.node.nextSibling;
    }
  }

  /** One row and the function that writes a newer status into it. */
  row(output) {
    if (isRecording(output)) return recordingRow(this.client, output);
    const needsKey = output.has_key === false;
    // What the buttons act on is read when they are pressed, so a row that
    // has stood through an edit sends the destination as it is now.
    let current = output;
    const dot = el("span.dot");
    const label = el("span.sm" + (needsKey ? ".needs-key" : ".dim"));
    const host = el("span.ellipsis.grow");
    const numbers = el("span.num");
    const advice = el("div.sm.output-advice", { hidden: true });
    const plan = el("div.output-plan");
    const extra = el("div");
    const hls = output.type === "hls/output";
    if (hls) views().then((m) => m.hlsInto(this.client, extra, () => current));
    const node = el("div.output-row", {}, [
      el("div.row", {}, [
        dot,
        el("strong", { text: output.id }),
        label,
        el("span.grow"),
        // An HLS output has no address or key to edit; a new ladder is a new
        // output, so it has no Edit.
        hls ? null : el("button.btn.icon" + (needsKey ? ".primary" : ""), {
          text: needsKey ? "Add key" : "Edit",
          onclick: () => editDestination(this.client, current),
        }),
        // Stop keeps the destination and its key; Remove forgets both.
        el("button.btn.icon" + (output.state === "stopped" ? ".primary" : ".danger"), {
          text: output.state === "stopped" ? "Start streaming" : "Stop streaming",
          onclick: () => import("./stream-toggle.js").then((m) => m.toggle(this.client, current)),
        }),
        // A destination with no key is never dialled, so there is nothing to
        // reconnect until the key is in, and a stopped one is not dialled.
        needsKey || output.state === "stopped" ? null : el("button.btn.icon", {
          text: "Reconnect",
          onclick: async () => {
            try {
              await this.client.call("output.reconnect", { id: current.id });
              toast({ text: `Reconnecting ${current.id}.` });
            } catch (e) {
              errorToast(e, "Reconnect");
            }
          },
        }),
        el("button.btn.icon.danger", {
          text: "Remove",
          onclick: async () => {
            const ask = `Remove "${current.id}"? It stops sending and forgets the address and stream key. Stop streaming keeps them.`;
            if (settings().confirmRemove && !(await confirmModal(ask, "Remove"))) return;
            try {
              await this.client.call("output.remove", { id: current.id });
            } catch (e) {
              errorToast(e, "Remove");
            }
          },
        }),
      ]),
      el("div.row.sm.faint.output-detail", {}, [host, numbers]),
      plan,
      advice,
      extra,
    ]);
    const update = (next) => {
      current = next;
      // A line of its own rather than a longer label: the label shares a row
      // with three buttons, and a sentence in there wraps them.
      const say = stalledAdvice(next);
      write(advice, "textContent", say);
      write(advice, "hidden", !say);
      write(dot, "className", "dot " + dotClass(next));
      write(dot, "title", next.state || "");
      write(label, "textContent", stateLabel(next));
      write(host, "textContent", hls ? "Served from this mixer's own port" : next.uri_host || "");
      write(numbers, "textContent", `buffer ${Number(next.queue_secs || 0).toFixed(1)}s · ${next.reconnects || 0} reconnects`);
      write(plan, "textContent", this.planText ? this.planText(next.id) : "");
      if (extra.update) extra.update(next);
    };
    return { node, update, shape: shape(output) };
  }
}

/** Set a property only when it differs, so a quiet status touches nothing. */
export function write(node, key, value) {
  if (node[key] !== value) node[key] = value;
}

/**
 * What decides how a row is built, as opposed to what it says. Two statuses
 * with the same shape are the same row with different words in it.
 */
export function shape(output) {
  if (isRecording(output)) return "recording:" + (recordingState(output).dot === "failed" ? "failed" : "ok");
  return "destination:" + (output.has_key === false ? "needs-key" : "keyed") + (output.state === "stopped" ? ":stopped" : "");
}

customElements.define("gmx-outputs", OutputsPanel);
if (window.godwinmixPanels) window.godwinmixPanels.push(OutputsPanel);
export default OutputsPanel;
