// What the backup section of a source's settings reads and sends.
//
// No elements here, so the tests check the request without a page. The
// section itself is backup-form.js.

/** The same defaults the core uses for a rule that names only its backup. */
export const DEFAULTS = { backup: "", after_secs: 3, transition: "fade", auto: true, return_after_secs: 5 };

/** The transitions offered, in the order an operator reaches for them. */
export const TRANSITIONS = [
  ["cut", "Cut"],
  ["fade", "Dissolve"],
  ["wipe", "Wipe"],
  ["dip", "Dip to black"],
  ["slide", "Slide"],
  ["push", "Push"],
  ["zoom", "Zoom"],
  ["box", "Box"],
];

export const NOTHING = "Nothing (hold the last frame, then black)";

/** The boxes' values for a source, from its rule or the defaults. */
export function ruleValues(source) {
  const rule = source.on_loss;
  if (!rule) return { ...DEFAULTS };
  const transition = String(rule.transition || "fade").toLowerCase();
  return {
    backup: rule.backup || "",
    after_secs: rule.after_secs ?? DEFAULTS.after_secs,
    transition: transition === "dissolve" ? "fade" : transition,
    auto: rule.return !== "manual",
    return_after_secs: rule.return_after_secs ?? DEFAULTS.return_after_secs,
  };
}

/** Every other source, by name, after the choice of none. */
export function backupChoices(source, sources, nameOf) {
  const others = (sources || []).filter((s) => s.id !== source.id);
  return [["", NOTHING], ...others.map((s) => [s.id, nameOf(s)])];
}

/** A whole number of seconds or more, or the default when the box is empty. */
function seconds(value, fallback) {
  const n = Number(value);
  return Number.isFinite(n) && n >= 0 && String(value).trim() !== "" ? n : fallback;
}

/**
 * The call that makes the source's rule what the boxes say, or null when
 * there is nothing to do: "Nothing" on a source that has no rule.
 */
export function ruleRequest(source, values) {
  const id = source.id;
  if (!values.backup) return source.on_loss ? { method: "source.on_loss.clear", params: { id } } : null;
  const params = {
    id,
    backup: values.backup,
    after_secs: seconds(values.after_secs, DEFAULTS.after_secs),
    transition: values.transition || DEFAULTS.transition,
    return: values.auto ? "auto" : "manual",
  };
  if (values.auto) params.return_after_secs = seconds(values.return_after_secs, DEFAULTS.return_after_secs);
  return { method: "source.on_loss.set", params };
}

/**
 * Why the chosen backup could not stand in right now, or "". The core's own
 * warning for the backup the source has; a word from here for one just
 * picked, before the core has had a chance to say.
 */
export function backupWarning(source, chosen, sources, nameOf) {
  if (!chosen) return "";
  if (source.on_loss && source.on_loss.backup === chosen && source.backup && source.backup.warning) {
    return source.backup.warning;
  }
  const backup = (sources || []).find((s) => s.id === chosen);
  if (!backup || backup.state === "live") return "";
  return `${nameOf(backup)} has no picture right now, so it could not stand in until it does.`;
}

/** The sentence under the boxes once a rule is saved. */
export function ruleSummary(source, values, nameFor) {
  const name = nameFor(source.id);
  if (!values.backup) return `Nothing stands in for ${name}.`;
  const after = seconds(values.after_secs, DEFAULTS.after_secs);
  const back = values.auto ? "It switches back by itself." : "You switch back when you are ready.";
  return `If ${name} goes down, ${nameFor(values.backup)} goes on after ${after} s. ${back}`;
}
