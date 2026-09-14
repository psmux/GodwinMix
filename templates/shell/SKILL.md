---
name: {{name}}-{{kind}}
description: Adds a {{name}} source to GodwinMix that draws a test pattern at the canvas size. Use when an operator asks for colour bars, a test picture, or something to check the programme chain with before a show.
---

# {{name}}

Add it with `source.add {id: "bars", type: "{{name}}/{{kind}}", params: {pattern: "smpte"}}`.

It produces video only. Take it with `program.take {source: "bars"}` once
`event/source.state` reports it live.

Change the pattern with `source.set {id: "bars", params: {pattern: "ball"}}`.
The plugin answers `restart_required` for that, so the supervisor rebuilds it
behind a freeze frame and the programme does not gap.
