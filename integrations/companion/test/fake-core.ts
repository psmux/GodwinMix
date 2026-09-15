// A fake core: enough of RFC 6455 and of the protocol to test a client.
//
// A real HTTP server on a real port, upgrading a real socket, so the client is
// exercised over the wire rather than against a mock of its own transport. The
// framing is written out here rather than pulled from `ws` because the point of
// this package is that it needs no dependencies, and a test that installs one
// is a test that cannot run on a machine with no registry.
//
// The same pattern appears in docs/how-to/write-a-ui.md, which is where a
// reader is told to copy it from.

import { createHash } from "node:crypto";
import { createServer, type Server } from "node:http";
import type { Duplex } from "node:stream";

const GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/** One call, as the fake core recorded it. */
export interface Call {
  method: string;
  params: Record<string, unknown>;
  id?: number;
}

/**
 * One connected client.
 *
 * Held apart from the core because a test with two panels on one mixer has to
 * be able to say which of them a message went to. Serving the session down the
 * socket that asked for it is what fixes the order the second panel sees: its
 * snapshot lands before anything the test pushes afterwards, every run.
 */
export interface Connection {
  /** Every call this client made, in order. */
  calls: Call[];
  /** Push one `event/...` notification to this client alone. */
  notify(name: string, params: unknown): void;
  /** Push one binary multiview frame to this client alone. */
  frame(seq: number, layout: number, runningTimeMs: number, jpeg: Uint8Array): void;
}

export interface FakeCore {
  url: string;
  /** The port the kernel gave us. Never guessed, never fixed. */
  port: number;
  /** Every call every client made, in order. */
  calls: Call[];
  /** The clients that are connected now, in the order they arrived. */
  connections: Connection[];
  /** Push one `event/...` notification to every connected client. */
  notify(name: string, params: unknown): void;
  /** Push one binary multiview frame. */
  frame(seq: number, layout: number, runningTimeMs: number, jpeg: Uint8Array): void;
  /** Answer this method with this result, or with `{ error }` to refuse it. */
  answer(method: string, reply: unknown | ((params: Record<string, unknown>) => unknown)): void;
  /** Take this method and never answer it, the way a wedged core does. */
  silence(method: string): void;
  /**
   * Wait until this method has been called `count` times and answered.
   *
   * The way to wait for a client to have got somewhere: a condition the core
   * can see, rather than a sleep long enough to be probably true. Calls that
   * already happened count, so asking after the fact answers at once.
   */
  waitForCall(method: string, count?: number): Promise<Call>;
  /** Wait until `count` clients have upgraded their sockets. */
  waitForConnections(count: number): Promise<Connection[]>;
  /** Drop every connection, the way a core being restarted does. */
  drop(): void;
  stop(): Promise<void>;
}

/**
 * Start a fake core. `onSubscribe` is the interesting hook: the core answers
 * `core.subscribe` and then sends the snapshot, the deltas and the flush, and
 * that sequence is what a client library has to get right.
 *
 * The hook is handed the connection that subscribed as well as the core, so a
 * session is served to one client rather than sprayed at all of them.
 */
export async function fakeCore(
  opts: { onSubscribe?: (core: FakeCore, conn: Connection) => void } = {},
): Promise<FakeCore> {
  const sockets = new Map<Duplex, Connection>();
  const calls: Call[] = [];
  const answers = new Map<string, unknown | ((params: Record<string, unknown>) => unknown)>();
  const silent = new Set<string>();
  const callWaiters: Array<{ method: string; count: number; resolve: (call: Call) => void }> = [];
  const connWaiters: Array<{ count: number; resolve: (conns: Connection[]) => void }> = [];

  const server: Server = createServer((_req, res) => {
    res.writeHead(404).end();
  });
  // A listen error would otherwise take the whole process down with a stack
  // that says nothing about which test was starting a core.
  server.on("error", (e) => {
    throw e;
  });

  const settleCalls = (): void => {
    for (let i = callWaiters.length - 1; i >= 0; i -= 1) {
      const waiter = callWaiters[i]!;
      const matching = calls.filter((c) => c.method === waiter.method);
      if (matching.length < waiter.count) continue;
      callWaiters.splice(i, 1);
      waiter.resolve(matching[waiter.count - 1]!);
    }
  };

  const settleConns = (): void => {
    const list = [...sockets.values()];
    for (let i = connWaiters.length - 1; i >= 0; i -= 1) {
      const waiter = connWaiters[i]!;
      if (list.length < waiter.count) continue;
      connWaiters.splice(i, 1);
      waiter.resolve(list);
    }
  };

  const core: FakeCore = {
    url: "",
    port: 0,
    calls,
    get connections() {
      return [...sockets.values()];
    },
    notify(name, params) {
      for (const conn of sockets.values()) conn.notify(name, params);
    },
    frame(seq, layout, runningTimeMs, jpeg) {
      for (const conn of sockets.values()) conn.frame(seq, layout, runningTimeMs, jpeg);
    },
    answer(method, reply) {
      answers.set(method, reply);
    },
    silence(method) {
      silent.add(method);
    },
    waitForCall(method, count = 1) {
      return new Promise<Call>((resolve) => {
        callWaiters.push({ method, count, resolve });
        settleCalls();
      });
    },
    waitForConnections(count) {
      return new Promise<Connection[]>((resolve) => {
        connWaiters.push({ count, resolve });
        settleConns();
      });
    },
    drop() {
      for (const socket of sockets.keys()) socket.destroy();
      sockets.clear();
    },
    async stop() {
      core.drop();
      await new Promise<void>((resolve) => server.close(() => resolve()));
    },
  };

  server.on("upgrade", (req, socket: Duplex) => {
    const key = String(req.headers["sec-websocket-key"] || "");
    const accept = createHash("sha1").update(key + GUID).digest("base64");
    socket.write(
      "HTTP/1.1 101 Switching Protocols\r\n" +
        "Upgrade: websocket\r\nConnection: Upgrade\r\n" +
        `Sec-WebSocket-Accept: ${accept}\r\n\r\n`,
    );
    const conn: Connection = {
      calls: [],
      notify(name, params) {
        const body = JSON.stringify({ jsonrpc: "2.0", method: name, params });
        socket.write(frame(0x1, Buffer.from(body)));
      },
      frame(seq, layout, runningTimeMs, jpeg) {
        const header = Buffer.alloc(16);
        header.writeUInt32LE(seq, 0);
        header.writeUInt32LE(layout, 4);
        header.writeBigUInt64LE(BigInt(runningTimeMs), 8);
        socket.write(frame(0x2, Buffer.concat([header, Buffer.from(jpeg)])));
      },
    };
    sockets.set(socket, conn);
    socket.on("close", () => sockets.delete(socket));
    socket.on("error", () => sockets.delete(socket));
    settleConns();
    read(socket, (opcode, payload) => {
      if (opcode === 0x8) {
        socket.end(frame(0x8, Buffer.alloc(0)));
        return;
      }
      if (opcode !== 0x1) return;
      let call: { id?: number; method?: string; params?: Record<string, unknown> };
      try {
        call = JSON.parse(payload.toString());
      } catch {
        return;
      }
      if (!call.method) return;
      const record: Call = { method: call.method, params: call.params || {}, id: call.id };
      calls.push(record);
      conn.calls.push(record);
      if (silent.has(call.method)) return;
      const made = answers.has(call.method)
        ? answers.get(call.method)
        : {
            error: {
              code: -32601,
              message: `this fake core has no ${call.method}. Register one with core.answer().`,
            },
          };
      const result = typeof made === "function" ? made(call.params || {}) : made;
      if (call.id !== undefined) {
        const body =
          result && typeof result === "object" && "error" in (result as object)
            ? { jsonrpc: "2.0", id: call.id, error: (result as { error: unknown }).error }
            : { jsonrpc: "2.0", id: call.id, result: result ?? {} };
        socket.write(frame(0x1, Buffer.from(JSON.stringify(body))));
      }
      if (call.method === "core.subscribe") opts.onSubscribe?.(core, conn);
      // Last, so a test waiting on a call is released once the answer and the
      // session that follows it are already on the wire.
      settleCalls();
    });
  });

  // Port 0: the kernel picks a free one and we read it back. Two cores started
  // at the same moment cannot collide, which a fixed number cannot promise.
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (typeof address !== "object" || !address || !address.port) {
    throw new Error("the fake core is listening but has no port");
  }
  core.port = address.port;
  core.url = `http://127.0.0.1:${address.port}`;
  return core;
}

/** One server to client frame: never masked, never fragmented. */
function frame(opcode: number, payload: Buffer): Buffer {
  const len = payload.length;
  let header: Buffer;
  if (len < 126) {
    header = Buffer.from([0x80 | opcode, len]);
  } else if (len < 65536) {
    header = Buffer.alloc(4);
    header[0] = 0x80 | opcode;
    header[1] = 126;
    header.writeUInt16BE(len, 2);
  } else {
    header = Buffer.alloc(10);
    header[0] = 0x80 | opcode;
    header[1] = 127;
    header.writeBigUInt64BE(BigInt(len), 2);
  }
  return Buffer.concat([header, payload]);
}

/** Client to server frames, which are always masked. */
function read(socket: Duplex, onFrame: (opcode: number, payload: Buffer) => void): void {
  let buffer = Buffer.alloc(0);
  socket.on("data", (chunk: Buffer) => {
    buffer = Buffer.concat([buffer, chunk]);
    for (;;) {
      if (buffer.length < 2) return;
      const opcode = buffer[0]! & 0x0f;
      const masked = (buffer[1]! & 0x80) !== 0;
      let length = buffer[1]! & 0x7f;
      let offset = 2;
      if (length === 126) {
        if (buffer.length < 4) return;
        length = buffer.readUInt16BE(2);
        offset = 4;
      } else if (length === 127) {
        if (buffer.length < 10) return;
        length = Number(buffer.readBigUInt64BE(2));
        offset = 10;
      }
      const mask = masked ? buffer.subarray(offset, offset + 4) : null;
      if (masked) offset += 4;
      if (buffer.length < offset + length) return;
      const payload = Buffer.from(buffer.subarray(offset, offset + length));
      if (mask) {
        for (let i = 0; i < payload.length; i++) payload[i]! ^= mask[i % 4]!;
      }
      buffer = buffer.subarray(offset + length);
      onFrame(opcode, payload);
    }
  });
}

/** The snapshot a test starts from: two sources, one of them on air. */
export function snapshot(seq = 42): unknown {
  return {
    seq,
    state: {
      program: "cam1",
      running_time_ms: 1500,
      uptime_secs: 12,
      backend: {},
      multiview: { enabled: true, cols: 2, rows: 1, width: 640, height: 180, fps: 4, cells: [] },
      outputs: [{ id: "twitch", uri_host: "live.twitch.tv", state: "live", reconnects: 0, queue_secs: 0.2 }],
      sources: [
        { id: "cam1", name: "Camera 1", uri: "rtmp://a", state: "live", has_video: true, has_audio: true },
        { id: "cam2", name: "Camera 2", uri: "rtmp://b", state: "connecting", has_video: true, has_audio: true },
      ],
    },
  };
}
