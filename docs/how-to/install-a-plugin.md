# Install a plugin

A plugin adds a source, an output, a filter, a background service or a device
finder to a running mixer. It is a separate process, so installing one cannot
break the programme and removing one takes everything it added with it.

This page takes about five minutes.

## Install it

Point `gmx plugin add` at the directory the plugin was built in, the one with
`gmx-plugin.toml` at its root:

    gmx plugin add ./my-plugin

The mixer copies it into its plugins directory, reads the manifest, and
registers everything the plugin declares. It answers with what it added:

    installed bars v0.1.0
      provides bars/source

    Add one with:  gmx source add <id> --type bars/source

Nothing restarts. The programme keeps going while this happens.

## Use it

The `type` id from that listing is what goes in `type` when you add a source:

    gmx source add cam1 --type bars/source
    gmx ctl status

Within a few seconds `gmx ctl status` shows `cam1` as live and you can take it
to programme:

    gmx take cam1

If the plugin declared a URI scheme, a bare address works too and picks the
plugin by rank, the way `rtmp://` already picks the RTMP source:

    gmx source add cam1 ndi://CAM\ 1\ \(Studio\)

## See what it costs

Every plugin's price is next to its name:

    gmx plugin list

    PLUGIN           VERSION   STATE    PROVIDES
    bars             0.1.0     on       bars/source
        cam1           running    cpu 6%    rss 48 MB   latency 0 ms   dropped 0     restarts 0

`gmx plugin stats` is the same numbers on their own, and `gmx plugin stats
--json` is the shape a dashboard wants. They are refreshed once a second.

This is the number to look at when a machine is struggling. A plugin using
sixty percent of a core on a Raspberry Pi is the answer to "why is the
programme stuttering", and it takes one command to find rather than an
afternoon.

## Keep a plugin inside a budget

If a plugin misbehaves, put a limit on it in your config:

    [plugins.bars]
    max_rss_mb = 512
    max_cpu_percent = 60
    on_over_budget = "restart"     # or "disable", or "alert"

A breach has to hold for three seconds before anything happens, because one
busy second while a plugin opens a file is not a plugin that is broken. When it
does hold, the core logs it, emits `event/plugin.state {state: "over-budget"}`
and acts on that one instance. It never restarts itself and the programme never
stops.

Set the numbers from what `gmx plugin stats` actually shows, not from a guess. A
limit below what a plugin honestly needs gives you a source that restarts every
few seconds, which is worse than the problem.

The default is `alert`: say so and carry on. A programme that keeps running is
the safe state.

## Change its settings

    gmx ctl ... plugin.settings.get     # or over HTTP: GET /api/v1/plugins/bars/settings

Settings are written in your config under `[plugins.<name>]` and the core never
reads inside them: the table is handed to the plugin named and nothing else
sees it. A change made over the API is written back to the config file so it
survives a restart.

One thing to know: writing settings back rewrites the config file from the
table that was parsed, so comments elsewhere in the file are lost. If you keep
your config in git and care about the comments, edit the file and restart
instead.

## Turn one off without removing it

    gmx plugin disable bars
    gmx plugin enable bars

Disabled means it contributes nothing: no process, no provide, no tool. The
directory stays where it is, so enabling it again needs no download.

## Update one

    gmx plugin update bars ./my-plugin

That reinstalls from the directory, keeping the settings. For a plugin you are
writing, `gmx plugin reload bars` is quicker: it reads the directory again in
place and swaps the running instances one at a time, with the freeze frame
covering each swap.

## Remove one

    gmx plugin remove bars

    removed bars
      unregistered 1 provide(s) and 0 tool(s)

Everything the plugin registered goes with it: its provides, its tools, its
panels, its hooks and its discovery matchers. The processes are stopped, then
asked to shut down, then their process groups are killed after eight seconds if
they have not gone. No process, no descriptor, no socket and no directory is
left behind, and there is a test that proves it.

A source still configured to use a type the plugin provided will say so, by
name, rather than failing silently.

## Where they live

    ~/.godwinmix/plugins/<name>/<version>/

Change it with `plugins_dir` under `[control]` in your config. Anything under
that directory is read at startup, and a manifest that does not validate is
listed by `gmx plugin list` with the reason rather than dropped in silence:

    PLUGIN           VERSION   STATE    PROVIDES
    broken           0.1.0     broken
        gmx-plugin.toml has 1 problems:
          provides[0].settings: 'schemas/source.json' does not exist.

The same directory serves panels. `<plugins_dir>/<name>/<version>/ui/` is
published at `/plugins/<name>/ui/`, which is how a plugin adds a page to the web
UI. See [write-a-panel.md](write-a-panel.md).

## When something goes wrong

`gmx plugin list` first: it says whether the plugin loaded, whether it is
enabled, and what each instance is doing.

A plugin writes JSON-RPC on its stderr and anything else it prints there goes to
the core's log tagged with the instance. So a Python traceback lands in the log
rather than breaking the protocol:

    gmx logs --instance cam1

If a plugin died of something it did not expect, the SDK leaves a crash report
in the plugin's directory and the core puts the path in
`event/plugin.state.detail`. `gmx support-bundle` collects those along with the
logs, the redacted config and every pipeline's graph.

With several plugins installed and one of them misbehaving, let the mixer find
it:

    gmx plugin bisect --check "gmx ctl status | grep -q live"

That disables half the plugins, runs the check, keeps the half that still fails,
and repeats. A bad plugin among thirty two takes at most six steps. Every plugin
is put back the way it was found, whatever happens.

## Next

* [Test a plugin](test-a-plugin.md), before you install it anywhere that matters.
* [The plugin lifecycle](../reference/plugin-lifecycle.md): the states, the
  budgets, and the environment a plugin process gets.
* [Write a source plugin](write-a-source-plugin.md).
