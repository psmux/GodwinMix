# Setup on first use

Some of what the mixer offers lives outside its own program: the browser
renderer that draws web pages, and the first party plugins for cameras,
screens, microphones, channels and the rest. A packaged mixer carries all of
them. A mixer run from a source checkout (`cargo run --release -- --config
godwinmix.example.toml`) has their sources and sets each one up the first time
somebody needs it. This page is the contract for that.

## Pieces

| `piece` | `title` | What setting it up does |
|---|---|---|
| `web` | Web pages | downloads the web page engine (CEF), builds `browser/` and, on macOS, makes its app bundle |
| `camera`, `screen`, `audio-device`, `ingest`, `ndi`, `srt` and every other plugin under `plugins/` | Cameras, Screen capture, Microphones and audio, Channels, ... | installs the plugin from the copy shipped beside the mixer or in the checkout, or switches it on when it is installed and off |

## What starts a setup

* `source.add` of a web page, or of a source whose first party plugin is not
  installed. The add answers at once with the source in state `connecting`
  and the piece's status under `setup`; the mixer keeps the source and starts
  it by itself once the piece is ready.
* A source in the config file that waits on a piece, when the mixer starts.
* Opening a category in the Add a source picker whose plugin is missing.
* `setup.start`, which is what Try again calls.

One piece is set up once at a time, however many callers ask. Every step runs
as a child process or a background task, never on the mixer thread or a
streaming thread, so the programme carries on while it runs. The build's own
output goes to `<home>/logs/setup-<piece>.log` (`<home>` is
`GODWINMIX_HOME`, else `~/.godwinmix`), never to the operator's screen.

## Methods

| Method | Scope | Params | Answer |
|---|---|---|---|
| `setup.list` | read | none | every piece and where it stands, as `SetupStatus` |
| `setup.get` | read | `{piece}` | one `SetupStatus` |
| `setup.start` | operate | `{piece}` | the `SetupStatus` at once; progress follows as events |

An unknown piece is refused with -32004 and the known ones in `data.valid`.

## `SetupStatus`

| Field | Meaning |
|---|---|
| `piece` | `web`, or the plugin's name |
| `title` | what it gives a person, such as `Web pages` |
| `state` | `ready`, `missing` (not there; this mixer can set it up), `running`, `failed` (the last attempt did not finish) or `unavailable` (not there, and this mixer cannot set it up) |
| `message` | one or two plain sentences for a person. Never a program, file, setting or plugin name |
| `progress` | 0 to 1 while a download says how far it has got |
| `action` | the button that moves it on: `setup` (Try again), sometimes with a `command` to copy |
| `detail` | for a developer: where it looked, what it ran, the log's path |

## `event/setup.changed`

`{ "setup": SetupStatus }`, sent when a piece starts, moves a percent through
its download, changes step, becomes ready or stops. The web UI shows the
message as a note while it runs and replaces it with "ready" or with the
failure and its button.

## Web pages, step by step

1. The engine's index is fetched from `https://cef-builds.spotifycdn.com`
   (or `CEF_DOWNLOAD_URL`) and the minimal archive for the version
   `browser/Cargo.lock` pins is found in it.
2. The archive is downloaded to `<CEF_PATH>/<version>/<name>.part`. A dropped
   connection is retried with a growing pause, and each retry asks for the
   bytes from where the file stopped, so nothing already downloaded is fetched
   again. Six attempts in a row that gain nothing end it with "the download
   did not finish" and a Try again button, and the partial file is kept for
   that.
3. The archive is checked against the SHA-1 the index publishes, unpacked
   with `tar` into the layout the `cef-dll-sys` build expects, and marked with
   its `archive.json`. That build then finds it and downloads nothing.
4. `cargo build --release` runs in `browser/` with `CEF_PATH` set.
5. On macOS, `browser/dev/mac-bundle.sh` makes
   `browser/target/release/godwinmix-browser.app`, where the mixer looks.

`CEF_PATH` defaults to `~/.cache/gmx-cef`, the folder the scripts under
`browser/dev/` use, so a renderer built by hand and one built by the mixer
share one download. Each step that is already done is skipped, so Try again
carries on from where the last attempt stopped.

CMake and Ninja are needed for step 4. When either is missing, the failure
says so and its action carries the command that installs them on this
platform (`brew install cmake ninja` on macOS, `sudo apt install cmake
ninja-build` on Debian and Ubuntu, `sudo dnf install cmake ninja-build` on
Fedora, `winget install Kitware.CMake Ninja-build.Ninja` on Windows).

## Where the renderer is looked for

In this order, the first that exists winning. On macOS each place names the
app bundle and the program inside it.

```text
browser.sidecar                                      the operator's own
<exe dir>/godwinmix-browser[.exe]                    beside the mixer
<exe dir>/godwinmix-browser.app                      the same, on macOS
<exe dir>/browser/godwinmix-browser[.exe]            a packaged folder with its libraries
<exe dir>/../Resources/godwinmix-browser.app         inside a macOS app bundle
<exe dir>/../lib/godwinmix/browser/godwinmix-browser an install under a prefix
<checkout>/browser/target/release/...                what the checkout's own build makes
PATH
```

`crates/godwinmix-core/src/setup/web.rs` holds this list and its tests.
