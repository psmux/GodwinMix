// Undo and redo.
//
// Every drop, delete, rename and layout paste pushes an entry. An entry is two
// functions and a sentence: what to do to take it back, what to do to put it
// back, and what to call it in the toast. Nothing here knows what a source is.
//
// A limit of 50 is deliberate. The stack holds closures over ids, not state, so
// it stays small, and an operator who wants to go back further than fifty
// actions wants a saved layout rather than a longer stack.

import { toast, undoToast } from "./toast.js";

const LIMIT = 50;

export class UndoStack {
  constructor() {
    this.done = [];
    this.undone = [];
    this.listeners = new Set();
  }

  onChange(fn) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  _changed() {
    for (const fn of this.listeners) fn(this);
  }

  get canUndo() {
    return this.done.length > 0;
  }

  get canRedo() {
    return this.undone.length > 0;
  }

  get nextUndoLabel() {
    return this.canUndo ? this.done[this.done.length - 1].label : "";
  }

  /**
   * @param {{label: string, undo: () => any, redo: () => any, offer?: boolean}} entry
   * `offer` puts an Undo button in a toast, which the dangerous ones want.
   */
  push(entry) {
    this.done.push(entry);
    if (this.done.length > LIMIT) this.done.shift();
    this.undone.length = 0;
    this._changed();
    if (entry.offer) undoToast(entry.label, () => this.undo());
  }

  async undo() {
    const entry = this.done.pop();
    if (!entry) {
      toast({ text: "Nothing to undo." });
      return;
    }
    this._changed();
    try {
      await entry.undo();
      this.undone.push(entry);
    } catch (e) {
      // Putting it back on the stack would offer a second try at something the
      // server just refused, so it goes away and the operator is told why.
      toast({ kind: "error", text: `Could not undo ${entry.label}: ${e.message}` });
    }
    this._changed();
  }

  async redo() {
    const entry = this.undone.pop();
    if (!entry) {
      toast({ text: "Nothing to redo." });
      return;
    }
    this._changed();
    try {
      await entry.redo();
      this.done.push(entry);
    } catch (e) {
      toast({ kind: "error", text: `Could not redo ${entry.label}: ${e.message}` });
    }
    this._changed();
  }

  clear() {
    this.done.length = 0;
    this.undone.length = 0;
    this._changed();
  }
}
