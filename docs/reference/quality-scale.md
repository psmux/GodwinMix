# The quality scale

Four tiers, read off every catalogue entry, every `plugin.list` result and every
`plugin.describe` answer. They say what was checked. They do not say a plugin is
safe, and the wording everywhere is careful about the difference.

The idea is Home Assistant's, including the part most projects leave out: a
`custom` tier that is a real tier with a real label, rather than a category of
thing that is not allowed to exist.

## The tiers

| Tier | What it asks for | What the operator sees |
|---|---|---|
| `custom` | Nothing. Listed by topic, added by path, or published without going through an index. | `custom, unreviewed`, and the sentence "it runs with the permissions you give it" |
| `bronze` | The manifest validates. The conformance harness passes on at least one platform. A `SKILL.md` is present. There is a release with a version. | a badge, and a listing in the community marketplace |
| `silver` | The harness passes on every platform the plugin declares. The settings schema has `examples` for every field. Tools carry annotations. No leaks after 30 add and remove cycles. A code owner answers within a month. | a badge, sorted above bronze |
| `gold` | Maintained by two or more people, or by the project. An eval suite with recorded shows. Documentation reviewed. On the official marketplace. | a badge, and it is preloaded in presets |

## What each requirement is actually checking

### The manifest validates

`gmx-plugin.toml` parses and every rule in
[the plugin manifest](plugin-manifest.md) holds. The validator reports every
problem it finds in one pass, each with the key path that caused it, so this is
a check an author fixes in one go.

### The harness passes

`gmx plugin test` runs eight checks against a real core with real GStreamer on
both sides: it spawns, it handshakes within five seconds, its video and audio
caps match the canvas contract, it delivers the frames it promised at the rate
it promised, it takes a `configure` while running, it answers `health`, it dies
cleanly when killed and the programme does not notice, and its manifest
validates. The kill test is the one that matters most for the promise in the
vision: a plugin that is killed mid frame must not stall the programme output,
and the harness proves that rather than asserting it.

Bronze wants one platform. Silver wants every platform the manifest declares,
which is why declaring platforms honestly costs nothing and declaring them
optimistically costs a tier.

### `SKILL.md`

The file an AI agent reads to know when to use the plugin and what to pass it.
It is required from bronze because an ecosystem that advertises agent
operability and then ships plugins an agent cannot discover is advertising
something it does not have.

### Examples on every settings field

From silver. The settings schema is the only settings UI a plugin gets: every
surface renders it, and a field with a type and no example is a field an
operator guesses at.

### No leaks after 30 add and remove cycles

From silver. `gmx chaos` and the harness's soak run add and remove an instance
thirty times and watch RSS and the file descriptor count. A plugin that leaks
a descriptor per instance is fine in a test and a problem in a four hour show.

### A code owner answers within a month

From silver. Measured off the repository, the same way the project measures its
own time to first response. It is the difference between a plugin and an
abandoned repository that still installs.

### An eval suite with recorded shows

From gold. Recorded sessions replayed against the plugin, so a change that
breaks it breaks a test rather than somebody's Sunday.

## Where the tier shows up

In the marketplace listing:

    gmx plugin search ndi

    PLUGIN           VERSION   TIER     SOURCE                     DESCRIPTION
    ndi              1.2.0     gold     psmux/gmx-ndi              NDI sources and outputs

In what is installed, where the label is about the signature rather than the
tier, because that is the question an operator has once it is on their machine:

    gmx plugin list

    PLUGIN           VERSION   STATE    TRUST                PROVIDES
    ndi              1.2.0     on       signed               ndi/source, ndi/output
    myhack           0.1.0     on       custom, unreviewed   myhack/source

And in full, with the sentence behind the label:

    gmx plugin describe myhack

    trust        custom, unreviewed
                 unreviewed: it was installed from a directory on this machine.
                 It runs with the permissions you give it.

## What no tier means

None of this is a sandbox, and the documentation does not pretend otherwise.
A plugin runs as a process with the permissions the operator gave the core. The
isolation described in the plugin architecture limits the blast radius of a
crash to one source or one output; it does not limit what a plugin can read or
send. Gold means several people looked at it and CI proved some things about
it. It does not mean it cannot read your streaming keys.

That honesty is deliberate and it is the same call Obsidian and Home Assistant
made. A tier that implied safety would be a tier that made people install
things they would otherwise have read first.

## The listing policy

The condition of being listed at all, checked by the bot where it can be
checked and by a person where it cannot:

* No obfuscation. The source that is published is the source that is built.
* No telemetry without disclosure, in the README and in the listing.
* Network use declared: which hosts, for what.
* Secrets declared: what the plugin reads, and from where.

A plugin that breaks the policy is delisted, and the pull request that delists
it says which clause. See the listing policy section of `CONTRIBUTING.md`.

## See also

* [The index format](index-format.md), where the tier is written down
* [Trust and signing](../explanation/trust-and-signing.md), what the label means
* [Publish a plugin](../how-to/publish-a-plugin.md)
* [Test a plugin](../how-to/test-a-plugin.md), the harness in detail
