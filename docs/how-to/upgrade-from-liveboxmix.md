# Upgrading from LiveboxMix


The product was called LiveboxMix until 0.2. The binaries, the config file, the
environment variables and the desktop app's URL scheme all carry the new name
now. A box that was running 0.1 keeps working for this one release, with a
warning in the log each time it uses an old name:

| Old | New | For how long |
|---|---|---|
| `liveboxmix`, `liveboxmix-browser` | `godwinmix` (and `gmx`), `godwinmix-browser` | replace the binaries at the same time |
| `liveboxmix.toml` | `godwinmix.toml` | the old name is read when the new one is absent, until 0.3 |
| `LIVEBOXMIX_TOKEN`, `LIVEBOXMIX_URL` | `GODWINMIX_TOKEN`, `GODWINMIX_URL` | the old names are read, until 0.3 |
| `lbx-browser-<pid>` profile directories | `gmx-browser-<pid>` | both are cleaned up, until 0.3 |
| `liveboxmix://quit` from a cached page | `godwinmix://quit` | the desktop app answers both, until 0.3 |
| `lbx.token` in the browser | `gmx.token` | moved across once on first load |

The runtime store still follows the config file's stem, so a mixer that falls
back to `liveboxmix.toml` keeps reading and writing `liveboxmix.runtime.toml`
and the sources it was given stay where they are. Rename both files together
when you rename anything.

Two names in the sidecar's environment changed without a fallback, because they
are development switches rather than deployment contracts:
`LBX_BROWSER_SWITCHES` is now `GMX_BROWSER_SWITCHES` and `LBX_SIDECAR_LOG` is
now `GMX_SIDECAR_LOG`.
