# Start the mixer for the first time

You have the `godwinmix` binary and nothing else. This page gets you from there
to the page in a browser.

## Run it in an empty folder

```sh
mkdir show && cd show
godwinmix
```

There is no `godwinmix.toml` here, so the mixer writes one, says where, and
starts on it:

```
2026-09-22T17:06:56.172Z  info godwinmix: first run: no config was here, so a starting one was written path="/home/you/show/godwinmix.toml"
...
GodwinMix is running. Open http://127.0.0.1:8080/ in a browser.
```

Open that address. The page asks what you are streaming, with a tile for a
church service, a classroom, a streamer, starting empty, and importing from
OBS. Pick one.

A preset tile sets the mixer up and opens Nearly there, a short checklist. Each
row has a button that does the step: Add key opens the same form the Outputs
panel uses, Add opens the source picker on the right category, Install fetches
a missing plugin, Show me brings a panel forward, and Put on air takes the
source. A row ticks itself once the mixer says it is done, for instance when
the output has its key. Steps only a person can do, like pressing the red
button when the service starts, are plain sentences.

Some settings a preset writes only take effect when the mixer starts, the
encoder bitrate for one. A bar across the top of the window names them. When
something will start the mixer again (the desktop app, the systemd unit, the
container) it has a Restart now button; on a mixer started by hand in a
terminal it says so instead, and the settings apply the next time you start
it. [Restart the mixer from the page](restart-the-mixer.md) has the detail.

Import from OBS opens a drop zone: drop the scene collection file on it, or
press it to choose the file. [Import your scenes from OBS](import-from-obs.md)
says what comes across.

## What the file it wrote holds

It is the example config with a short note at the top. Every setting is there
with its default and a comment. The sample cameras and outputs are there too,
commented out, so the mixer starts with nothing that tries to connect to an
RTMP server you do not have. That is also why the page offers its tiles: they
only show on a mixer with no sources.

Sources and outputs you add from the page are kept beside it in
`godwinmix.runtime.toml`, so you never have to edit the file to keep them.

## When it is not written

* A file is already there. It is read as it is and never touched.
* You passed `--config` with a path whose folder does not exist. The mixer stops
  and says the file could not be written there, because a missing folder is
  more likely a typo than a first run. Make the folder, or fix the path.
* The folder is not writable. Same message; start it from a folder you own.

## Print the file without starting anything

```sh
godwinmix --example-config
```

prints the same text the first run writes, for a service that keeps its config
in `/etc/godwinmix/`. See [Run it on a headless server](headless-server.md).

## The desktop app

The desktop app starts its own mixer with a config in the application data
folder, and that mixer writes the same first run file. The note at the top of
it says which two lines the app overrules: the control address and the token.
