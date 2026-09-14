# Integrations

Plugins for hosts that are not GodwinMix and that decide for themselves what a
plugin is written in.

| Directory | Host | Why it is here |
|---|---|---|
| `companion/` | Bitfocus Companion | over 800 modules, and every hardware panel that matters already speaks to it |
| `streamdeck/` | Elgato Stream Deck | the panel a lot of people own and nothing else |

Both are TypeScript. Everything else first party in GodwinMix is Rust, and the
reason these are not is in each README: Companion loads a module as a Node
process over `@companion-module/base`, and the Stream Deck app loads a plugin as
a Node process over `@elgato/streamdeck`. There is no other way in to either.

Neither is a second implementation of the protocol. Both are clients of
`@godwinmix/client` in `clients/typescript`, which speaks the same public
contract as the web UI, and neither can do anything a script you write yourself
could not.

Both run `npm test` with nothing installed: no registry, no lockfile, no
network. Each has its own copy of the fake core from
`clients/typescript/test/fake-core.ts`, which is a real HTTP server on a real
port upgrading a real socket, so the code under test is exercised over the wire.

Neither host can be installed in this repository's environment, so neither has
been loaded by the real thing. Each README says exactly what was verified and
what a person has to check on real hardware.
