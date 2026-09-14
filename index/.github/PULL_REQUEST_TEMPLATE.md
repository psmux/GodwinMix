# Listing

**Plugin name**:
**Source**:
**Tier you are asking for**: custom / bronze / silver / gold
**What it does, in one sentence**:

## Before you open this

* [ ] I ran `python3 bot/validate.py --entry <my-plugin>` and every check passed.
* [ ] I ran `gmx plugin test .` in the plugin and it printed `is conformant`.
* [ ] The version in `versions` has a matching tag in the source repository.
* [ ] The `api` in the entry is the `plugin.api` out of my `gmx-plugin.toml`.
* [ ] Every platform in `platforms` really ships an asset. I have not listed one
      I have not built for.
* [ ] I added my entry and changed nothing else in `index.json`.
* [ ] I did not edit `compatibility.md`. CI writes it.

## The policy

Every tier, including custom. README.md carries the full text.

* [ ] **No obfuscation.** The source this points at is the source that runs.
      Anything generated is built from sources in the same repository.
* [ ] **Telemetry is declared.** `policy.telemetry` says `none`, or says what is
      collected, where it goes and how it is switched off.
* [ ] **Network use is declared.** `policy.network` lists every host or protocol,
      one line each. An empty array means it never touches the network.
* [ ] **Secrets are declared.** `policy.secrets` names every credential it reads
      and where each one is sent. It reads nothing it was not given.
* [ ] **It stays in its lane.** It does not take, change the layout, or
      reconfigure other plugins unless the operator drove it there.

## The tier you asked for

Tick the block for your tier and nothing below it. The bot checks the mechanical
half of this and a reviewer reads the rest.

### custom

Nothing is required. The operator is told "unreviewed, runs with the permissions
you give it", and that is the whole promise.

### bronze

* [ ] The manifest validates: `gmx plugin test` check 7 passes.
* [ ] The conformance harness passes on at least one platform, and the result is
      in `versions[].harness` as `<platform>: 8/8`.
* [ ] A `SKILL.md` is present, and the entry's `skill` field names it.
* [ ] There is a release with a version.

### silver

Everything in bronze, and:

* [ ] The harness passes on all declared platforms, with a line for each.
* [ ] The settings schema has examples for every field. Harness check 4
      configures the plugin with each of them.
* [ ] Tools have annotations: `readOnlyHint`, `destructiveHint` and the rest,
      set honestly.
* [ ] No leaks after 30 add and remove cycles. Harness check 5 asserts no
      descriptors and no directories are left behind.
* [ ] A code owner responds within a month. Name who that is.

Code owner:

### gold

Everything in silver, and:

* [ ] Maintained by two or more people, or by the project. Named in
      `maintainers`.
* [ ] An eval suite with recorded shows. `evals` points at it.
* [ ] Documentation reviewed. The date is in `docs_reviewed`.
* [ ] On the official marketplace. `official` is true.

## Anything a reviewer should know

Hardware it needs, a licence that is not an SPDX id, a platform you left out on
purpose, a dependency that has to be installed first. Say it here rather than
leaving it to be found.
