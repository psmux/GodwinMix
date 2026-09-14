// Themes: one <link> that gets its href swapped.
//
// A theme is one CSS file of custom properties. Adding one means copying a file
// into ui/themes/ and naming it here, or, for a plugin, serving it from
// /plugins/<name>/ui/theme.css and listing it in the manifest. There is no
// build step and no JavaScript in a theme.

const KEY = "gmx.theme";

export const THEMES = [
  { id: "dark", title: "Dark", href: "themes/dark.css" },
  { id: "light", title: "Light", href: "themes/light.css" },
  { id: "high-contrast", title: "High contrast", href: "themes/high-contrast.css" },
  { id: "system", title: "Follow the system", href: "themes/system.css" },
];

/** Themes a plugin contributed, added at runtime by the registry. */
const extra = [];

export function addTheme(theme) {
  if (!theme || !theme.id || !theme.href) return;
  if (THEMES.some((t) => t.id === theme.id) || extra.some((t) => t.id === theme.id)) return;
  extra.push(theme);
}

export function allThemes() {
  return THEMES.concat(extra);
}

export function current() {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved && allThemes().some((t) => t.id === saved)) return saved;
  } catch {
    /* a window with no storage always starts dark */
  }
  return "dark";
}

export function applyTheme(id) {
  const theme = allThemes().find((t) => t.id === id) || allThemes()[0];
  let link = document.getElementById("gmx-theme");
  if (!link) {
    link = document.createElement("link");
    link.id = "gmx-theme";
    link.rel = "stylesheet";
    document.head.appendChild(link);
  }
  if (link.getAttribute("href") !== theme.href) link.setAttribute("href", theme.href);
  document.documentElement.dataset.theme = theme.id;
  try {
    localStorage.setItem(KEY, theme.id);
  } catch {
    /* the choice lasts until reload, which is better than refusing it */
  }
  return theme.id;
}

/** Called once at boot, before the first paint, so nothing flashes. */
export function initTheme() {
  return applyTheme(current());
}
