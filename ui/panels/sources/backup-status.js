// What a source's backup is doing, in words an operator reads mid show.
//
// No elements here, so the tests reach it without a page. The core sends a
// snapshot each time a source moves phase; between snapshots the countdowns
// are run down here from the numbers in the last one and the time since it
// came, so nothing is asked of the core for a clock.

/** The phases that change by themselves and so want a ticking clock. */
const TICKING = new Set(["lost", "on_backup", "returning"]);

/** 12000 is "0:12", 3725000 is "1:02:05". */
export function clock(ms) {
  const total = Math.max(0, Math.floor(ms / 1000));
  const s = String(total % 60).padStart(2, "0");
  const m = Math.floor(total / 60) % 60;
  const h = Math.floor(total / 3600);
  return h ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

/** Whole seconds left of `ms` once `elapsed` has gone, never below zero. */
export function secondsLeft(ms, elapsed) {
  return Math.max(0, Math.ceil(((ms || 0) - (elapsed || 0)) / 1000));
}

/**
 * The line for a source with a backup, or null when there is nothing to say.
 *
 * @param {object} source   a SourceStatus
 * @param {(id: string) => string} nameFor   a source's name by its id
 * @param {number} elapsed  milliseconds since this snapshot arrived
 * @returns {{phase, text, warning, ticking, actions: {method, label}[]}|null}
 */
export function backupView(source, nameFor, elapsed = 0) {
  const b = source && source.backup;
  if (!b || !b.phase) return null;
  const name = nameFor(source.id);
  const warning = b.warning || "";
  const view = { phase: b.phase, text: "", warning, ticking: TICKING.has(b.phase), actions: [] };
  if (b.phase === "ready") {
    if (!warning) return null;
    view.text = `${name} is live`;
  } else if (b.phase === "lost") {
    const left = secondsLeft(b.switch_in_ms, elapsed);
    view.text = left ? `${name} lost, backup in ${left} s` : `${name} lost, switching to the backup`;
  } else if (b.phase === "on_backup") {
    const lost = b.lost_ms === undefined ? `${name} is down` : `${name} lost ${clock(b.lost_ms + elapsed)}`;
    view.text = b.showing ? `${lost}, showing ${nameFor(b.showing)}` : `${lost}, and its backup is down too`;
  } else if (b.phase === "returning") {
    const left = secondsLeft(b.return_in_ms, elapsed);
    view.text = left ? `${name} is back, returning in ${left} s` : `${name} is back, returning now`;
    view.actions = [
      { method: "source.backup.return", label: "Return now" },
      { method: "source.backup.stay", label: "Stay on backup" },
    ];
  } else if (b.phase === "main_back") {
    view.text = b.showing ? `${name} is back, ${nameFor(b.showing)} is still on` : `${name} is back`;
    view.actions = [{ method: "source.backup.return", label: `Back to ${name}` }];
  } else {
    return null;
  }
  return view;
}

/**
 * When each source's last backup snapshot arrived, so a countdown runs from
 * the right moment. A snapshot with the same numbers keeps its first time:
 * a redraw for some other reason does not restart the clock.
 */
export class BackupClock {
  constructor(now = () => Date.now()) {
    this.now = now;
    this.seen = new Map();
  }

  /** Milliseconds since this source's backup last changed. */
  elapsed(source) {
    const key = JSON.stringify(source.backup || null);
    const was = this.seen.get(source.id);
    if (!was || was.key !== key) {
      this.seen.set(source.id, { key, at: this.now() });
      return 0;
    }
    return this.now() - was.at;
  }
}
