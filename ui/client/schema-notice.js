// `x-gmx-notice` on a settings schema: a line the form shows above its
// fields, with an optional link. NDI's licence asks for a link to ndi.video
// and its trademark line wherever NDI is picked, and the schema is the one
// thing every surface that picks a plugin's kind already renders.
//
//   "x-gmx-notice": { "text": "NDI® is a registered trademark of Vizrt NDI AB.",
//                     "link": "https://ndi.video/", "label": "ndi.video" }

/** The notice element for a schema, or null when it has none. */
export function schemaNotice(schema) {
  const n = schema && schema["x-gmx-notice"];
  if (!n || (typeof n.text !== "string" && typeof n.link !== "string")) return null;
  const p = document.createElement("p");
  p.className = "hint notice";
  if (n.text) p.append(String(n.text));
  // Only a web address: a schema comes from a plugin, and a javascript: link
  // in it would run in the operator's page.
  if (typeof n.link === "string" && /^https?:\/\//.test(n.link)) {
    const a = document.createElement("a");
    a.href = n.link;
    a.target = "_blank";
    a.rel = "noopener";
    a.textContent = n.label ? String(n.label) : n.link;
    if (n.text) p.append(" ");
    p.append(a);
  }
  return p;
}
