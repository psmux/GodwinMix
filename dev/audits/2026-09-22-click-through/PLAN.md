# Click through, not edit a file: the plan

Three audits on 2026-09-22 (ui.md, core.md, setup.md in this folder) read
every string the web UI shows, every refusal the core sends a client, and
the path from download to streaming. The standard they were judged against:
the GUI never tells a person to edit a file, run a command or restart
something by hand. It does those things for them.

## What the three audits agree on

1. The welcome tiles apply a preset over the protocol, and then the "Nearly
   there" dialog prints the preset's steps verbatim: "Put your YouTube and
   Facebook stream keys into the two [[outputs]] blocks of godwinmix.toml"
   and "Run `gmx` and open http://localhost:8080". All six presets. The
   Outputs panel behind that dialog already has an Add key button.
2. No protocol method reads or writes the mixer's own configuration. Page
   Settings writes only to localStorage. This one gap is the cause of about
   half the findings: multiview off, snapshots off, the media folder, the
   recording folder, exec sources, the take guard, the nodes bridge, all of
   them answer "set X in the config and restart".
3. Nothing can restart the core. core.shutdown exists and exits cleanly,
   which under the shipped systemd unit (Restart=on-failure) stays down.
4. The OBS tile prints a terminal command although scene.import.obs exists.
5. preset.apply and plugin.settings.set rewrite the config file and strip
   every comment. preset.apply also reports keys as written when it kept
   them. core.doctor ignores --config.
6. `godwinmix` with no config file exits with an error, while the desktop
   shell already holds code that writes a good first config.
7. errorToast throws away the `data` object every error carries, so nothing
   a refusal says can become a button.

## Wave A: the foundation (core and protocol)

A1. config.get, config.set, config.reset. Dotted keys matching
    ConfigChange.key, an `applies` answer per key (live, restart, next
    source), secrets handled as plugin.settings.set does, written with
    toml_edit so comments survive. preset.apply and plugin.settings.set move
    onto the same writer. Fix still_pending counting kept keys. core.doctor
    honours --config. Size L.
A2. core.restart with a `supervised` answer, a --supervised flag set by the
    service unit, the compose file and the desktop sidecar launch,
    Restart=always in the unit, a restart_core command in the desktop shell.
    A missing config file is a first run, not an error: move
    first_run_config from tauri-app into the core. The media folder is
    created on first use. scene.import.obs takes `content` inline and an
    `add_sources` flag and adds the sources over source.add. Size M.

## Wave B: the GUI on top

B1. Preset steps become structured (what it does, what it targets) and the
    "Nearly there" dialog draws a checklist of buttons that open the Add key
    dialog, the picker, the plugin install. needs_restart becomes a bar with
    a Restart now button that calls core.restart when it can and says the
    honest thing when it cannot. The OBS tile becomes a drop zone. Size M.
B2. A Mixer Settings dialog, distinct from page Settings, schema driven on
    config.get and config.set: canvas, programme bitrates, control address
    and token, multiview and snapshot switches, media folder, recording
    folder with a folder picker (path.list), hardware, exec sources, plugin
    enable and disable. The multiview panel offers the switch when off. The
    Command tile offers the switch. The token dialog stops naming an
    environment variable. Size L.
B3. errorToast acts on `data`: a next step button, CONFIRM_REQUIRED handled,
    retryable retried. The core refusals that name TOML keys, gmx commands
    or a restart (snapshot, multiview, nodes, safety, plugin.add, the web
    source sidecar, the two restart alerts) carry a `data.action` the toast
    can offer, and their messages say what the person can press. CLI text
    keeps naming commands; that is the terminal. Size M.

## Left for later, with the reason

Node enrolment through the GUI (a second machine), the Screens permission
on macOS (the OS owns that dialog), and the desktop shell's sidecar staging
script. Each is real and none blocks a first time user on one machine.
