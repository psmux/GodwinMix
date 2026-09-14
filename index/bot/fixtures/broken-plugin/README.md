# A deliberately broken plugin

Nothing here works, on purpose. It is a fixture for `bot/validate.py --self-test`.

The manifest parses. `[plugin]` is complete, the `[[provides]]` block is a
valid source, the settings schema and the SKILL.md beside it are real files.
The single fault is that `[run]` says the plugin starts with `main.py` and
there is no `main.py` in this directory.

That is the shape of the most common broken listing: a release whose archive
was packed without the entry point, or a manifest that still names a file
somebody renamed. A core that tried to run it would spawn nothing and time out
after five seconds, and the operator would see a source that never comes up.

So the bot catches it before the release is ever listed. `entry.manifest` reads
the manifest, resolves every path `[run]` names, and refuses the listing when
one of them is missing. `entry.harness-run` would catch it too by running
`gmx plugin test`, and that check needs a core on the runner, which the self
test does not have. The cheap check runs first for that reason.

Do not fix this directory. A green fixture here means the self test is testing
nothing.
