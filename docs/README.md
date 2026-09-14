# GodwinMix documentation

Four kinds of page, kept apart on purpose. A tutorial takes you from nothing to
a working thing and never stops to explain. A how to guide answers one
question you already have. Reference tells you what a thing is. Explanation
says why it is like that. Mixing them makes a page that serves nobody, which is
the shape most project documentation ends up in.

Where a feature does not exist yet, the page says so in a sentence and names
the command that will do it. Nothing here shows output from a command that
cannot be run today.

## Tutorials

Start here if you have never run it.

* [Your first stream in five minutes, with Docker](tutorials/first-stream-docker.md)
* [Your first stream with the desktop app](tutorials/first-stream-desktop.md)
* [Your first plugin](tutorials/your-first-plugin.md) (planned; the page says what is missing)

## How to

* [Run it on a headless server](how-to/headless-server.md)
* [Put it behind a reverse proxy with TLS](how-to/reverse-proxy.md)
* [Run it on a Raspberry Pi](how-to/raspberry-pi.md)
* [Choose a hardware encoder](how-to/choose-a-hardware-encoder.md)
* [Roll an ad break](how-to/ad-breaks.md)
* [Add a theme](how-to/add-a-theme.md) (planned)
* [Write a preset](how-to/write-a-preset.md) (planned)
* [Upgrade from LiveboxMix](how-to/upgrade-from-liveboxmix.md)
* [Run the smoke test](how-to/smoke-test.md)
* [Embed the engine in your own program](how-to/embed-the-engine.md)

## Reference

* [The HTTP API](reference/http-api.md)
* [Safety: the minimum hold, the rate limit, the flash guard](reference/safety.md)
* [`agent.state`, both formats and their sizes](reference/agent-state.md)
* [Tasks: work that outlives the call that started it](reference/tasks.md)
* [The command line](reference/cli.md)
* [Configuration, every key](reference/configuration.md)
* [Sources](reference/sources.md)
* [Web page sources](reference/web-page-sources.md)
* [The generated protocol reference](reference/protocol.md)

## Explanation

* [Why the programme never stops](explanation/why-the-programme-never-stops.md)
* [Why plugins are processes](explanation/why-plugins-are-processes.md)
* [Why the UI is a client](explanation/why-the-ui-is-a-client.md)
* [Footprint budgets and the reference machines](explanation/footprint-budgets.md)
* [The crate map](explanation/architecture.md)

## For AI agents

* [The operator playbook](agents.md)
* [The friction log](friction-log.md), which is where a stuck moment on the
  developer path gets written down whether the person stuck was human or not.
