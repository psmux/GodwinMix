# Add a theme

**Themes do not exist yet.** There is no theme directory, no `gmx theme`
command and no stylesheet contract. This page says what is planned and what you
can do today, so that nobody spends an afternoon looking for a feature that is
not there.

## What is planned

A theme will be a directory of CSS and, optionally, icons, dropped where the
mixer can find it and selected in the UI's settings. It restyles the reference
UI without forking it. The contract will be a documented set of custom
properties (colours, spacing, the tally red, the on air bar) plus a stable set
of class names on the shell, so that a theme survives a UI release.

Themes land with the UI split (roadmap Phase 3), which turns `ui/index.html`
into a client, a shell and six panels. A theme is not worth defining against a
single 1,500 line file, because every release would break it.

The same phase brings panel plugins, which are a different thing: a panel adds
a piece of UI, a theme restyles the UI that is there.

## What you can do today

The UI is one page served by the binary, and the mixer will serve a directory
of your own instead of the built in one:

```toml
[control]
ui_dir = "/srv/godwinmix/ui"
```

```sh
cp ui/index.html /srv/godwinmix/ui/index.html
# edit it, reload the browser
```

That is a fork, not a theme. Your copy will drift from the one in the release
and you will have to merge changes by hand. It is worth doing for a fixed
install (a church that wants its own colours on a screen nobody else sees) and
not worth doing for anything you plan to keep up to date.

There is no build step: it is plain HTML, CSS and JavaScript with no framework.
The CSS is the first 650 lines of the file.

## If you want to help shape it

The thing most likely to make themes work or fail is the set of custom
properties. If you have restyled a broadcast tool before and know which values
matter (and which ones nobody ever changes), say so on an issue. That is
cheaper to get right before the contract is written than after.

Add the moment you got stuck to [the friction log](../friction-log.md).
