# Examples

Working programs, not snippets. Each one runs.

| File | What it is |
|---|---|
| [zero-dep-source.py](zero-dep-source.py) | a whole GodwinMix source plugin in under 200 lines of Python that imports nothing outside the standard library. It draws colour bars at the canvas caps and writes them as raw I420 in a streamable Matroska stream on stdout |
| [test-zero-dep.sh](test-zero-dep.sh) | drives that plugin with a hand written handshake and reads the first cluster back. python3 and a shell, nothing else |
| [ai-director.py](ai-director.py) | an agent that watches the state document and decides what goes on programme. Read it alongside `docs/agents.md` |
| [tkinter-panel.py](tkinter-panel.py) | a control surface in 150 lines of Tkinter: the source list, take buttons, an MJPEG preview in a Label and a settings form built from a plugin's schema |
| [tkinter-designer.py](tkinter-designer.py) | a scene designer in a few hundred lines of Tkinter, over the same three kits the web composer uses: the preview behind the canvas, handles from each plugin's `designer` block, and an inspector generated from its schema. No HTML anywhere |

`zero-dep-source.py` exists to keep the protocol honest. If it ever needs a
dependency, the protocol has drifted and the protocol is wrong, not the example.

```sh
./examples/test-zero-dep.sh
```

The Rust equivalent, on the SDK, is
`cargo run -p godwinmix-sdk --example colour-bars`. Five language templates that
do the same thing are under [templates/](../templates/).
