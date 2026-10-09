// Reading what a person pastes from Livebox into channels to make. No DOM and
// no calls, so the tests can hand it text and read back what it found.
//
// Two shapes are taken, in any mix:
//
//   rtmp://host:1935/Church/main?psk=Sunday-2024          one whole address a line
//
//   STREAM URL  rtmp://host:1935/Church/                  the dashboard's two fields,
//   STREAM KEY  main?psk=Sunday-2024                      the label on its own line or not
//
// Each line found becomes {line, app, stream, secret}, with `line` the number
// a person counts from 1. A line that cannot be read is {line, error}, with a
// sentence that says what to fix. Whether the app and secret are acceptable
// is the core's to decide: channel.add says so in its own words.

const LABEL = /^(stream\s*url|stream\s*key|server|url|key)(\s*[:=]\s*|\s+|$)/i;
const URL_AT_START = /^(rtmps?):\/\/([^/\s]+)\/?(.*)$/i;
const KEY_PARAMS = ["psk", "key", "token", "Token"];

/** `%20` and `+` back to spaces, leaving anything malformed as it came. */
function unescape(raw, plus) {
  const s = plus ? raw.replace(/\+/g, " ") : raw;
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}

/** The password from a query, by any of the names the listener takes. */
export function passwordOf(query) {
  const pairs = query.split("&").map((p) => p.split("=")).filter((p) => p.length >= 2);
  for (const want of KEY_PARAMS) {
    const hit = pairs.find(([k]) => k === want);
    if (hit) return unescape(hit.slice(1).join("="), true);
  }
  return "";
}

/** `main?psk=x` into the stream name and the password. */
function streamKey(text) {
  const at = text.indexOf("?");
  const name = (at < 0 ? text : text.slice(0, at)).replace(/^\/+|\/+$/g, "");
  return { stream: name, secret: at < 0 ? "" : passwordOf(text.slice(at + 1)) };
}

/** One entry, or the reason it is not one. */
function entry(line, app, key) {
  const { stream, secret } = streamKey(key);
  if (!app) return { line, error: `Line ${line}: the address has no channel name after the port, such as rtmp://host:1935/Church/main?psk=password.` };
  if (!secret) return { line, error: `Line ${line}: there is no ?psk= password on it. Paste the stream key as Livebox shows it, such as main?psk=password.` };
  return { line, app, stream: stream || "main", secret };
}

/** The non empty lines, numbered, with any label taken off the front. */
function lines(text) {
  return String(text || "")
    .split(/\r?\n/)
    .map((raw, i) => ({ n: i + 1, text: raw.trim().replace(LABEL, "").trim() }))
    .filter((l) => l.text);
}

/**
 * Everything pasted, as channels to make. A server address on its own waits
 * for the stream key on the line after it.
 */
export function parseLivebox(text) {
  const out = [];
  let server = null;
  for (const { n, text: t } of lines(text)) {
    const url = URL_AT_START.exec(t);
    if (!url) {
      if (server) out.push(entry(server.n, server.app, t));
      else out.push({ line: n, error: `Line ${n}: this is not an rtmp:// address, and no Stream URL came before it to go with it as a stream key.` });
      server = null;
      continue;
    }
    if (server) out.push({ line: server.n, error: `Line ${server.n}: a Stream URL with no stream key after it. Put main?psk=password on the next line.` });
    const rest = url[3];
    const q = rest.indexOf("?");
    const path = (q < 0 ? rest : rest.slice(0, q)).replace(/\/+$/, "");
    const query = q < 0 ? "" : rest.slice(q);
    const parts = path.split("/").filter(Boolean);
    const app = unescape(parts[0] || "", false).trim();
    if (parts.length < 2 && !query) {
      server = { n, app };
      continue;
    }
    server = null;
    out.push(entry(n, app, parts.slice(1).join("/") + query));
  }
  if (server) out.push({ line: server.n, error: `Line ${server.n}: a Stream URL with no stream key after it. Put main?psk=password on the next line.` });
  return out;
}
