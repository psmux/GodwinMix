<!--
One change per pull request. If you fixed a bug and also reformatted a file,
that is two pull requests.

First response is targeted at under 48 hours. It might be a question or a
"this needs a week", but you should not be left wondering whether anyone read
it.
-->

## What this changes

<!-- One or two sentences. If it changes behaviour anyone depends on (an HTTP
route, a config key, an environment variable, a file name), say that first. -->

## Why

<!-- The problem, not the patch. Link the issue if there is one. -->

## What you tested

<!-- What you ran and what you watched to decide it worked. A change that
cannot be tested without hardware should say what hardware and what you saw. -->

- [ ] `cargo test` passes
- [ ] `cargo clippy --all-targets` adds no warning that was not already there
- [ ] I added a test that fails without this change, or explained below why
      there is not one

## Effect on the programme output

<!-- The one question every change here has to answer. Delete the lines that
do not apply.

- Nothing in this change runs on a GStreamer streaming thread or in the bus
  handler.
- Nothing in this change can block, stall, or slow the programme encoder.
- This changes the source lifecycle, the mixing, or the output path, and I
  measured the programme's frame interval across it: ______ ms largest gap.
-->

## What is still missing

<!-- Known gaps, things you were not sure about, things you would like a
second opinion on. An honest list here makes review faster, not slower. -->

---

By opening this you agree to the [CLA](../CLA.md) and the
[code of conduct](../CODE_OF_CONDUCT.md). Sign off your commits with
`git commit -s`, which is how the CLA is accepted. Commit messages follow the
house style: read `git log` and match it, one sentence saying what changed and
why, no prefixes and no ticket numbers. The writing rules in
[CONTRIBUTING.md](../CONTRIBUTING.md) apply to the description you are typing
now as well as to the code.
