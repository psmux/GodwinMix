// The Get more tab: search the marketplaces, install from one, install from a
// source you were sent, and manage the marketplaces themselves.
//
// The order matters. A fresh mixer knows no marketplaces at all, so a search
// here answers nothing and the honest first thing on screen is the offer to
// add the one the project runs. That offer is a button, because the operator
// this whole panel exists for does not have a terminal open.

import { el, clear } from "../../shell/dom.js";
import { confirmModal } from "../../shell/modal.js";
import { runTask, showProblem } from "./task.js";

/** The one sentence under a search result that says what it is. */
export function resultNote(result) {
  const bits = [];
  if (result.version) bits.push("v" + result.version);
  if (result.tier) bits.push(result.tier === "custom" ? "custom, unreviewed" : result.tier);
  if ((result.kinds || []).length) bits.push(result.kinds.join(", "));
  if (result.marketplace) bits.push("from " + result.marketplace);
  return bits.join(" · ");
}

export function marketTab(client) {
  const node = el("div.col");
  const problems = el("div");
  const markets = el("div.col");
  const results = el("div.col");
  const term = el("input", { type: "search", placeholder: "camera, ndi, chat…", "aria-label": "Search plugins" });
  const searchButton = el("button.btn.primary", { text: "Search" });

  const pasted = el("input", { type: "text", placeholder: "psmux/gmx-ndi, a git URL, or a directory", "aria-label": "Install from a source" });
  const pastedSay = el("div.plugin-say.sm.dim");
  const pastedButton = el("button.btn", { text: "Install" });

  node.append(
    problems,
    el("div.row", {}, [term, searchButton]),
    results,
    el("details.plugin-more", {}, [
      el("summary", { text: "Install from a source you were sent" }),
      el("div.col.sm", {}, [
        el("p.sm.dim", {
          text: "Anything gmx plugin add takes: owner/repo, a git URL, cargo:, npm:, pypi:, or a directory on this machine.",
          style: { margin: "0" },
        }),
        el("div.row", {}, [pasted, pastedButton]),
        pastedSay,
      ]),
    ]),
    el("details.plugin-more", { open: true }, [el("summary", { text: "Marketplaces" }), markets])
  );

  async function search() {
    problems.textContent = "";
    searchButton.disabled = true;
    let answer;
    try {
      answer = await client.call("plugin.search", { term: term.value.trim() });
    } catch (e) {
      searchButton.disabled = false;
      showProblem(problems, e);
      return;
    }
    searchButton.disabled = false;
    drawResults(answer || {});
  }

  function drawResults(answer) {
    clear(results);
    const found = answer.results || [];
    if (!found.length) {
      results.appendChild(
        el("p.faint.sm", {
          text: (answer.marketplaces || []).length
            ? "Nothing matched. Try a shorter word, or leave the box empty to list everything."
            : "This mixer knows no marketplaces yet, so there is nothing to search. Add one below and search again.",
        })
      );
      return;
    }
    for (const result of found) results.appendChild(resultRow(result));
  }

  function resultRow(result) {
    const say = el("div.plugin-say.sm.dim");
    const button = el("button.btn.primary.sm", { text: result.installed ? "Installed" : "Install", disabled: !!result.installed });
    button.onclick = async () => {
      button.disabled = true;
      try {
        await runTask(client, "plugin.add", { source: result.source }, (words) => {
          say.textContent = words;
        });
      } catch (e) {
        button.disabled = false;
        say.textContent = "";
        showProblem(problems, e);
        return;
      }
      button.textContent = "Installed";
      say.textContent = "Installed. It is on the Installed tab, and its types are in the add picker.";
    };
    return el("div.plugin-row", { "data-result": result.name }, [
      el("div.row", {}, [
        el("strong.ellipsis", { text: result.name }),
        el("span.grow"),
        button,
      ]),
      result.description ? el("div.sm.dim", { text: result.description }) : null,
      el("div.sm.faint.ellipsis", { text: resultNote(result) }),
      say,
    ]);
  }

  pastedButton.onclick = async () => {
    const source = pasted.value.trim();
    if (!source) {
      pastedSay.textContent = "Paste a source first: owner/repo, a URL, or a directory on this machine.";
      return;
    }
    problems.textContent = "";
    pastedButton.disabled = true;
    try {
      await runTask(client, "plugin.add", { source }, (words) => {
        pastedSay.textContent = words;
      });
    } catch (e) {
      pastedButton.disabled = false;
      pastedSay.textContent = "";
      showProblem(problems, e);
      return;
    }
    pastedButton.disabled = false;
    pasted.value = "";
    pastedSay.textContent = "Installed. It is on the Installed tab.";
  };

  searchButton.onclick = () => search();
  term.onkeydown = (e) => {
    if (e.key === "Enter") search();
  };

  async function refresh() {
    let answer;
    try {
      answer = await client.call("marketplace.list", {});
    } catch (e) {
      showProblem(problems, e);
      return;
    }
    drawMarkets(answer || {});
    await search();
  }

  function drawMarkets(answer) {
    clear(markets);
    markets.appendChild(marketList(client, answer, refresh, problems));
  }

  return { node, refresh, destroy() {} };
}

/**
 * The marketplaces themselves: what is added, what the project runs, and a box
 * for anybody else's.
 *
 * Split out of the tab because it is the part a first run sees first, and it
 * has to read as a complete thought on its own.
 */
function marketList(client, answer, refresh, problems) {
  const box = el("div.col");
  const added = answer.marketplaces || [];
  if (!added.length) {
    box.appendChild(
      el("p.sm.dim", {
        text: "None yet. Without one, a plugin can only be installed from a source written out in full, and searching by name finds nothing.",
        style: { margin: "0" },
      })
    );
  }
  for (const market of added) box.appendChild(marketRow(client, market, refresh, problems));

  for (const offer of answer.recommended || []) {
    if (offer.added) continue;
    box.appendChild(offerRow(client, offer, refresh, problems));
  }
  if ((answer.only || []).length) {
    box.appendChild(
      el("p.sm.faint", {
        text: `This mixer is pinned to ${answer.only.join(", ")} by its config, so nothing else is read even if it is added here.`,
      })
    );
  }

  const input = el("input", { type: "text", placeholder: "owner/repo, a URL, or a directory", "aria-label": "Add a marketplace" });
  const say = el("div.plugin-say.sm.dim");
  const button = el("button.btn", { text: "Add" });
  button.onclick = async () => {
    const source = input.value.trim();
    if (!source) {
      say.textContent = "Write owner/repo, a URL, or a path to a directory with a marketplace document in it.";
      return;
    }
    button.disabled = true;
    try {
      await runTask(client, "marketplace.add", { source }, (words) => {
        say.textContent = words;
      });
    } catch (e) {
      button.disabled = false;
      say.textContent = "";
      showProblem(problems, e);
      return;
    }
    input.value = "";
    await refresh();
  };
  box.append(el("div.row", {}, [input, button]), say);
  return box;
}

function marketRow(client, market, refresh, problems) {
  const say = el("div.plugin-say.sm.dim");
  const remove = el("button.btn.sm", { text: "Remove" });
  remove.onclick = async () => {
    const sure = await confirmModal(
      `Forget ${market.name}? Plugins installed from it stay installed and keep working, but a bare name it was the only source of will stop resolving.`,
      "Forget it"
    );
    if (!sure) return;
    remove.disabled = true;
    try {
      await client.call("marketplace.remove", { id: market.name });
    } catch (e) {
      remove.disabled = false;
      showProblem(problems, e);
      return;
    }
    await refresh();
  };
  return el("div.plugin-row", { "data-marketplace": market.name }, [
    el("div.row", {}, [
      el("strong.ellipsis", { text: market.title || market.name }),
      el("span.num.faint", { text: `${market.plugins} plugin(s)` }),
      el("span.grow"),
      remove,
    ]),
    el("div.sm.faint.ellipsis", { text: market.source }),
    market.problem ? el("div.sm", { text: market.problem }) : null,
    market.consulted === false
      ? el("div.sm", { text: "Not consulted: this mixer's config pins it to a different list." })
      : null,
    say,
  ]);
}

/** One of the marketplaces the project runs, behind one button. */
function offerRow(client, offer, refresh, problems) {
  const say = el("div.plugin-say.sm.dim");
  const button = el("button.btn" + (offer.first_party ? ".primary" : ""), { text: `Add ${offer.title}` });
  button.onclick = async () => {
    button.disabled = true;
    try {
      await runTask(client, "marketplace.add", { source: offer.source }, (words) => {
        say.textContent = words;
      });
    } catch (e) {
      button.disabled = false;
      say.textContent = "";
      showProblem(problems, e);
      return;
    }
    await refresh();
  };
  return el("div.plugin-row.plugin-offer", { "data-offer": offer.name }, [
    el("div.row", {}, [el("strong.ellipsis", { text: offer.title }), el("span.grow"), button]),
    el("div.sm.dim", { text: offer.description }),
    say,
  ]);
}
