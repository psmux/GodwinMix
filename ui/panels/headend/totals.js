// The band's numbers, worked out from what the view holds: channels live
// of total, the box from machine.status, and what comes in and goes out,
// the governor's count where it has one, else every channel's summed.

const LIVE = new Set(["live", "silent", "black", "frozen", "scrambled"]);

/** Arrays summed index by index, lined up at their newest end. */
export function summed(lists, n = 60) {
  const out = new Array(n).fill(0);
  for (const l of lists) {
    const tail = l.slice(-n);
    const at = n - tail.length;
    tail.forEach((v, i) => { out[at + i] += v || 0; });
  }
  return out;
}

export function totals(all, data, box) {
  const series = [...data.series.values()];
  const inHist = summed(series.map((s) => s.inK));
  const outHist = summed(series.map((s) => s.outK));
  const gov = data.gov || {};
  return {
    total: all.length,
    live: all.filter((r) => LIVE.has(r.state)).length,
    problems: all.filter((r) => r.state !== "live").length,
    box: box.box,
    inK: gov.ingress_kbps || inHist.at(-1),
    outK: gov.egress_kbps || outHist.at(-1),
    inHist,
    outHist,
  };
}
