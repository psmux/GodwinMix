# Write a panel

A panel is a piece of the web UI. One JavaScript file, no build step, no npm, no
framework. If you can write a function that puts text in a `<div>` you can write
a panel, and you should have one on screen in about ten minutes.

## The shortest one that works

Put this at `~/.godwinmix/plugins/hello/ui/panel.js`:

```js
class HelloPanel extends HTMLElement {
  static get panel() {
    return { id: "hello/sources", title: "Sources", slots: ["sidebar"] };
  }

  setClient(client) {
    this.client = client;
  }

  connectedCallback() {
    this.innerHTML = '<div class="pad"><strong>Sources</strong><ul></ul></div>';
    const list = this.querySelector("ul");
    // onRender fires once now, and again at every flush. Never per event: a
    // batch of twenty changes paints the panel once.
    this.off = this.client.onRender((state) => {
      list.innerHTML = state.sources
        .map((s) => `<li>${s.name} <span class="dim">${s.state}</span></li>`)
        .join("");
    });
  }

  disconnectedCallback() {
    if (this.off) this.off();
  }
}

customElements.define("gmx-hello-sources", HelloPanel);
window.godwinmixPanels.push(HelloPanel);
```

Reload the page. It is in the sidebar, listing every source and its state, and
it updates as the mixer changes. Nothing else had to be edited: not the shell,
not a manifest, not the config. The core lists what is on disk at
`/plugins/index.json` and the shell loads it.

## Where the file goes

```
~/.godwinmix/plugins/<name>/ui/panel.js      a trusted panel
~/.godwinmix/plugins/<name>/ui/panel.html    a sandboxed panel
~/.godwinmix/plugins/<name>/ui/panel.json    optional, names it and picks a slot
```

Change the directory with `plugins_dir` under `[control]` in the config file.
Everything under `ui/` is served at `/plugins/<name>/ui/`, so icons, a stylesheet
and extra modules all work by relative path.

`panel.json`, when you want one:

```json
{
  "id": "ndi/senders",
  "title": "NDI senders",
  "slots": ["sidebar"],
  "tier": "trusted",
  "height": 260
}
```

Without it, `panel.js` is trusted and `panel.html` is sandboxed.

## The contract

| Member | When it runs | What it is for |
|---|---|---|
| `static get panel()` | at registration | `{id, title, slots}`. `id` is a slug like `ndi/senders`; `slots` is where it may sit |
| `setClient(client)` | before the element is connected | the client: `state`, `call`, `onRender`, `want` |
| `setConfig(config)` | before the element is connected | the panel's own saved config, `{}` at first |
| `connectedCallback()` | on mount | build the DOM, subscribe |
| `disconnectedCallback()` | on removal | unsubscribe, release anything expensive |

`customElements.define` is optional. A class that only pushes itself is defined
for you under a tag derived from its id (`ndi/senders` becomes `gmx-ndi-senders`).

Slots: `header`, `main`, `sidebar`, `strip`, `footer`, `modal`. Name the ones
that make sense; the shell puts the panel in the first one that exists.

## What the client gives you

```js
client.state                              // the whole status document, now
client.onRender(fn)                        // fn(state) now, and at every flush
client.call("program.take", {source: id})  // any method in the protocol
client.on("alert", fn)                     // alert, meters, position, event, open, close
client.capabilities                        // what this core can actually do
client.snapshotUrl("cam1", 320)            // a still, at a width you name
```

Errors from `call` are one shape: `{code, message, data}`, and the message
already names the current state and the next step, so show it as it came.

```js
try {
  await client.call("program.take", { source: "cam9" });
} catch (e) {
  // "source 'cam9' is not live (state: connecting). Live sources: cam1, cam2."
  console.log(e.code, e.message, e.retryable);
}
```

## Asking for pictures

Anything expensive is opt in, and the core does no work for a stream nobody
asked for. Ask with `want`, and give it back:

```js
connectedCallback() {
  this.want = this.client.want("multiview", { fps: 8, width: 640 });
}
disconnectedCallback() {
  this.want.release();
}
```

The client adds up what every panel wants and subscribes once, at the widest
width and the highest rate anyone needs. When the last holder releases, the
subscription goes away and the core stops encoding. Ask for the width your
elements are actually drawn at, multiplied by the device pixel ratio, and no
more: `sheetWidthFor(cssWidth, cols)` in `ui/client/frames.js` does that sum.

Other keys: `meters`, `tally`, `positions`, `preview`, `telemetry`.

## The two tiers

A trusted panel is a custom element in the page. It gets the real client object
and can touch the DOM. First party panels and plugins the operator has marked
trusted run this way.

A sandboxed panel is an HTML file in an `<iframe sandbox="allow-scripts">` with
no same origin access. It cannot reach the page, the token or storage. It talks
the same JSON-RPC over `postMessage`, proxied by the shell. This is the default
for anything the operator has not vouched for, and a panel written for one tier
is nearly the same in the other:

```html
<!doctype html>
<meta charset="utf-8">
<body>
<script type="module">
import { connectPanel } from "/client/sandbox-client.js";

const client = await connectPanel();
client.onRender((state) => {
  document.body.textContent = state.sources.map((s) => s.name).join(", ");
});
client.resize(120);            // tell the shell how tall you want to be
await client.subscribe({ meters: true });
</script>
```

Same method names, same events, same state document. The only differences: you
call `connectPanel()` instead of being handed a client, you ask for `ext` streams
with `client.subscribe({...})` rather than `want`, and `document` is your own
frame's, not the page's.

Write for the sandbox unless you need the DOM. It is the tier an operator will
accept from a stranger.

## Styling

Use the shell's classes and its custom properties, and your panel matches
whatever theme the operator picked:

```html
<div class="pad col">
  <div class="row"><strong class="grow">Senders</strong><button class="btn">Scan</button></div>
  <div class="row"><span class="dot live"></span><span class="ellipsis grow">STUDIO (Cam 1)</span></div>
</div>
```

`pad`, `row`, `col`, `grow`, `dim`, `faint`, `sm`, `num`, `ellipsis`, `btn`,
`btn primary`, `dot`, `dot live`, `pill`, `tile`, `empty`. The full set is at
the top of `ui/themes/base.css`, and the properties a theme can change are in
`ui/themes/README.md`. Hard coding `#1a1a1a` works and looks wrong the moment
someone switches to the light theme.

## Adding a command

Anything you register appears in the command palette on Ctrl+K and can be bound
to a key:

```js
import { register } from "/shell/commands.js";

connectedCallback() {
  this.offCommand = register({
    id: "ndi.scan",
    title: "Scan for NDI senders",
    group: "NDI",
    run: () => this.client.call("plugin.tool.call", { name: "scan" }),
  });
}
disconnectedCallback() {
  this.offCommand();
}
```

Trusted tier only: a sandboxed panel has no access to the shell's modules, which
is the point of the sandbox.

## Settings forms

Never hand write a form for a plugin's settings. Ask for the schema and render
it:

```js
// Imported when the form is wanted, not when the panel loads. The reader is
// eleven kilobytes and a panel that never shows a form should never fetch it.
const { SchemaForm } = await import("/client/schema-form.js");

const described = await this.client.call("plugin.describe", { id: "ndi" });
const form = new SchemaForm(described.schemas["ndi/source"], {});
this.appendChild(form.el);
// later
if (form.validate()) await this.client.call("source.set", form.read());
```

It reads JSON Schema draft 2020-12: objects, scalars, enums, arrays, `if`/`then`
for conditional fields, `format: "secret"` for a password field that is never
echoed back, `x-gmx-unit` for a unit suffix, and `x-gmx-group` for a collapsible
section of advanced fields.

## Loading your own code late

The shell fetches what a page needs to be usable and nothing else: the composer,
the command palette, the sandbox bridge, the schema form reader and the adapter
for cores with no `/rpc` all arrive on the first thing that asks for them. A
test in `crates/godwinmix/src/ui.rs` walks the import graph from `boot.js` and
measures what is left, so a static import added to a hot path shows up as a
failure rather than as a slower first paint.

Do the same in a panel of any size. A `import()` inside the handler that needs
it costs nothing at load and one request when it is used:

```js
this.button.onclick = async () => {
  const { openBigThing } = await import("./big-thing.js");
  openBigThing(this.client);
};
```

## Designing scenes from a panel

A panel that draws on the canvas or edits a scene should use the designer kits
rather than its own arithmetic: the record mirror, drag prediction, handles from
each plugin's `designer` block, snapping and the schema renderer are all in
`ui/kits/`, documented in
[designer-kits.md](../reference/designer-kits.md), and the same three kits ship
in `@godwinmix/client` and in the Python library so your panel and a Tkinter
surface behave the same way.

```js
import { SceneClient } from "/kits/protocol/index.js";

this.scenes = new SceneClient(this.client, { undo: shell.undo });
await this.scenes.start();
this.scenes.onChange(() => this.render());
```

## Testing it

Open `/test/` for the shell's own suite, and add yours the same way: a page, a
module, assertions that print to the console. There is no runner to install.
The page is served only when the core was started with `GMX_UI_DEV=1`, which is
what keeps its assertions out of what a volunteer's browser downloads.

`dev/ui-tests.sh` runs that page against a core it starts itself, in headless
Chrome, and prints every assertion. Its live suite drives the real panels over
`/rpc`, so it is also the quickest way to see your own panel working against a
mixer with a scene in it.

While you are working, point the core at your checkout so you do not rebuild to
see a change:

```toml
[control]
ui_dir = "/home/you/GodwinMix/ui"
plugins_dir = "/home/you/plugins"
```

## What to avoid

* HTML5 drag and drop (`dragstart`, `drop`) for anything inside the page.
  WebView2 on Windows swallows it when the window also accepts files dropped
  from the desktop, which the mixer's window does. Use pointer events. The
  shell's `ui/shell/pointer.js` is the model.
* Polling. Subscribe and render at flush. A panel that fetches every second is a
  panel that makes the mixer slower on a Pi.
* Painting per event. Meters arrive ten times a second per source; `onRender`
  fires once a batch for a reason.
* Holding an `ext` stream after `disconnectedCallback`. That is a picture the
  core encodes and nobody looks at.
