//! The wrapper page: what a browser source actually loads.
//!
//! It is the only HTML this host writes. Its job is to be the thing OGraf
//! assumes is there and does not specify: something that imports the graphic's
//! module, puts the custom element on the page, calls `load` with the words,
//! and then calls `updateAction`, `playAction` and `stopAction` when it is
//! told to. The graphic itself writes no host code, which is the whole point
//! of the specification.
//!
//! Two things the page does that are not in OGraf and are the difference
//! between a demo and something on air. It reads its state from the host
//! rather than from the URL, so a browser the supervisor restarted comes back
//! showing what it was showing. And it paints nothing of its own: the body is
//! transparent, and what is behind a graphic is whatever the mixer puts there.
//! See `docs/reference/graphics.md` for what that costs today.

use crate::catalogue::Graphic;

/// The page for one placement of one graphic.
pub fn wrapper(graphic: &Graphic, instance: &str) -> String {
    let type_id = graphic.type_id();
    let main = graphic.main();
    let name = crate::host::escape(
        graphic
            .manifest
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&type_id),
    );
    // Both go into JavaScript string literals, so they are JSON encoded rather
    // than pasted: an id with a quote in it would otherwise end the script.
    let instance_literal = serde_json::Value::from(instance).to_string();
    let defaults_literal = serde_json::Value::Object(graphic.defaults()).to_string();
    let type_literal = serde_json::Value::from(type_id.as_str()).to_string();
    let main_literal = serde_json::Value::from(main).to_string();
    format!(
        r#"<!doctype html>
<meta charset="utf-8">
<title>{name}</title>
<style>
  /* Nothing here paints. The mixer decides what is behind a graphic, and a
     page with a background of its own would cover it. */
  html, body {{ margin: 0; height: 100%; background: transparent; overflow: hidden; }}
  #graphic {{ position: absolute; inset: 0; }}
</style>
<div id="graphic"></div>
<script type="module">
const INSTANCE = {instance_literal};
const TYPE = {type_literal};
const MAIN = {main_literal};
const DEFAULTS = {defaults_literal};
const BASE = `/graphic/${{TYPE}}`;

const slot = document.getElementById("graphic");
let element = null;
let loaded = false;

/** Put the graphic's own custom element on the page, once. */
async function mount() {{
  if (element) return element;
  const module = await import(`${{BASE}}/${{MAIN}}`);
  const Element = module.default;
  // OGraf's own examples export the class and register it themselves. Both
  // are allowed here: whichever the template did, this ends up with a node.
  const tag = `ograf-${{TYPE.replace(/[^a-z0-9]+/gi, "-")}}`;
  if (Element && !customElements.get(tag)) customElements.define(tag, Element);
  element = document.createElement(customElements.get(tag) ? tag : "div");
  slot.appendChild(element);
  await customElements.whenDefined(tag).catch(() => {{}});
  return element;
}}

/** One action from the host, in OGraf's method names. */
async function act(verb, state) {{
  const node = await mount();
  const data = (state && state.values) || {{}};
  try {{
    if (verb === "load" || !loaded) {{
      if (node.load) await node.load({{ data }});
      loaded = true;
      if (verb === "load") return;
    }}
    if (verb === "update" && node.updateAction) await node.updateAction({{ data }});
    if (verb === "play" && node.playAction) await node.playAction({{ data, step: state.step }});
    if (verb === "stop" && node.stopAction) await node.stopAction({{ data }});
  }} catch (e) {{
    // A template that throws takes its own page down and not the other three,
    // which is why there is one page per placement. Say so where the browser
    // sidecar's log will carry it.
    console.error(`{{"graphic":"${{TYPE}}","instance":"${{INSTANCE}}","verb":"${{verb}}"}}`, e);
  }}
}}

/** What the page shows when the host has nothing loaded for it yet.
 *
 * The manifest's own defaults, with anything in `?values=` over the top and
 * `?play=1` to play it. That is what makes `gmx-ograf --serve` a preview a
 * template author can work against, and it means a page that comes up before
 * the core has loaded it shows the template rather than a blank rectangle. */
function preview() {{
  const query = new URLSearchParams(location.search);
  let given = {{}};
  try {{
    given = JSON.parse(query.get("values") || "{{}}");
  }} catch (e) {{
    console.error("?values= is not JSON", e);
  }}
  return {{
    values: Object.assign({{}}, DEFAULTS, given),
    step: 1,
    playing: query.get("play") === "1",
  }};
}}

/** Read the whole state, for a first load and after falling behind. */
async function resync() {{
  let state = preview();
  const answer = await fetch(`/state/${{encodeURIComponent(INSTANCE)}}`).catch(() => null);
  if (answer && answer.ok) state = await answer.json();
  await act("load", state);
  if (state.playing) await act("play", state);
}}

const stream = new EventSource(`/events/${{encodeURIComponent(INSTANCE)}}`);
for (const verb of ["load", "update", "play", "stop"]) {{
  stream.addEventListener(verb, (e) => act(verb, JSON.parse(e.data)));
}}
stream.addEventListener("resync", () => resync());
// The stream sends the current state on connect, so this is only for the case
// where the socket is slow and the page would otherwise be blank meanwhile.
resync();
</script>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn graphic() -> Graphic {
        crate::catalogue::in_plugin(&PathBuf::from(env!("CARGO_MANIFEST_DIR")))
            .into_iter()
            .next()
            .expect("the example graphic")
    }

    #[test]
    fn the_page_calls_every_ograf_method_the_specification_names() {
        let page = wrapper(&graphic(), "graphic-x-1");
        for method in ["load", "updateAction", "playAction", "stopAction"] {
            assert!(page.contains(method), "the page never calls {method}");
        }
    }

    #[test]
    fn it_paints_nothing_of_its_own() {
        let page = wrapper(&graphic(), "graphic-x-1");
        assert!(page.contains("background: transparent"), "{page}");
    }

    #[test]
    fn an_instance_id_with_a_quote_in_it_cannot_end_the_script() {
        let page = wrapper(&graphic(), "a\"; alert(1); \"");
        assert!(
            !page.contains("alert(1)") || page.contains("\\\""),
            "{page}"
        );
        assert!(
            page.contains(r#"\"; alert(1); \""#),
            "the id is JSON encoded: {page}"
        );
    }

    #[test]
    fn it_shows_the_templates_defaults_when_the_host_has_nothing_for_it() {
        let page = wrapper(&graphic(), "graphic-x-1");
        assert!(
            page.contains("#1f6f4f"),
            "the manifest's defaults are in the page: {page}"
        );
        assert!(
            page.contains("?values="),
            "a template author can preview with values: {page}"
        );
    }

    #[test]
    fn it_fetches_its_state_rather_than_taking_it_from_the_url() {
        let page = wrapper(&graphic(), "graphic-x-1");
        assert!(
            page.contains("/state/"),
            "a restarted browser has to come back showing"
        );
        assert!(page.contains("EventSource"), "{page}");
    }
}
