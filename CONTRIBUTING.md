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
in `rust-version` in `Cargo.toml` or newer) and GStreamer with its development
headers.

```sh
brew install gstreamer                  # macOS
# Linux: your distro's gstreamer plus plugins base, good, bad, ugly, libav, rs
# Windows: the MSVC runtime and development MSIs from gstreamer.freedesktop.org

cargo build
cargo build --release
```

Two more crates live beside the mixer and are built separately:

* `browser/` is the web page sidecar. It links against CEF and needs the CEF
  distribution in place first (`browser/dev/install-cef-dist.sh`). Most changes
  do not touch it and you do not need to build it.
* `tauri-app/` is the desktop shell. It needs the Tauri prerequisites for your
  platform.

To see what the mixer will actually use on your machine:

```sh
cargo run --release -- --probe
cargo run --release -- --example-config > godwinmix.toml
cargo run --release -- --config godwinmix.toml
```

## Test

```sh
cargo test
```

The tests build real GStreamer pipelines, so GStreamer has to be installed for
them to pass. They run in a few seconds and they are expected to be green on
Linux, macOS and Windows before you open a pull request. The GitHub Actions
workflow in `.github/workflows/build.yml` builds and tests on all three.

Also run:

```sh
cargo fmt
cargo clippy --all-targets
```

There is a local test rig under `dev/harness/` (mediamtx and a synthetic
camera) for anything that needs a real RTMP endpoint. `dev/harness/up.sh`
starts it and `dev/harness/down.sh` stops it.

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
  unlock, harness, landscape, journey, transformative, testament.
* Comments say why, not what. The code already says what.

## Pull requests

Keep a pull request to one change. If you fixed a bug and also cleaned up the
formatting of a file, that is two pull requests.

Say in the description what the change does, what you tested, and what you know
is still missing. If it changes behaviour anyone depends on (an HTTP route, a
config key, an environment variable, a file name), say that first.

## Response times

First response on an issue or a pull request is targeted at under 48 hours.
That first response might be a question or a "this needs a week", but you
should not be left wondering whether anyone read it. If three days pass with
nothing, assume it was missed and say so on the thread.

## Security

Do not open a public issue for a security problem. Mail maintainers@example.invalid
with what you found and how to reproduce it.
