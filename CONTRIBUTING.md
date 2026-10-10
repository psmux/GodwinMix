# Contributing to GodwinMix

Patches are welcome. So are bug reports with a config file and a log attached,
which are often worth more than a patch.

## Before you start

By contributing you agree to the [CLA](CLA.md) and to the
[code of conduct](CODE_OF_CONDUCT.md). The project is licensed under the
Apache License 2.0, in [LICENSE](LICENSE).

If the change is larger than a bug fix, open an issue first and say what you
intend to do. It saves you writing code that goes the wrong way.

## Build

GodwinMix is Rust on top of GStreamer. You need a Rust toolchain (the version
in `rust-version` in the root `Cargo.toml` or newer) and GStreamer with its
development headers.

```sh
brew install gstreamer                  # macOS
# Linux: your distro's gstreamer plus plugins base, good, bad, ugly, libav, rs
# Windows: the MSVC installer from gstreamer.freedesktop.org, see below

cargo build
cargo build --release
```

### On Windows

GStreamer 1.28 for Windows is one installer, `gstreamer-1.0-msvc-x86_64-1.28.x.exe`,
and the full type brings the development files cargo needs. From an
administrator PowerShell:

```powershell
.\gstreamer-1.0-msvc-x86_64-1.28.6.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TYPE=full
```

It sets `GSTREAMER_1_0_ROOT_MSVC_X86_64` and nothing else. Add the other two
yourself, once:

```powershell
$g = "C:\Program Files\gstreamer\1.0\msvc_x86_64"
[Environment]::SetEnvironmentVariable("PKG_CONFIG_PATH", "$g\lib\pkgconfig", "User")
[Environment]::SetEnvironmentVariable("Path", [Environment]::GetEnvironmentVariable("Path", "User") + ";$g\bin", "User")
```

Git for Windows has to be installed too, because a first party plugin builds
with `sh build` and the mixer runs that with Git's `sh`. Building the desktop
installer also wants `rustup component add llvm-tools` and `cargo install
tauri-cli --version "^2.0"`; the steps are the ones in
`.github/workflows/release.yml`, and they run in that order on a desk as well.

Two things on a Windows machine are not the mixer's fault and will fail tests
that look like they are. A VPN client can drop UDP on the loopback address,
which takes the SRT, RIST, RTP and ONVIF tests with it; send a datagram to
`127.0.0.1` from Python to see whether yours does. And Intel's Quick Sync
runtime on some Arc GPUs corrupts the heap of the process that loads it, about
one load in four (exit code `0xc0000374`), which can end a test binary after
every test in it has passed.

Windows Firewall asks once for every program that listens on all interfaces,
and each cargo build of a new test binary is a new program to it. Loopback is
never filtered, so the prompts do not break a test, but they pile up. From an
elevated PowerShell, `pwsh -File dev/windows-firewall.ps1` allows this
workspace's builds from the local subnet only, and `-Remove` takes the rules
out again. Run it again after a build adds test binaries.

### The crates

The repository is a Cargo workspace. `cargo build` at the root builds all of
it; `cargo run -- --probe` still reaches the `godwinmix` binary.

| Crate | What belongs there |
|---|---|
| `crates/godwinmix-protocol` | The wire contract: request, response, event and status types, error codes, scopes, the method table's descriptions, and the generators behind `protocol.json`, `protocol.md` and `openapi.json`. serde and schemars underneath it, nothing else. No GStreamer, no server. |
| `crates/godwinmix-core` | The mixing engine as a library: the mixer thread, sources, outputs, the plugin traits and built in kinds, the catalogue, scenes, multiview, snapshots, and the instrumentation. No HTTP server, no MCP, no command line. `cargo add godwinmix-core` is meant to give somebody this and nothing else. |
| `crates/godwinmix-host` | The tier 2 plugin host: manifest, handshake, transports, loader. Empty so far. |
| `crates/godwinmix` | The binaries. The axum control plane, `/rpc`, `/api/v1`, the WebSocket layer, the method handlers, `gmx ctl`, the MCP server, the web UI, bench, and the clap definition. |

[The crate map](docs/explanation/architecture.md) says what does not belong in
each and where to put a new module. Two rules that catch most mistakes: if a
module needs axum, clap or reqwest it is not engine code, and a `use` line
names the crate that owns the item rather than a re-export through a nearer
one.

`codecs.toml`, `layouts/`, `presets/`, `schemas/` and `ui/` stay at the
repository root and the crates reach out to them with a relative path.

Two more crates live beside the mixer and are outside the workspace, because
they have their own lockfiles and their own heavy dependency trees. They are
built separately:

* `browser/` is the web page renderer. It links against CEF. You do not need
  to build it by hand: the mixer builds it the first time a web page is added
  (see below). `browser/dev/install-cef-dist.sh` is for putting a CEF with
  H.264 and AAC in place of the default download.
* `tauri-app/` is the desktop shell. It needs the Tauri prerequisites for your
  platform.

To see what the mixer will actually use on your machine:

```sh
cargo run --release -- --probe
cargo run --release -- --example-config > godwinmix.toml
cargo run --release -- --config godwinmix.toml
```

### What the first run sets up by itself

`cargo run --release -- --config godwinmix.example.toml` is the whole install.
Nothing else has to be built or installed by hand for the page to offer
everything it shows. Two kinds of piece are set up the first time somebody
needs them, in the background, while the programme carries on:

* **Web pages.** The first web page (one in the config counts) starts the
  renderer's setup: the CEF download (about 124 MB, resumed if the connection
  drops), `cargo build --release` in `browser/`, and on macOS the app bundle.
  The page shows "Setting up web pages. This happens once and takes a few
  minutes", and the web page appears by itself when it is ready. On an Apple
  silicon Mac with the crates already in the cargo cache it took 2 minutes:
  28 seconds to download, about 10 to unpack, 35 to build and 45 to bundle.
  It needs CMake and Ninja (`brew install cmake ninja`); without them the page
  says so and shows that command with a copy button.
* **Cameras, screens, microphones, channels and the other first party
  plugins.** Opening the category in Add a source, or adding a source that
  needs one, installs it from `plugins/` in about a minute. No Install button.

The build output goes to `~/.godwinmix/logs/setup-web.log` (or under
`GODWINMIX_HOME`), not to the terminal. If a step does not finish, the page
says so in a sentence and offers Try again, which carries on from where it
stopped. [docs/reference/setup.md](docs/reference/setup.md) has the details.

## Test

```sh
cargo test --workspace
```

The tests build real GStreamer pipelines, so GStreamer has to be installed for
them to pass. They run in a few seconds and they are expected to be green on
Linux, macOS and Windows before you open a pull request. The GitHub Actions
workflow in `.github/workflows/build.yml` builds and tests on all three.

Also run `cargo clippy --workspace --all-targets`. The count is zero and CI
runs it with `-D warnings`, so a new one fails the build. If you disagree with
a lint, add an `#[allow]` with a comment saying why rather than leaving a
warning in the log for the next person to scroll past.

The tree is not `cargo fmt` clean and running it would reformat most of the
repository, so do not. Match the style of the file you are editing: wider lines
than rustfmt's default, and a struct literal on one line when it fits.

There is a local test rig under `dev/harness/` (mediamtx and a synthetic
camera) for anything that needs a real RTMP endpoint. `dev/harness/up.sh`
downloads mediamtx on first run, starts the RTMP server, a test page server and
a mixer on a test config, and `dev/harness/down.sh` stops all of it. A
`gmx harness up` that does the same thing without a shell script is planned.

A change to the engine should also keep `cargo build -p godwinmix-core
--example embed` working. That example is what `cargo add godwinmix-core` buys,
and `crates/godwinmix-core/tests/embed.rs` runs it.

A change to the mixing, the source lifecycle or the output path should come
with a test that fails without it. A change that cannot be tested without
hardware should say in the pull request what you ran it against and what you
watched to decide it worked.

## Commit messages

Read `git log` and match it. One sentence, present tense, saying what changed
and why it changed, written for a person reading the history a year from now.
No prefixes, no ticket numbers, no conventional commit tags. Lines from the
history:

```
A superimposed source leaked its media decoder, and one counter per side of its compositor
Nothing keyed by a source id outlives the source
UI: a slim bar marks the output instead of a heavy red frame
```

A `UI:` prefix is used for changes confined to `ui/`. If the sentence needs
more, put the detail in the body after a blank line. Sign off your commits with
`git commit -s`, which is how you accept the CLA.

## Writing rules

These apply to code comments, documentation, commit messages, issues and pull
request descriptions.

* No em dashes, no en dashes, and no double hyphens standing in for them. Use a
  comma, a full stop, a colon, or parentheses. Write ranges as "1 to 3 seconds",
  not "1-3 seconds". If a sentence wants a dramatic pause, rewrite it.
* Plain declarative sentences of uneven length. Some short. Some longer, with a
  clause an editor would cut.
* No slogan openers, no taglines, no "not X but Y" constructions, no bolded
  lead ins on every bullet, no sets of three used for rhythm.
* Avoid the words: leverage, seamless, robust, cutting edge, delve, elevate,
  unlock, harness, landscape, journey, transformative, testament. The one
  exception is "harness" as a noun: the conformance harness is a thing with a
  name, and `gmx plugin test` runs it. As a verb it is still out.
* Comments say why, not what. The code already says what.

## Pull requests

Keep a pull request to one change. If you fixed a bug and also cleaned up the
formatting of a file, that is two pull requests.

Say in the description what the change does, what you tested, and what you know
is still missing. If it changes behaviour anyone depends on (an HTTP route, a
config key, an environment variable, a file name), say that first.

## The friction log

Every time somebody gets stuck on the path from "I want to use this" to "it
works", it goes in [`docs/friction-log.md`](docs/friction-log.md). Not in a
private note, not in a ticket that gets closed. In that file, where the next
person can read it.

The rule: if you got stuck, write it down, even if you worked it out yourself
thirty seconds later. Especially then. A thirty second confusion that happens
to everybody costs more in total than an hour long problem that happens to one
person. Three things make an entry useful: what you were trying to do, what you
saw, and what you expected instead. Do not polish it.

That applies to AI agents as well as people. An agent that had to read the
source to find out what a command does has hit friction, and the fix is the
same fix.

Every entry is triaged to one of three outcomes: fixed (the error message, the
default or the page changed), documented (the behaviour is right and now the
page says so), or accepted (the fix is expensive or waiting on something else,
and the entry says which). An entry is never closed with "works as intended"
and nothing else. If three people trip over the same intention, the intention
is the problem.

The measured time from nothing to a working plugin is the number this project
keeps. The friction log is where the things that make that number worse are
recorded.

## Response times

First response on an issue or a pull request is targeted at under 48 hours.
That first response might be a question or a "this needs a week", but you
should not be left wondering whether anyone read it. If three days pass with
nothing, assume it was missed and say so on the thread.

This is a commitment rather than an aspiration, and the reason is in the
evidence: a study of 111,094 pull requests found that the wait for a first
reply is what decides whether a newcomer becomes a contributor. A "good first
issue" label on its own does not work; a label plus a fast reply does.

## Listing a plugin

Anybody may write a plugin and publish it wherever they like. Nothing here
stops that, and a plugin installed from a path or a repository works exactly as
well as a listed one. What a listing buys is what the project does on the
author's behalf: five platform builds, the conformance harness on each, sigstore
signatures the installer checks, a badge, and a row on the public compatibility
dashboard.

The condition of being listed is this policy. The bot checks what can be
checked mechanically; the rest is checked by a person reading the pull request,
and by anybody who reads the source afterwards.

**No obfuscation.** The source that is published is the source that is built.
Minification for a browser panel is fine and is not obfuscation. Packed
binaries, encoded payloads pulled at runtime, and generated code with the
generator withheld are not, and a listing carrying any of them is refused
without a second look.

**No telemetry without disclosure.** A plugin that reports anything anywhere
says so in its README, in its listing description, and in a sentence the
operator sees before it runs. Anonymous counts are still telemetry. Opt out is
better than opt in only for the project's own installation count, and a plugin
is not the project.

**Declared network use.** Say which hosts the plugin talks to and what for. A
source that fetches from one address, an output that pushes to one CDN, and a
service that polls one API are all easy to declare. A plugin that talks to an
address chosen at runtime says that, and says who chooses it.

**Declared secrets.** Say what the plugin reads and from where: an environment
variable, a file, the operator's config, a keychain. A plugin that reads a
stream key declares it, and an operator can then decide before it is running
rather than after.

Alongside those, a listing has to be a plugin somebody can use: a licence, a
version, a release, and a `SKILL.md` so an agent knows when to reach for it.
[The quality scale](docs/reference/quality-scale.md) has the full list per
tier, and [the index format](docs/reference/index-format.md) has every field of
an entry.

### How a listing is handled

Open a pull request against `index.json` in the index repository. The bot runs
`index/bot/validate.py`, which checks the document, the tier requirements and
the harness result and names the check that failed when it refuses. A pull
request that passes is looked at by a person, and the 48 hour first response
above covers listing pull requests as well as issues.

You can run the same check yourself before opening it:

    python3 index/bot/validate.py --entry <your plugin>

### When a listing breaks the policy

It is delisted, and the pull request that delists it names the clause and links
to the evidence. An author who fixes it can relist, and the history stays in
the repository because a quiet delisting teaches nobody anything.

A plugin that is abandoned rather than in breach is not delisted. It falls to
the tier its checks still support, which for a plugin whose harness run stopped
passing is `custom`, and the compatibility dashboard shows why.

### AI written pull requests

This project advertises that an agent can operate and extend it, so it will get
agent written pull requests, and some of them will be the kind that made tldraw
pause external contributions in January 2026. The answer here is the harness
rather than a ban: a pull request that does not pass `gmx plugin test` or
`cargo test` is closed by a bot with the failing check named, and one that does
pass gets read like any other. Say in the description what wrote it. Nobody
minds; the check is the same either way.

## Security

Do not open a public issue for a security problem. Open a
[private security advisory](https://github.com/psmux/GodwinMix/security/advisories/new)
with what you found and how to reproduce it.
