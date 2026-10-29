// The keys a presenter, a floor manager or a foot pedal presses.
//
// Most pedals and presentation clickers send Page Down and Page Up, some send
// the arrows or a space, so all of them do something sensible here.

/** Which move a key asks for, or null. */
export function moveFor(e) {
  if (e.altKey || e.ctrlKey || e.metaKey) return null;
  switch (e.key) {
    case " ":
    case "Spacebar":
    case "k":
      return { method: "toggle" };
    case "ArrowDown":
    case "ArrowRight":
      return { method: "prompter.speed", params: { by: 0.1 } };
    case "ArrowUp":
    case "ArrowLeft":
      return { method: "prompter.speed", params: { by: -0.1 } };
    case "PageDown":
    case ".":
      return { method: "prompter.seek", params: { step: 1 } };
    case "PageUp":
    case ",":
      return { method: "prompter.seek", params: { step: -1 } };
    case "Home":
      return { method: "prompter.seek", params: { row: 0 } };
    case "End":
      return { method: "prompter.seek", params: { fraction: 1 } };
    case "m":
    case "M":
      return { local: "mirror" };
    case "f":
    case "F":
      return { local: "fullscreen" };
    default:
      return null;
  }
}

/** Listen on `target` and hand each move to `run`. */
export function bindKeys(target, run) {
  target.addEventListener("keydown", (e) => {
    const move = moveFor(e);
    if (!move) return;
    // Space and the page keys would scroll the page itself.
    e.preventDefault();
    if (e.repeat && move.method !== "prompter.speed") return;
    run(move);
  });
}
