# Trust and signing

What `gmx plugin add` checks before it copies anything, what the label on an
installed plugin means, and what none of it proves.

## The problem this is solving

The single biggest complaint from people who tried to write OBS plugins, after
build complexity, was signing. Three platform builds, macOS notarisation, and
99 dollars a year to Apple per author, to publish something they were giving
away. The result was predictable: plugins shipped unsigned, users were told to
click through the warning, and the warning stopped meaning anything.

GodwinMix takes the signing off the author. The index CI builds every listed
plugin from its tagged source on five platforms, runs the conformance harness,
and signs the artefacts with sigstore keyless signing, tied to the CI's own
identity. The author needs no key, no certificate and no Apple account.

This works because a plugin here is a separate process rather than a library
loaded into an app bundle. Gatekeeper's rules for an in process `.plugin` do
not apply the way they do for a dylib inside a notarised app: the core is
notarised once, and it executes the plugin the way it executes any other
program. Where a platform still complains, what `gmx plugin add` checks is the
CI's signature, and this page is what the docs point at.

## The two levels, and why there are two

The obvious way to check a sigstore bundle is the `sigstore` crate. That was
measured rather than assumed, and the numbers are here so the decision can be
argued with:

| Build | Release binary |
|---|---|
| before any of this | 14,774,544 bytes |
| with the check that shipped | 15,512,032 bytes |
| with the `sigstore` crate instead | 21,569,952 bytes |

The crate was added at 0.14 with `verify` and `sigstore-trust-root` on, and
called from a path the binary actually reaches, because fat LTO strips a
verifier nothing calls and a measurement of stripped code is not a measurement.
It costs 5.78 MiB for one check, against the 3 MB this project allows any one
feature. It brings its own TUF client, an X.509 stack, a protobuf runtime, the
Rekor and Fulcio API models, and aws-lc-rs beside the rustls that is already
here.

It also does not compile into this workspace as it stands. Its transitive
`typed_path` dependency carries a blanket `AsRef` implementation for
`Cow<'_, str>` that breaks type inference in three untouched lines of the
engine, so adopting it would mean editing code that has nothing to do with
signatures.

The core has to fit on a Raspberry Pi and start fast. A verifier that costs a
third of the binary, to check a file most operators install once, is the wrong
trade.

So the check is in two levels, and the level reached is recorded rather than
glossed over.

### Level one: cosign is installed

If `cosign` is on `PATH`, `cosign verify-blob` runs and does the full
cryptographic verification: the signature over the artefact, the certificate
chain back to Fulcio's root, the certificate's identity against the one the
marketplace pinned, and inclusion in the Rekor transparency log.

This is the strong answer. The label is `signed` and the explanation names who
signed it.

### Level two: cosign is not installed

The bundle is parsed. The artefact's SHA-256 is computed and compared against
the digest the bundle says it signed. The bundle is required to carry a
certificate, a signature, and a transparency log entry with a log index.

This catches a swapped download, a truncated one, and a bundle that belongs to
another file. It does not prove who signed it, because nothing here validates
the certificate chain.

The label is `signed, digest only`, and the explanation says so in words:

    the bundle records sha256:9f2c1a... and that is what arrived, but nothing
    checked who signed it. Install cosign and run `gmx plugin update ndi` for
    the full check.

An operator who wants the strong answer installs cosign. The install line is
printed when the fallback happens, rather than the operator having to know.

The code is `crates/godwinmix-host/src/verify/mod.rs`, and the SHA-256 beside
it is written out rather than pulled in for the same reason: sixty lines of
arithmetic that has not changed since 2001 does not need a crate with a trait
hierarchy and a feature matrix. Its tests run the NIST vectors, so a mistake
fails the build.

Both bundle shapes sigstore has shipped are read: the current
`application/vnd.dev.sigstore.bundle.v0.3+json`, and the one
`cosign sign-blob --bundle` wrote for years. Both are in the wild.

## What identity a signature is checked against

A marketplace can say who signs the things it lists:

```json
"signing": {
  "identity_regexp": "^https://github.com/psmux/godwinmix-plugins/.github/workflows/.+",
  "oidc_issuer": "https://token.actions.githubusercontent.com"
}
```

A plugin resolved through that marketplace is verified against that identity,
so a valid signature from somebody else is refused. Without the block, a
signature is checked against the bytes and nothing pins who made it, which is
weaker and is what an unofficial listing gets.

## The third label: custom, unreviewed

Everything that arrives with no signature at all is labelled
`custom, unreviewed`, and the sentence under it is the one Home Assistant and
Obsidian both settled on: it runs with the permissions you give it.

That covers a path install, a git clone built on the machine, and anything from
cargo, npm or PyPI, because none of those three publishes a signature a core
can check. A crates.io checksum proves the bytes match what crates.io holds. It
does not say who published them, and the label does not pretend otherwise.

The label is written beside the install as `.gmx-trust.json` and read back on
every start, so a core that restarts still knows. A plugin directory that
somebody dropped in by hand has no record, and the honest answer for it is the
same one: unreviewed, and nothing recorded where it came from.

## The switch

    [plugins]
    allow_unsigned = true

True is the default, and it has to be: `gmx plugin add ./my-plugin` is the
whole of the developer path, and a mixer that refused it would be a mixer
nobody could write a plugin for.

An operator running unattended channels sets it to false. Then a plugin nothing
signed is refused, with the config block to add printed in the refusal, and
only a signed release installs. It is a deployment decision, not a default, and
the documentation says which is which rather than shipping a default that gets
in the way of the thing this project is for.

## What it does not prove

None of this is a sandbox, and no amount of signature checking makes it one.

A signature says the bytes that arrived are the bytes that were built, from the
source that was tagged, by the CI that was pinned. It says nothing about what
that source does. A signed plugin can read your streaming keys, open a socket
to anywhere, and write to any file the core can write to, because it is a
process running with the permissions you gave the core.

The isolation described in the plugin architecture is about blast radius: a
plugin that crashes costs one source or one output and never the programme
output, and the harness proves that with a kill test rather than asserting it.
It is not about containment.

What limits the damage is the same thing that limits it for every other program
on the machine: reading the source, or trusting whoever did. The quality scale
says which of those happened. That is all it says, and saying more would make
the label worth less than saying nothing.

## The api range check, which is the other half

Alongside the signature, every install checks that the plugin's `api` level is
one this core speaks, and this is where the refusal is worth more than the
check.

Terraform advertises a protocol version per provider and tells you which
version of Terraform speaks it. GodwinMix does the same thing, and it turns a
mismatch from a crash at launch into an upgrade hint:

    gmx-ndi 2.0.0 needs api 2 and was not installed. GodwinMix 0.4.0 or later
    speaks api 2; this core is 0.2.0, which speaks api 1. `gmx plugin search
    gmx-ndi` lists the versions and the api level of each.

The table of which core release first spoke each level is in
`verify::CORE_RELEASES`, one row per level, appended when a level ships. A level
with no row has not been released, and the message says that rather than
inventing a version number.

## See also

* [The quality scale](../reference/quality-scale.md), what each tier checked
* [The index format](../reference/index-format.md), where the signing identity
  is declared
* [Publish a plugin](../how-to/publish-a-plugin.md), from the author's side
* [Why plugins are processes](why-plugins-are-processes.md)
