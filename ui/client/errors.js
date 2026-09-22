// The one error shape, and the reading of it a human needs.
//
// Every failure from the core arrives as {code, message, data}. The message
// already names the current state and the next step (03 section 6), so the UI
// shows it verbatim rather than inventing wording of its own. What this file
// adds is the small amount of structure a toast wants: a title, the next step
// pulled out where the server gave one, and whether offering a retry is honest.

/** Codes the core uses. Anything else falls through to the generic branch. */
export const CODES = {
  PARSE: -32700,
  INVALID_REQUEST: -32600,
  NO_METHOD: -32601,
  BAD_PARAMS: -32602,
  INTERNAL: -32603,
  WRONG_STATE: -32001,
  NO_SCOPE: -32002,
  SAFETY: -32003,
  NOT_FOUND: -32004,
  NO_PLACEMENT: -32005,
  PLUGIN_DIED: -32010,
  LINE_TOO_LONG: -32011,
  RESTART_REQUIRED: -32012,
  CONFIRM_REQUIRED: -32020,
};

const TITLES = {
  [CODES.NO_METHOD]: "This core does not have that command",
  [CODES.BAD_PARAMS]: "The command was not filled in correctly",
  [CODES.WRONG_STATE]: "Not ready for that yet",
  [CODES.NO_SCOPE]: "This token is not allowed to do that",
  [CODES.SAFETY]: "Held back by the safety settings",
  [CODES.NOT_FOUND]: "Not found",
  [CODES.NO_PLACEMENT]: "That plugin cannot run there",
  [CODES.PLUGIN_DIED]: "The plugin stopped during the call",
  [CODES.RESTART_REQUIRED]: "The plugin has to be reloaded",
  [CODES.CONFIRM_REQUIRED]: "Confirmation needed",
};

export class RpcError extends Error {
  constructor(code, message, data) {
    super(message || "the call failed");
    this.name = "RpcError";
    this.code = code;
    this.data = data || {};
  }

  /** A short heading for a toast. The message stays as the body. */
  get title() {
    return TITLES[this.code] || "That did not work";
  }

  /** True when trying the same call again could plausibly succeed. */
  get retryable() {
    if (typeof this.data.retryable === "boolean") return this.data.retryable;
    return this.code === CODES.WRONG_STATE || this.code === CODES.SAFETY || this.code === CODES.PLUGIN_DIED;
  }

  /** Milliseconds to wait before a retry, when the core named one. */
  get retryAfterMs() {
    const ms = this.data.retry_after_ms;
    return Number.isFinite(ms) ? ms : null;
  }

  /**
   * The button the core offers for this refusal, `data.action`, or null.
   * `{kind, label, ...}`; the kinds are in docs/reference/errors.md.
   */
  get action() {
    const a = this.data.action;
    return a && typeof a.kind === "string" && typeof a.label === "string" ? a : null;
  }

  /** The sentence after the last full stop is the next step, by convention. */
  get nextStep() {
    const parts = String(this.message).split(/(?<=\.)\s+/).filter(Boolean);
    return parts.length > 1 ? parts[parts.length - 1] : "";
  }
}

/** Turn anything thrown by a transport into an RpcError. */
export function asRpcError(e) {
  if (e instanceof RpcError) return e;
  if (e && typeof e.code === "number") return new RpcError(e.code, e.message, e.data);
  return new RpcError(CODES.INTERNAL, e && e.message ? e.message : String(e), {});
}
