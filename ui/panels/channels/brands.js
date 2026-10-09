// The platform marks, drawn here as inline SVG in each brand's colour so a
// tile reads at a glance. Simplified glyphs on a rounded square, one request
// fewer than an image and nothing fetched from anyone's CDN.
//
// Every mark is a 24 unit square. `tile` is the square's fill, `glyph` what
// sits on it. Nothing in here takes input from anywhere, so it is safe as
// markup.

const R = `<rect width="24" height="24" rx="5.5"`;

const MARKS = {
  youtube: `${R} fill="#ff0000"/><rect x="3.6" y="6.4" width="16.8" height="11.2" rx="3.2" fill="#fff"/><path d="M10.2 9.3v5.4l4.7-2.7z" fill="#ff0000"/>`,
  facebook: `${R} fill="#0866ff"/><path d="M13.3 21v-7h2.4l.4-2.9h-2.8V9.3c0-.8.3-1.4 1.4-1.4h1.5V5.3c-.3 0-1.1-.1-2.1-.1-2.1 0-3.6 1.3-3.6 3.7v2.2H8.1V14h2.4v7z" fill="#fff"/>`,
  twitch: `${R} fill="#9146ff"/><path fill="#fff" fill-rule="evenodd" d="M7 4.5 5 7.8v10.4h3.6V20h2l1.8-1.8h2.9L19 14.5v-10zm1.4 1.4h9.2v7.9l-2.3 2.3h-3l-1.9 1.9v-1.9H8.4z"/><path d="M11.2 8.4h1.4v4.1h-1.4zM14.9 8.4h1.4v4.1h-1.4z" fill="#fff"/>`,
  instagram: `<defs><linearGradient id="gmx-ig" x1="0" y1="1" x2="1" y2="0"><stop offset="0" stop-color="#feda75"/><stop offset=".3" stop-color="#fa7e1e"/><stop offset=".6" stop-color="#d62976"/><stop offset="1" stop-color="#4f5bd5"/></linearGradient></defs>${R} fill="url(#gmx-ig)"/><rect x="5.5" y="5.5" width="13" height="13" rx="3.8" fill="none" stroke="#fff" stroke-width="1.7"/><circle cx="12" cy="12" r="3.1" fill="none" stroke="#fff" stroke-width="1.7"/><circle cx="15.7" cy="8.3" r=".95" fill="#fff"/>`,
  kick: `${R} fill="#53fc18"/><path d="M6.5 5h3.6v4.3h1.7V7.6h1.7V5h3.7v4.5h-1.8v1.7h-1.7v1.6h1.7v1.7h1.8V19h-3.7v-2.6h-1.7v-1.7h-1.7V19H6.5z" fill="#0b0e0f"/>`,
  linkedin: `${R} fill="#0a66c2"/><circle cx="7.6" cy="7.4" r="1.75" fill="#fff"/><path d="M6.1 10h3v8.4h-3zM11 10h2.9v1.3c.5-.9 1.6-1.5 3-1.5 2.4 0 3.1 1.6 3.1 3.9v4.7h-3v-4.1c0-1-.3-1.8-1.3-1.8-1.1 0-1.7.8-1.7 1.9v4h-3z" fill="#fff"/>`,
  x: `${R} fill="#000"/><rect x=".5" y=".5" width="23" height="23" rx="5" fill="none" stroke="#fff" stroke-opacity=".18"/><path d="M5.6 5.5h4l3.2 4.5 3.9-4.5h1.5l-4.7 5.4 5 7.6h-4l-3.4-4.8-4.2 4.8H5.4l5.2-5.9zm2 1 7.9 11h1.6l-7.8-11z" fill="#fff"/>`,
  tiktok: `${R} fill="#000"/><rect x=".5" y=".5" width="23" height="23" rx="5" fill="none" stroke="#fff" stroke-opacity=".18"/><g transform="translate(-.7 -.5)"><path d="${note()}" fill="#25f4ee"/></g><g transform="translate(.7 .5)"><path d="${note()}" fill="#fe2c55"/></g><path d="${note()}" fill="#fff"/>`,
  custom: `${R} fill="#5b6b8c"/><g fill="none" stroke="#fff" stroke-width="1.6" stroke-linecap="round"><path d="M12 13v6.5M9.4 19.5h5.2"/><circle cx="12" cy="11" r="1.6" fill="#fff" stroke="none"/><path d="M8.6 7.6a4.8 4.8 0 0 0 0 6.8M15.4 7.6a4.8 4.8 0 0 1 0 6.8M6.2 5.2a8.2 8.2 0 0 0 0 11.6M17.8 5.2a8.2 8.2 0 0 1 0 11.6"/></g>`,
  // The two that stay on this machine: a record button, and a play button
  // with the waves of something being shared.
  file: `${R} fill="#3a3f4b"/><circle cx="12" cy="12" r="6.6" fill="none" stroke="#fff" stroke-width="1.6"/><circle cx="12" cy="12" r="4.2" fill="#ff4d4d"/>`,
  hls: `${R} fill="#2f6fde"/><path d="M8.6 7.4v9.2l7.4-4.6z" fill="#fff"/><path d="M5.6 7.8a6.4 6.4 0 0 0 0 8.4M18.4 7.8a6.4 6.4 0 0 1 0 8.4" fill="none" stroke="#fff" stroke-width="1.5" stroke-linecap="round"/>`,
  srt: `${R} fill="#14a38b"/><path d="M4.5 9h11.5l-2.4-2.4M19.5 15H8l2.4 2.4" fill="none" stroke="#fff" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/><circle cx="19" cy="9" r="1.4" fill="#fff"/><circle cx="5" cy="15" r="1.4" fill="#fff"/>`,
};

function note() {
  return "M13.4 4.5h2.5c.2 1.8 1.4 3.1 3.3 3.3v2.5c-1.2 0-2.4-.4-3.3-1v5.2a4.6 4.6 0 1 1-4.6-4.6h.4v2.6a2.1 2.1 0 1 0 1.7 2z";
}

/** The mark for a platform id, as an SVG element, or the custom one. */
export function brandMark(id, size = 40) {
  const holder = document.createElement("span");
  holder.className = "brand";
  holder.innerHTML = `<svg viewBox="0 0 24 24" width="${size}" height="${size}" aria-hidden="true">${MARKS[id] || MARKS.custom}</svg>`;
  return holder;
}

export const BRANDS = Object.keys(MARKS);
