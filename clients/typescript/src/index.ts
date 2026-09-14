// @godwinmix/client
//
// The same contract the first party UI uses: one WebSocket at `/rpc`, JSON-RPC
// over it, `core.subscribe` to say what you want, a snapshot then deltas, and
// `event/flush` to say when to repaint.
//
//   import { connect } from "@godwinmix/client";
//
//   const client = await connect({ base: "http://127.0.0.1:8080", token });
//   client.onFlush((state) => draw(state.sources, state.program));
//   await client.programTake({ source: "cam1" });
//
// The types, the typed methods and the event map come from `protocol.json` by
// way of `clients/gen/generate.py`. Everything else is hand written: the
// socket, the store, the frame decoder, the URLs, the video helpers and the
// schema reader.
//
// The major version of this package is the `api_level` it speaks. A core is
// safe when its `core.info` reports api_compatible <= API_LEVEL <= api_level.

export { Client, connect, UI_EVENTS } from "./client.ts";
export type { ConnectOptions, Want } from "./client.ts";

export { CODES, RpcError, asRpcError } from "./errors.ts";
export type { RpcErrorData } from "./errors.ts";

export { HEADER_BYTES, bareFrame, cellFor, parseFrame, sheetWidthFor } from "./frames.ts";
export type { Frame } from "./frames.ts";

export { Store, emptyState } from "./store.ts";
export type { Listener, State } from "./store.ts";

export { RpcSocket } from "./rpc.ts";
export type { RpcHooks, WebSocketFactory, WebSocketLike } from "./rpc.ts";

export { httpBase, mjpegUrl, rpcUrl, snapshotUrl, whepUrl } from "./urls.ts";

export { attachMjpeg, attachWhep } from "./video.ts";
export type { WhepHandle } from "./video.ts";

export {
  applyConditions,
  describeForm,
  missing,
  readForm,
  valuesOf,
} from "./schema-form.ts";
export type { Choice, ControlKind, FormDescription, FormField, ShowWhen } from "./schema-form.ts";

// The designer kits: the record mirror, prediction, undo, canvas geometry,
// handles, snapping, safe areas, the UI schema layer and the renderer registry.
// Namespaced, because they carry names a surface is likely to have of its own
// (View, Handle, Rect) and because `kits.applyDrag` says where it came from.
export * as kits from "./kits/index.ts";

export {
  API_COMPATIBLE,
  API_LEVEL,
  EVENT_NAMES,
  EXT_KEYS,
  GeneratedMethods,
  METHODS,
} from "./generated/protocol.ts";
export type * from "./generated/protocol.ts";
