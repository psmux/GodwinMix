#!/usr/bin/env node
/* {{description}}
 *
 * A GodwinMix source plugin. It draws colour bars at the canvas caps and writes
 * them to stdout as raw I420 frames in a streamable Matroska stream. Control is
 * JSON-RPC 2.0, one object per line, on stdin and stderr.
 *
 * Change draw() and schemas/source.json. Nothing else here needs touching.
 */
"use strict";

const readline = require("node:readline");

const API = 1;
const TIMECODE_SCALE_NS = 1000000;
const CLUSTER_MS = 2000;
const UNKNOWN = Buffer.from("01ffffffffffffff", "hex"); // an EBML size that never ends
// Y, U, V for the eight standard bars, white through to black.
const BARS = [[235, 128, 128], [210, 16, 146], [170, 166, 16], [145, 54, 34],
              [106, 202, 222], [81, 90, 240], [41, 240, 110], [16, 128, 128]];

const MEDIA = process.stdout;
const state = {
  canvas: null, params: {}, running: false, failed: false, timer: null,
  frames: 0, index: 0, clusterMs: null, header: false,
};


// --- what you change --------------------------------------------------------

/**
 * Return one I420 frame of exactly the right size for the canvas.
 *
 * An I420 frame is a Y plane of width*height bytes, then a U plane and a V
 * plane of ceil(width/2)*ceil(height/2) each. Y is brightness, 16 is black and
 * 235 is white; U and V are colour, 128 each is grey.
 *
 * This draws eight colour bars and ignores the time. Your plugin will not.
 */
function draw(canvas, params, ptsNs) {
  const width = canvas.width;
  const height = canvas.height;
  const cw = (width + 1) >> 1;
  const ch = (height + 1) >> 1;
  const asked = Number.parseInt(params.bars, 10);
  const count = Math.max(1, Math.min(8, Number.isFinite(asked) ? asked : 8));

  const bar = (x) => BARS[Math.min(Math.floor((x * count) / width), count - 1)];

  const luma = Buffer.alloc(width);
  for (let x = 0; x < width; x++) luma[x] = bar(x)[0];
  const blue = Buffer.alloc(cw);
  const red = Buffer.alloc(cw);
  for (let x = 0; x < cw; x++) {
    blue[x] = bar(x * 2)[1];
    red[x] = bar(x * 2)[2];
  }

  const frame = Buffer.alloc(width * height + 2 * cw * ch);
  let at = 0;
  for (let y = 0; y < height; y++) at += luma.copy(frame, at);
  for (let y = 0; y < ch; y++) at += blue.copy(frame, at);
  for (let y = 0; y < ch; y++) at += red.copy(frame, at);
  return frame;
}


// --- the control channel ----------------------------------------------------

/** One JSON object on one line of stderr. stdout is media and only media. */
function send(obj) {
  process.stderr.write(JSON.stringify(obj) + "\n");
}

function log(level, message) {
  send({ jsonrpc: "2.0", method: "log", params: { level, message } });
}

function reply(rid, result) {
  if (rid !== undefined && rid !== null) {
    send({ jsonrpc: "2.0", id: rid, result });
  }
}

function fail(rid, code, message, data) {
  send({ jsonrpc: "2.0", id: rid, error: { code, message, data } });
}


// --- the container transport: streamable Matroska on stdout -----------------

/** An element id, written the way the Matroska spec prints it. */
function id(hex) {
  return Buffer.from(hex, "hex");
}

/** A data size, as an EBML variable length integer, in the shortest form. */
function vint(value) {
  for (let length = 1; length <= 8; length++) {
    if (value < Math.pow(2, 7 * length) - 1) {
      // No shifts: a 1080p frame is over 2**21 bytes, and JavaScript's bitwise
      // operators wrap at 32 bits. Divide instead, then set the marker bit.
      const out = Buffer.alloc(length);
      let rest = value;
      for (let i = length - 1; i >= 0; i--) {
        out[i] = rest % 256;
        rest = Math.floor(rest / 256);
      }
      out[0] |= 1 << (8 - length);
      return out;
    }
  }
  throw new RangeError("a size no Matroska stream ever has");
}

/** An unsigned integer with its leading zero bytes removed. */
function uint(value) {
  const out = Buffer.alloc(8);
  let rest = value;
  for (let i = 7; i >= 0; i--) {
    out[i] = rest % 256;
    rest = Math.floor(rest / 256);
  }
  let start = 0;
  while (start < 7 && out[start] === 0) start++;
  return out.subarray(start);
}

function elem(elementId, payload) {
  return Buffer.concat([elementId, vint(payload.length), payload]);
}

function str(text) {
  return Buffer.from(text + "\0", "utf8");
}

/** EBML header, an open ended Segment, Info, and one raw video track. */
function writeHeader(canvas) {
  const ebml = Buffer.concat([
    elem(id("4282"), str("matroska")), elem(id("4287"), uint(4)),
    elem(id("4285"), uint(2))]);
  const info = Buffer.concat([
    elem(id("2ad7b1"), uint(TIMECODE_SCALE_NS)),
    elem(id("4d80"), str("{{name}}")), elem(id("5741"), str("{{name}}"))]);
  const video = Buffer.concat([
    elem(id("b0"), uint(canvas.width)), elem(id("ba"), uint(canvas.height)),
    elem(id("9a"), uint(2)), elem(id("2eb524"), Buffer.from("I420"))]);
  const track = Buffer.concat([
    elem(id("d7"), uint(1)), elem(id("73c5"), uint(1)), elem(id("83"), uint(1)),
    elem(id("86"), str("V_UNCOMPRESSED")),
    elem(id("23e383"), uint(Math.floor(1000000000 / canvas.fps))),
    elem(id("e0"), video)]);
  MEDIA.write(Buffer.concat([
    elem(id("1a45dfa3"), ebml), id("18538067"), UNKNOWN,
    elem(id("1549a966"), info),
    elem(id("1654ae6b"), elem(id("ae"), track))]));
  state.header = true;
}

/**
 * One SimpleBlock, opening a Cluster when the last one is full. Returns false
 * when stdout has taken all it can hold for now.
 */
function writeFrame(ptsNs, data) {
  const ms = Math.floor(ptsNs / TIMECODE_SCALE_NS);
  let base = state.clusterMs;
  if (base === null || ms - base >= CLUSTER_MS) {
    MEDIA.write(Buffer.concat([id("1f43b675"), UNKNOWN, elem(id("e7"), uint(ms))]));
    base = state.clusterMs = ms;
  }
  const head = Buffer.alloc(4);
  head[0] = 0x81;                     // track 1, as a variable length integer
  head.writeInt16BE(ms - base, 1);    // this frame's place inside the cluster
  head[3] = 0x80;                     // a keyframe, which every raw frame is
  MEDIA.write(Buffer.concat([id("a3"), vint(head.length + data.length), head]));
  return MEDIA.write(data);
}

/**
 * Frames at canvas fps, PTS on our own clock starting at zero.
 *
 * The deadline comes from the frame count, not from adding a sleep each time,
 * so one slow draw does not push every later frame back.
 */
function produce() {
  const canvas = state.canvas;
  const stepNs = Math.floor(1000000000 / canvas.fps);
  const expected = Math.floor((canvas.width * canvas.height * 3) / 2);
  const started = process.hrtime.bigint();

  const schedule = () => {
    const due = state.index * stepNs;
    const elapsed = Number(process.hrtime.bigint() - started);
    state.timer = setTimeout(tick, Math.max(0, (due - elapsed) / 1e6));
  };

  const tick = () => {
    state.timer = null;
    if (!state.running) return;
    const pts = state.index * stepNs;
    const frame = draw(canvas, state.params, pts);
    if (frame.length !== expected) {
      state.failed = true;
      log("error", "draw() returned " + frame.length + " bytes, the canvas needs " + expected);
      return;
    }
    let room;
    try {
      room = writeFrame(pts, frame);
    } catch (err) {
      state.failed = true;
      log("error", "the media pipe closed: " + (err.code || err.message));
      return;
    }
    state.frames++;
    state.index++;
    // stdout is full: wait for the reader rather than piling frames up in
    // memory. The next deadline still comes from the frame count.
    if (room) schedule();
    else MEDIA.once("drain", () => { if (state.running) schedule(); });
  };

  state.index = 0;
  schedule();
}


// --- the methods the core calls ---------------------------------------------

function onStart(rid, params) {
  const transport = params.transport || "container";
  if (transport !== "container") {
    return fail(rid, -32602,
      "this plugin only speaks the container transport. Declare "
      + "transports = [\"container\"] in gmx-plugin.toml, which is the default.",
      { transport, retryable: false });
  }
  state.canvas = params.canvas || state.canvas;
  if (!state.header) writeHeader(state.canvas);
  state.running = true;
  state.failed = false;
  produce();
  reply(rid, { latency_ms: 0 });
}

function onStop(rid) {
  state.running = false;
  if (state.timer !== null) {
    clearTimeout(state.timer);
    state.timer = null;
  }
  MEDIA.removeAllListeners("drain");
  reply(rid, {});
}

/** Answer one call. Returns false when the process should exit. */
function dispatch(message) {
  const rid = message.id;
  const method = message.method;
  const params = message.params || {};
  if (method === "start") {
    onStart(rid, params);
  } else if (method === "stop") {
    onStop(rid);
  } else if (method === "configure") {
    // The full validated object, not a diff. draw() reads it next frame.
    state.params = params.params ?? params;
    reply(rid, { applied: true });
  } else if (method === "health") {
    reply(rid, {
      state: state.failed ? "failing" : "ok",
      detail: state.frames + " frames sent",
      latency_ms: 0,
    });
  } else if (method === "shutdown") {
    onStop(null);
    reply(rid, {});
    return false;
  } else if (method === "keyframe" || method === "initialized") {
    reply(rid, {});
  } else {
    fail(rid, -32601,
      "this plugin has no method '" + method + "'. It implements start, stop, "
      + "configure, health and shutdown.",
      { method, retryable: false });
  }
  return true;
}

function main() {
  // A broken media pipe is a stop, not a crash: the core has gone away.
  MEDIA.on("error", (err) => {
    state.running = false;
    state.failed = true;
    log("error", "the media pipe closed: " + (err.code || err.message));
  });

  const provides = [{
    kind: "source", id: "source", transports: ["container"],
    media: { video: "raw", audio: "none", alpha: false, thumb: true },
    capabilities: ["restart-in-place", "health"], latency_ms: 0,
    settings: "schemas/source.json", skill: "skills/source/SKILL.md",
  }];
  send({
    jsonrpc: "2.0", id: 0, method: "initialize",
    params: {
      plugin: "{{name}}", version: "0.1.0", api: API,
      transports: ["container"], provides,
    },
  });

  const lines = readline.createInterface({ input: process.stdin });
  lines.on("line", (raw) => {
    const line = raw.trim();
    if (!line) return;
    let message;
    try {
      message = JSON.parse(line);
    } catch (err) {
      log("info", "ignored a line that was not JSON: " + line.slice(0, 200));
      return;
    }
    if (message.id === 0 && "result" in message) {
      const ready = message.result;
      const canvas = state.canvas = ready.canvas;
      state.params = ready.params || {};
      log("info", "{{name}} at " + canvas.width + "x" + canvas.height + "@"
        + canvas.fps + " as '" + (ready.instance || "?") + "'");
      send({ jsonrpc: "2.0", method: "initialized", params: {} });
    } else if ("method" in message && !dispatch(message)) {
      lines.close();
    }
  });
  lines.on("close", () => {
    state.running = false;
    if (state.timer !== null) clearTimeout(state.timer);
    state.timer = null;
    process.stdin.destroy();
    // Nothing is left to wait for. The event loop drains stdout and exits.
  });
}

if (require.main === module) {
  main();
}

module.exports = { draw, vint, uint, elem };
