// One row of a script as the screen draws it: `**bold**` and `*italic*`,
// which may run on from one row to the next, and nothing else. The mixer
// made the rows (prompter.rows), so every screen breaks them in the same
// places; this only decides how the words inside one look.

/** The row's words without their asterisks, for measuring. */
export function plain(text) {
  return text.replace(/\*/g, "");
}

/**
 * The row as a node, and the emphasis it leaves on for the next row.
 * @param {{text: string, marker?: boolean}} row
 * @param {{bold: boolean, italic: boolean}} on
 */
export function drawRow(row, on) {
  const node = document.createElement("div");
  node.className = row.marker ? "row marker" : "row";
  let bold = on.bold;
  let italic = on.italic;
  for (const piece of row.text.split(/(\*\*|\*)/)) {
    if (piece === "**") bold = !bold;
    else if (piece === "*") italic = !italic;
    else if (piece) node.appendChild(span(piece, bold, italic));
  }
  if (!node.firstChild) node.textContent = " ";
  // A marker's line ends its emphasis, so a stray asterisk in a heading
  // cannot run on into the whole script.
  return { node, on: row.marker ? { bold: false, italic: false } : { bold, italic } };
}

function span(text, bold, italic) {
  if (!bold && !italic) return document.createTextNode(text);
  const s = document.createElement(bold ? "strong" : "em");
  if (bold && italic) {
    const inner = document.createElement("em");
    inner.textContent = text;
    s.appendChild(inner);
  } else {
    s.textContent = text;
  }
  return s;
}
