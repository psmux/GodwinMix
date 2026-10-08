// The phone deck's drawings: one stroke weight, a 24 unit box, currentColor,
// so a theme recolours them with no work and they stay sharp at any density.

const PATHS = {
  live: '<circle cx="12" cy="12" r="2.6"/><path d="M8.2 8.2a5.4 5.4 0 0 0 0 7.6M15.8 8.2a5.4 5.4 0 0 1 0 7.6M5.2 5.2a9.6 9.6 0 0 0 0 13.6M18.8 5.2a9.6 9.6 0 0 1 0 13.6"/>',
  sources: '<rect x="2.5" y="6" width="13" height="12" rx="2.6"/><path d="M15.5 10.4 21.5 7v10l-6-3.4z"/>',
  audio: '<path d="M6 4v5M6 13v7M12 4v11M12 19v1M18 4v2M18 10v10"/><circle cx="6" cy="11" r="2"/><circle cx="12" cy="17" r="2"/><circle cx="18" cy="8" r="2"/>',
  outputs: '<path d="M12 15V3.5M7.5 8 12 3.5 16.5 8"/><path d="M4 13.5V18a3 3 0 0 0 3 3h10a3 3 0 0 0 3-3v-4.5"/>',
  more: '<rect x="4" y="4" width="6.5" height="6.5" rx="1.6"/><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.6"/><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.6"/><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.6"/>',
  back: '<path d="M14.5 5.5 8 12l6.5 6.5"/>',
  graphics: '<path d="M12 3.5 14 9l5.5 2-5.5 2-2 5.5-2-5.5-5.5-2L10 9z"/>',
  media: '<rect x="3" y="4.5" width="18" height="15" rx="2.6"/><path d="M3 9h18M3 15h18M8 4.5v15M16 4.5v15"/>',
  alerts: '<path d="M6 16.5V11a6 6 0 0 1 12 0v5.5l1.5 2h-15z"/><path d="M10 20.5a2.2 2.2 0 0 0 4 0"/>',
  channels: '<rect x="3" y="6.5" width="18" height="13" rx="2.6"/><path d="m8.5 2.5 3.5 4 3.5-4"/>',
  plugin: '<path d="M9.5 4h5v3a1.8 1.8 0 1 0 3.5 0V4H20v6h-3a1.8 1.8 0 1 0 0 3.5h3V20h-6v-3a1.8 1.8 0 1 0-3.5 0v3H4v-6h3a1.8 1.8 0 1 0 0-3.5H4V4z"/>',
  studio: '<rect x="2.5" y="6" width="8" height="7" rx="1.6"/><rect x="13.5" y="6" width="8" height="7" rx="1.6"/><path d="M8 18h8"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M12 2.8v2.6M12 18.6v2.6M2.8 12h2.6M18.6 12h2.6M5.5 5.5l1.8 1.8M16.7 16.7l1.8 1.8M5.5 18.5l1.8-1.8M16.7 7.3l1.8-1.8"/>',
  mixer: '<path d="M4 7h10M18 7h2M4 17h2M10 17h10"/><circle cx="16" cy="7" r="2"/><circle cx="8" cy="17" r="2"/>',
  routing: '<circle cx="5.5" cy="6" r="2"/><circle cx="5.5" cy="18" r="2"/><circle cx="18.5" cy="12" r="2"/><path d="M7.5 6c5 0 4 6 9 6M7.5 18c5 0 4-6 9-6"/>',
  wall: '<rect x="3" y="4" width="8" height="7" rx="1.6"/><rect x="13" y="4" width="8" height="7" rx="1.6"/><rect x="3" y="13" width="8" height="7" rx="1.6"/><rect x="13" y="13" width="8" height="7" rx="1.6"/>',
  black: '<circle cx="12" cy="12" r="8.5"/><path d="M6 6l12 12"/>',
  device: '<rect x="7" y="2.5" width="10" height="19" rx="2.6"/><path d="M11 18.5h2"/>',
};

/** An inline SVG for one of the names above; a plugin piece for anything else. */
export function icon(name) {
  const span = document.createElement("span");
  span.className = "phone-icon";
  span.setAttribute("aria-hidden", "true");
  span.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${PATHS[name] || PATHS.plugin}</svg>`;
  return span;
}
