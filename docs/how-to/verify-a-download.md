# Check a download

Every release has a `SHA256SUMS` file with the hash of every other file in
it. Checking a hash tells you the file arrived whole. It does not tell you who
made it, since anyone who could change a file on the release page could
change `SHA256SUMS` as well. For that, check `SHA256SUMS` itself.

## The hash

In the folder you downloaded into:

```sh
sha256sum -c SHA256SUMS --ignore-missing
```

On macOS use `shasum -a 256 -c SHA256SUMS --ignore-missing`. Each file you
have should say `OK`.

## Who built it

From 0.3.34 the release workflow signs a provenance statement for every file
it publishes, `SHA256SUMS` included. The statement says which repository,
workflow and tag produced the file, and it is signed with the workflow run's
own GitHub identity through Sigstore, so there is no key of ours to leak or
lose. With the GitHub CLI:

```sh
gh attestation verify SHA256SUMS --repo psmux/GodwinMix
```

It prints the workflow and the tag, and fails if `SHA256SUMS` was not made by
this repository's release workflow. Once `SHA256SUMS` checks out, every hash
in it does too, so the hash check above covers the rest. You can verify any
single file the same way.

A script that wants the tag pinned as well can add
`--source-ref refs/tags/v0.3.34`.

## With minisign

A checker that would rather pin a plain Ed25519 key can use minisign, once
the release carries `SHA256SUMS.minisig` and `godwinmix-minisign.pub`. The
public key is also in this repository at `deploy/keys/godwinmix.pub`, and that
copy is the one to pin, not the one on the release page:

```sh
minisign -Vm SHA256SUMS -p godwinmix.pub
```

### Turning it on, for a maintainer

The release signs with minisign only when the repository has a secret called
`MINISIGN_SECRET_KEY`. Without it the release still goes out, with its
provenance and no `.minisig`. To set it up once:

```sh
minisign -G -W -p deploy/keys/godwinmix.pub -s godwinmix.key
gh secret set MINISIGN_SECRET_KEY --repo psmux/GodwinMix < godwinmix.key
```

Commit `deploy/keys/godwinmix.pub`. Keep `godwinmix.key` somewhere private
and off the repository, or delete it and make a new pair if it is ever needed
again. `-W` leaves the key without a password, because the workflow has no
one to type one; the secret store is what protects it. The release checks
its signature against the committed public key before publishing, so a
secret and a public key that do not match fail the release rather than ship.
