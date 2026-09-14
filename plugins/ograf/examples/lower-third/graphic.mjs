// A lower third, as an OGraf graphic.
//
// Everything a graphic has to be is here and it is not much: a custom element
// exported as the default, with `load`, `updateAction`, `playAction` and
// `stopAction` on it, each answering `{ status: 0 }`. No host code, no mixer
// API, no framework. The same file plays in the GodwinMix graphics host, in
// OGraf's own preview harness, and in any other host that reads the
// specification.
//
// Copy this directory to start a graphic of your own. `docs/how-to/
// make-a-graphic.md` walks through it.

/** How long the slide takes, both ways. Matches `duration` in playAction. */
const DURATION = 400;

export default class LowerThird extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: "open" });
    this.shadowRoot.innerHTML = `
      <style>
        :host { display: block; width: 100%; height: 100%; }
        .bar {
          position: absolute;
          inset: auto 0 0 0;
          display: flex;
          flex-direction: column;
          justify-content: center;
          gap: 0.15em;
          padding: 0.6em 1em;
          box-sizing: border-box;
          background: var(--bar, #1f6f4f);
          color: #fff;
          font: 400 4.2vh/1.15 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
          opacity: 0;
          transform: translateX(-110%);
          transition: transform ${DURATION}ms cubic-bezier(.22,.61,.36,1), opacity ${DURATION}ms;
        }
        .bar.right { transform: translateX(110%); }
        .bar.on { opacity: 1; transform: translateX(0); }
        .name { font-weight: 600; letter-spacing: 0.01em; }
        .title { font-size: 0.62em; opacity: 0.86; }
        /* A graphic that has nothing to say stays off rather than showing an
           empty bar, which is the mistake every first template makes. */
        .bar.empty { display: none; }
      </style>
      <div class="bar" part="bar">
        <div class="name" part="name"></div>
        <div class="title" part="title"></div>
      </div>`;
    this.bar = this.shadowRoot.querySelector(".bar");
  }

  /** Put the words in and get ready. Nothing is shown until playAction. */
  async load(params) {
    this.write(params && params.data);
    return { status: 0 };
  }

  /** New words while it is on air, with no animation. */
  async updateAction(params) {
    this.write(params && params.data);
    return { status: 0 };
  }

  /** Step 1 is on. Any step past the last one is off again. */
  async playAction(params) {
    const step = (params && params.step) || 1;
    if (params && params.data) this.write(params.data);
    this.bar.classList.toggle("on", step === 1);
    return { status: 0, duration: DURATION };
  }

  /** Off. */
  async stopAction() {
    this.bar.classList.remove("on");
    return { status: 0, duration: DURATION };
  }

  /** Whatever this host calls when the item is taken off the canvas. */
  async dispose() {
    this.bar.classList.remove("on");
    return { status: 0 };
  }

  write(data) {
    const values = data || {};
    const name = values.name || "";
    const title = values.title || "";
    this.shadowRoot.querySelector(".name").textContent = name;
    this.shadowRoot.querySelector(".title").textContent = title;
    this.shadowRoot.querySelector(".title").style.display = title ? "" : "none";
    this.bar.style.setProperty("--bar", values.colour || "#1f6f4f");
    this.bar.classList.toggle("right", values.side === "right");
    this.bar.classList.toggle("empty", !name && !title);
  }
}
