// A pasted list of feeds into rows, and rows into show.add_many's shapes.
// One address per line, or CSV (or tab separated, as a spreadsheet copies)
// with the columns name, input, program, output, format, with or without
// a header line naming them.

export const FIELDS = ["name", "input", "program", "output", "format"];

const ALIASES = {
  name: "name", show: "name", channel: "name", title: "name",
  input: "input", uri: "input", url: "input", source: "input", feed: "input", address: "input",
  program: "program", programme: "program", program_number: "program", service: "program",
  output: "output", outputs: "output", destination: "output", target: "output",
  format: "format", rendition: "format", preset: "format",
};

export const TEMPLATE = [
  "name,input,program,output,format",
  "News,udp://@239.1.1.1:5000,1,udp://10.0.0.50:6000,copy",
  "Sport,udp://@239.1.1.2:5000,1,udp://10.0.0.50:6002,copy",
  "Movies,udp://@239.1.1.3:5000,1,udp://10.0.0.50:6004,copy",
].join("\n");

/** One line of CSV, quotes and doubled quotes honoured. */
export function splitLine(line, delim) {
  const out = [];
  let cell = "";
  let quoted = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (quoted) {
      if (c === '"' && line[i + 1] === '"') { cell += '"'; i++; }
      else if (c === '"') quoted = false;
      else cell += c;
    } else if (c === '"' && !cell.trim()) quoted = true;
    else if (c === delim) { out.push(cell.trim()); cell = ""; }
    else cell += c;
  }
  out.push(cell.trim());
  return out;
}

/** A bare multicast address and port is a UDP input; anything else is left as typed. */
export function fixInput(s) {
  const v = String(s || "").trim();
  if (/^2(2[4-9]|3\d)\.\d+\.\d+\.\d+:\d+$/.test(v)) return `udp://@${v}`;
  return v;
}

/** A name from an address, for a list that gave none. */
export function nameFor(uri, i) {
  const m = /^[a-z]+:\/\/@?([^/?#]+)(\/[^?#]*)?/i.exec(uri || "");
  if (!m) return `Feed ${i + 1}`;
  const tail = (m[2] || "").split("/").filter(Boolean).pop();
  return tail ? tail : m[1];
}

/** @returns {Array<{name: string, input: string, program: string, output: string, format: string}>} */
export function parse(text) {
  const lines = String(text || "").split(/\r?\n/).map((l) => l.trim()).filter((l) => l && !l.startsWith("#"));
  if (!lines.length) return [];
  const delim = lines.some((l) => l.includes("\t")) ? "\t" : lines[0].includes(",") ? "," : null;
  if (!delim) return lines.map((l, i) => row({ input: l }, i));
  const first = splitLine(lines[0], delim).map((c) => ALIASES[c.toLowerCase().replace(/\s+/g, "_")]);
  const header = first.includes("input");
  const cols = header ? first : FIELDS;
  return lines.slice(header ? 1 : 0).map((l, i) => {
    const cells = splitLine(l, delim);
    const r = {};
    cols.forEach((f, k) => { if (f && cells[k] !== undefined) r[f] = cells[k]; });
    return row(r, i);
  });
}

function row(r, i) {
  const input = fixInput(r.input);
  return { name: r.name || nameFor(input, i), input, program: r.program || "", output: r.output || "", format: r.format || "" };
}

/** What a format cell asks for: copy, a preset by id, or nothing to send. */
export function rendition(format) {
  const f = String(format || "").trim();
  if (!f || /^(copy|same|passthrough|pass)$/i.test(f)) return null;
  return { preset: f };
}

/** One row as a ShowAdd for show.add_many: a direct show with its one output. */
export function toShow(r) {
  const input = { uri: fixInput(r.input) };
  const program = parseInt(r.program, 10);
  if (program > 0) input.program = program;
  const outputs = String(r.output || "").split(/\s*;\s*/).filter(Boolean).map((uri) => ({ uri, rendition: rendition(r.format) }));
  return { name: String(r.name || "").trim(), compositing: false, input, outputs };
}
