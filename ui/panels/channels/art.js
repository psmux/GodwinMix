// The empty state: a camera and a phone pointing at the mixer, drawn in the
// theme's own colours, with one line under it and the button. The picture
// says what a channel is faster than a paragraph would.

import { el } from "../../shell/dom.js";

const PICTURE = `<svg viewBox="0 0 360 150" role="img" aria-label="A camera and a phone sending to the mixer">
<g class="a-dev">
  <rect x="14" y="44" width="72" height="46" rx="9"/>
  <rect x="26" y="35" width="22" height="12" rx="3"/>
  <path class="a-leg" d="M50 90 36 128M50 90 64 128M50 90v38"/>
</g>
<circle class="a-lens" cx="50" cy="67" r="15"/>
<circle class="a-glass" cx="50" cy="67" r="7.5"/>
<circle class="a-live a-pulse" cx="76" cy="53" r="3.2"/>
<g class="a-dev"><rect x="98" y="62" width="28" height="50" rx="6"/></g>
<rect class="a-glass" x="102" y="68" width="20" height="34" rx="2.5"/>
<circle class="a-live a-pulse" cx="112" cy="85" r="3"/>
<path class="a-flow" d="M136 60 H222"/>
<path class="a-flow a-flow2" d="M136 86 H222"/>
<path class="a-head" d="M216 54l8 6-8 6M216 80l8 6-8 6"/>
<text class="a-label" x="179" y="48" text-anchor="middle">rtmp://</text>
<g class="a-mix"><rect x="232" y="30" width="116" height="86" rx="11"/></g>
<rect class="a-screen" x="244" y="41" width="92" height="44" rx="5"/>
<path class="a-wave" d="M252 70l10-9 9 7 12-13 9 10 10-6 9 8 9-4"/>
<circle class="a-live a-pulse" cx="327" cy="50" r="3.4"/>
<rect class="a-knob" x="248" y="94" width="30" height="5" rx="2.5"/>
<rect class="a-knob" x="284" y="94" width="18" height="5" rx="2.5"/>
<circle class="a-knob" cx="325" cy="100" r="7"/>
</svg>`;

export function emptyArt(onAdd) {
  const art = el("div.chn-art", { html: PICTURE });
  return el("div.chn-empty", {}, [
    art,
    el("h3", { text: "Let an encoder in" }),
    el("p", { text: "OBS, a phone or a hardware encoder publishes to a channel. It can go on air and on to YouTube, Facebook and the rest." }),
    el("button.btn.primary.chn-big", { text: "Add RTMP Channel", onclick: onAdd }),
  ]);
}
