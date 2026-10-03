# Install a plugin

A plugin adds a source, an output, a filter, a background service or a device
finder to a running mixer. It is a separate process, so installing one cannot
break the programme and removing one takes everything it added with it.

This page takes about five minutes.

## Install it

`gmx plugin add` takes seven forms. Reach for the first one:

    gmx plugin add camera                  a plugin that ships with the mixer
    gmx plugin add ndi                     a name a marketplace knows
    gmx plugin add psmux/gmx-ndi           a GitHub release, @1.2.0 to pin
    gmx plugin add https://x/y.git         a git clone, built here
    gmx plugin add cargo:gmx-ndi           a crate
    gmx plugin add npm:@x/gmx-chat         an npm package
    gmx plugin add pypi:gmx-director       a PyPI package
    gmx plugin add ./my-plugin             a directory you are working in

A bare name is first looked for among the plugins that ship with this mixer,
on its own disk, with no network. These are the places, in order:

    <folder of the godwinmix binary>/../share/godwinmix/plugins/<name>
    <folder of the godwinmix binary>/plugins/<name>
    <checkout>/plugins/<name>

The first is an install under a prefix (`/usr/local/bin/godwinmix` and
`/usr/local/share/godwinmix/plugins/camera`). The second is everything in one
folder, the way a Windows install or an unpacked archive is laid out. The third
is a mixer built from a source checkout and run from its `target` folder: the
checkout is the first folder up to four levels above the binary that has both
a `Cargo.toml` and a `plugins` folder. A plugin there is taken when its binary
for this machine is built, or when its manifest has a `[build]` section, which
the install then runs. This is what the Install buttons in the source picker
use, so the camera, screen and ingest plugins install on a mixer with no
marketplace set up. What is installed this way is recorded the way a folder
install is, as custom and unreviewed.

A name found in none of those is looked up in the marketplaces this mixer
knows, which is why a bare name is the form to teach: it does not change when
an author moves their repository. Add the community index once and names
start working:

    gmx marketplace add psmux/godwinmix-plugins
    gmx plugin search ndi

A directory works from a clean checkout, with nothing built first:

    gmx plugin add ./plugins/camera

If the binary the manifest names is not in the directory, the install runs the
manifest's `[build]` command where the directory is, waits for it, and then
installs what it produced. A build that fails stops the install and prints the
command, the exit status and what the build said. The second install of the
same directory is quick, because the binary is there by then and nothing
rebuilds. See [the manifest reference](../reference/plugin-manifest.md).

For a release, the asset for your platform is chosen by the platform triple in
its name, its signature is checked against the bytes that arrived, and its
`api` level is checked against what this core speaks. Only then is anything
copied.

The mixer puts it in its plugins directory, reads the manifest, and registers
everything the plugin declares. It answers with what it added:

    installed bars v0.1.0
      provides bars/source

    Add one with:  gmx ctl source add <id> --type bars/source

Nothing restarts. The programme keeps going while this happens.

## Use it

The `type` id from that listing is what goes in `type` when you add a source:

    gmx ctl source add cam1 --type bars/source
    gmx ctl status

Within a few seconds `gmx ctl status` shows `cam1` as live and you can take it
to programme:

    gmx ctl take cam1

If the plugin declared a URI scheme, a bare address works too and picks the
plugin by rank, the way `rtmp://` already picks the RTMP source:

    gmx ctl source add feed srt://203.0.113.10:9000

The scheme only chooses the plugin. Whether the address is enough on its own is
that plugin's business: `srt/source` reads it, while `ndi/source` wants its
sender in `name` or `address`, so an NDI source is named with `--type` and
given its settings.

## See what it costs

Every plugin's price is next to its name:

    gmx plugin list

    PLUGIN           VERSION   STATE    TRUST                PROVIDES
    bars             0.1.0     on       custom, unreviewed   bars/source
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

There is no `gmx` subcommand for this yet, so it is two HTTP calls:

    curl -s localhost:8080/api/v1/plugins/bars/settings
    curl -s -X POST localhost:8080/api/v1/plugins/bars/settings \
      -H 'content-type: application/json' -d '{"settings":{"<key>":"<value>"}}'

`gmx plugin describe bars` prints the schema, so it says what the keys are.
Only the keys you name change.

Settings are written in your config under `[plugins.<name>]` and the core never
reads inside them: the table is handed to the plugin named and nothing else
sees it. A change made over the API is written back to the config file so it
survives a restart.

Only the plugin's own `[plugins.<name>]` table is touched when settings are
written back, and it is edited in place: every comment in the file stays,
including the ones inside that table beside keys that are still there.

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

To try a plugin without touching that folder, set `GODWINMIX_HOME` to a
scratch folder for both the mixer and `gmx`; plugins then go in its
`plugins/`, and so does everything else per user (see
[configuration](../reference/configuration.md#where-per-user-files-live)).

Change it with `plugins_dir` under `[control]` in your config. Anything under
that directory is read at startup, and a manifest that does not validate is
listed by `gmx plugin list` with the reason rather than dropped in silence:

    PLUGIN           VERSION   STATE    TRUST                PROVIDES
    broken           0.1.0     broken   custom, unreviewed
        gmx-plugin.toml has 1 problems:
          provides[0].settings: 'schemas/source.json' does not exist.

The same directory serves panels. `<plugins_dir>/<name>/<version>/ui/` is
published at `/plugins/<name>/ui/`, which is how a plugin adds a page to the web
UI. See [write-a-panel.md](write-a-panel.md).

## When something goes wrong

A name found nowhere is refused with the places it was looked for. With no
marketplace set up, the sentence names the first folder a shipped plugin would
be in, and the error's `data` carries `source`, `marketplaces` and `looked_in`,
the full list, for a page to show.

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

## Keep it up to date

    gmx plugin update ndi

That fetches a newer build from wherever this plugin was installed from,
installs it beside the one that is running, and gives it ten seconds to say
hello. A build that does not start is rolled back and you keep the version that
worked, so a bad release from an author costs you an error message rather than
a show.

## What the trust column means

`signed` is a signature cosign verified. `signed, digest only` is a signature
whose digest matches the bytes that arrived, on a machine with no cosign
installed to check who made it. `ships with GodwinMix` is a first party plugin
that came with the mixer, from the installer or the checkout it was built in,
installed by its name; a locked down mixer accepts it like a signed one.
`custom, unreviewed` is everything else nobody signed, which includes every
path install and everything from cargo, npm and PyPI.

`gmx plugin describe <name>` prints the sentence behind the label. A mixer that
should install signed releases only sets this in its config:

    [plugins]
    allow_unsigned = false

[Trust and signing](../explanation/trust-and-signing.md) says what each level
proves and what none of them do.

## Next

* [Test a plugin](test-a-plugin.md), before you install it anywhere that matters.
* [Publish a plugin](publish-a-plugin.md), when it is somebody else's turn to
  install yours.
* [The plugin lifecycle](../reference/plugin-lifecycle.md): the states, the
  budgets, and the environment a plugin process gets.
* [Write a source plugin](write-a-source-plugin.md).
