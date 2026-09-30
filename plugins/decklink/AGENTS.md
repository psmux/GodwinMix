# AGENTS.md

For a coding agent changing this plugin.

`decklink/source` (`src/handlers.rs`, `src/card.rs`) and `decklink/devices`.
The capture runs through `plugins/capture-common` like the camera plugin: the
chain ends at the canvas contract and `wiring` adds the transport the core
negotiated.

* Discovery must never fail on a machine without a card: it answers an empty
  list and says why in health.
* Each missing piece (element, driver, card) gets its own sentence.
* Nothing here has run against a real card. Say so in anything you write
  about it until someone has.

```sh
cargo test -p gmx-decklink
cargo clippy -p gmx-decklink --all-targets
```
