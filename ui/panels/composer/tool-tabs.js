// The composer's tools on a phone: one group at a time.
//
// A desk shows Align, Space, Size and Structure side by side. A phone had
// them as four rows that each swiped sideways, so most buttons sat off the
// edge where nobody found them, and the four rows cost the picture its room.
// Here a row of four chips picks the group, and that group's buttons wrap
// under it, every one of them in sight. composer.css hides the chips on a
// wide screen, where every group shows as before.

import { el } from "../../shell/dom.js";

/**
 * @param {HTMLElement[]} groups each a `.composer-group`, its first child the
 *   group's name
 * @returns {HTMLElement} the chips, to go before the groups
 */
export function toolTabs(groups) {
  const chips = groups.map((group, i) =>
    el("button.composer-tool-tab", {
      type: "button",
      role: "tab",
      text: group.firstElementChild ? group.firstElementChild.textContent : String(i + 1),
      onclick: () => pick(i),
    })
  );
  function pick(n) {
    groups.forEach((g, i) => g.classList.toggle("on", i === n));
    chips.forEach((c, i) => c.setAttribute("aria-selected", String(i === n)));
  }
  pick(0);
  return el("div.composer-tool-tabs", { role: "tablist", "aria-label": "Tools" }, chips);
}
