// The names pasted destinations get. A line is named for whose server it is
// (YouTube, Facebook, or the host of a server of your own), and a strip of
// tiles all called 127.0.0.1 tells nobody which is which, so a name that
// would repeat is made longer until it does not.

/**
 * Names a person can tell apart. Two lines to one server of your own are
 * both named by the host and the first part of the path (`10.0.0.9/live`);
 * a name still in use, on this channel or earlier in the paste, gets a
 * number after it. `taken` is the labels the channel already has.
 */
export function distinctLabels(lines, taken = []) {
  const good = lines.filter((l) => !l.error);
  const used = new Set(taken);
  return lines.map((item) => {
    if (item.error) return item;
    const twice = used.has(item.label) || good.some((o) => o !== item && o.label === item.label);
    const base = twice ? withPath(item) : item.label;
    let label = base;
    for (let n = 2; used.has(label); n++) label = `${base} ${n}`;
    used.add(label);
    return { ...item, label };
  });
}

/** `host/first-part-of-the-path` for a server named by its host, else the label as it is. */
function withPath(item) {
  try {
    const url = new URL(item.server);
    const first = url.pathname.split("/").filter(Boolean)[0];
    return url.hostname === item.label && first ? `${item.label}/${decodeURIComponent(first)}` : item.label;
  } catch {
    return item.label;
  }
}
