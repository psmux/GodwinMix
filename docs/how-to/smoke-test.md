# Run the smoke test

`dev/smoke.sh` starts a core on a free port with a token, adds a test source,
puts it on air, and then looks at the running mixer through every door it has:
`/api/v1`, `/rpc`, `/metrics`, the web UI's own transport probe, `gmx ctl` and
`gmx mcp`. It ends by shutting the core down and checking that no process and
no panic was left behind.

Every step prints `ok` or fails the run, and the script exits non-zero if any
of them did, so it belongs in CI as well as on your own machine before a push.
A failure keeps the working directory and names it, with the core's log,
the `/rpc` transcript and the config it started from inside.

```
dev/smoke.sh          # or dev/smoke.sh --keep to keep the working directory anyway
```
