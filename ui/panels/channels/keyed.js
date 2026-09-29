// Rows kept and written into, never rebuilt while they stand.
//
// The Outputs panel learned why: a render arrives with every status, and a
// button remade between the press and the release of a click does nothing.
// A channel sends a change every time its numbers move, so the same rule
// holds here. A row is made again only when its `shape` changes.

/**
 * @param {HTMLElement} box   where the rows live, in order
 * @param {Map} rows          id -> {node, update, shape}, kept by the caller
 * @param {object[]} items
 * @param {(item) => string} idOf
 * @param {(item) => {node, update, shape?}} make
 * @param {(item) => string} [shapeOf]
 */
export function keyed(box, rows, items, idOf, make, shapeOf = () => "") {
  const wanted = new Set(items.map(idOf));
  for (const [id, row] of rows) {
    if (wanted.has(id)) continue;
    row.node.remove();
    rows.delete(id);
  }
  let before = box.firstChild;
  for (const item of items) {
    const id = idOf(item);
    let row = rows.get(id);
    if (!row || row.shape !== shapeOf(item)) {
      const made = make(item);
      made.shape = shapeOf(item);
      if (row) row.node.replaceWith(made.node);
      row = made;
      rows.set(id, row);
    }
    row.update(item);
    if (row.node !== before) box.insertBefore(row.node, before);
    before = row.node.nextSibling;
  }
}

/** Set a property only when it differs, so a quiet update touches nothing. */
export function write(node, key, value) {
  if (node[key] !== value) node[key] = value;
}

/** Copy text, and say so on the button for a moment. */
export async function copy(button, text) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // An address over plain http has no clipboard API. Select the text in a
    // throwaway box and use the old command, which still works there.
    const box = Object.assign(document.createElement("textarea"), { value: text });
    document.body.appendChild(box);
    box.select();
    document.execCommand("copy");
    box.remove();
  }
  const was = button.dataset.label || button.textContent;
  button.dataset.label = was;
  button.textContent = "Copied";
  button.classList.add("copied");
  setTimeout(() => {
    button.textContent = was;
    button.classList.remove("copied");
  }, 1400);
}
