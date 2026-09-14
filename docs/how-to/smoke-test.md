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

On Windows it is `dev/smoke.ps1`, which is a step for step port: the same
forty one steps in the same order, the same labels, the same `ok` and `FAIL`
lines, the same `--keep`, and the same exit code.

```
pwsh dev/smoke.ps1
```

Five of the forty one print `skip` there rather than `ok`, with the reason on
the same line. They are the plugin steps, which scaffold a shell plugin, and
the host refuses to launch one where `sh` is not a given. A skip is not a
failure and does not change the exit code. Everything else runs, including the
whole `/rpc`, preset, MCP and teardown sequence.

`dev/smoke_rpc.py`, `dev/smoke_mcp.py` and `dev/smoke_streams.py` are Python
and both scripts run them as they are.
