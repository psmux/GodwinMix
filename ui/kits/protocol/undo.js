// The undo proxy: Ctrl+Z on this page is `scene.undo` in the core.
//
// The undo stack for the document lives in the core, as an inverse diff stack
// (11 section 4). A client that kept its own would be wrong the moment a second
// client, an agent or the CLI changed anything, so this class does not keep
// one. What it keeps is the shell's stack of one line labels, each step of
// which calls the core and lets the core decide what the inverse actually is.
//
// A drag is one step, not forty. `scene.history.mark` is what folds the moves
// between two marks into a single entry, which is why `group()` exists.

export class UndoProxy {
  /**
   * @param {{call: (m: string, p?: object) => Promise<any>}} client
   * @param {{push: (entry: object) => void}} stack the shell's undo stack
   */
  constructor(client, stack) {
    this.client = client;
    this.stack = stack;
    /** False once the core has told us it has no history, so we stop asking. */
    this.available = true;
  }

  /** Name what follows, so a drag becomes one Ctrl+Z. */
  async mark(label) {
    if (!this.available) return;
    try {
      await this.client.call("scene.history.mark", label ? { label } : {});
    } catch (e) {
      if (e && e.code === -32601) this.available = false;
    }
  }

  /**
   * Run a batch of edits as one undo step.
   *
   * The first mark carries the label; the second closes the group, which is
   * the shape `scene.history.mark` documents. The entry is pushed only if the
   * work succeeded: a failed edit has nothing to take back.
   */
  async group(label, fn, opts = {}) {
    await this.mark(label);
    const result = await fn();
    await this.mark(null);
    this.record(label, opts);
    return result;
  }

  /**
   * One finished change, as a line in the shell's undo menu.
   * `offer` puts an Undo button in a toast, which the destructive ones want.
   */
  record(label, opts = {}) {
    if (!this.available) return;
    this.stack.push({
      label,
      offer: !!opts.offer,
      undo: () => this.undo(),
      redo: () => this.redo(),
    });
  }

  undo() {
    return this.step("scene.undo");
  }

  redo() {
    return this.step("scene.redo");
  }

  /**
   * The core answers with the patch it applied and how many steps are left, so
   * a menu can grey itself out without asking a second question.
   */
  async step(method) {
    const answer = await this.client.call(method, {});
    if (this.onStep) this.onStep(answer);
    return answer;
  }
}
