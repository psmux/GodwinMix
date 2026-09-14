# The protocol reference

**This page is a pointer, not the reference.** The reference itself is
generated from the code, and the generator is being built now. Nothing here is
typed by hand, because a protocol document maintained by hand is wrong within a
release and nobody notices until somebody's client breaks.

## What will be generated, and from where

| File | Produced by | What it is |
|---|---|---|
| `protocol.json` | `godwinmix --api-info` | every request, response, event and status type, as JSON Schema. Committed to the repository; CI fails when it drifts from the code |
| `protocol.md` | from `protocol.json` | the same thing as a page to read |
| `openapi.json` | served at `/api/openapi.json` | the HTTP surface, for generating clients |

The drift check already has a job in
[`.github/workflows/build.yml`](../../.github/workflows/build.yml): the test
suite asserts that every route the control server serves appears in the
committed schema, and a step regenerates `protocol.json` and fails if the
result differs from what is in the tree. Until `--api-info` exists that step
reports that there is nothing to check, which is the correct thing for it to
do.

## Until then

[The HTTP API](http-api.md) is the hand written list of routes. It is accurate
as of the release it ships with and it will be deleted the day the generated
reference exists.

## The compatibility promise

This is the part that will not change.

* `api_level` 1 is frozen for breaking changes.
* Additions bump `api_level`. A client written against level 1 keeps working
  against a mixer at level 3.
* `api_compatible` moves only at an announced major version, at most once a
  year, and the previous level is supported for a year after that.
* Every error carries a machine readable shape and a message that names the
  next action, not only what went wrong.

`core.info` will carry `version`, `api_level`, `api_compatible`, the feature
list and the limits, so a client can decide what it may call without guessing
from a version string.

The models for this are Go 1, Rust's RFC 1105 and Stripe's versioning since
2011. The anti model is a plugin surface that breaks on every major release,
which is what the research on plugin ecosystems found kills them.

## Writing a client before the generator lands

The API is small enough to write against by hand, and
[`gmx ctl`](cli.md) is a worked example of a complete client in the repository.
Two rules that will not change under you:

* Ids are legible slugs (`cam-wide`), stable across restarts, never UUIDs. An
  unknown id answers with the valid ones.
* A mutating call returns the resulting state, so you never need a follow up
  read to find out what happened.
