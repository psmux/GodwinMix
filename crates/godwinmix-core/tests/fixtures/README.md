# Plugin fixtures

Whole plugins, in shell, for the tests that need a real process on the other
end of the control channel rather than a mock. They use only the public
contract in `docs/reference/plugin-protocol.md`, which is the point: if one of
these stops working, a plugin somebody wrote has stopped working too.

| Directory | What it is | Which test uses it |
|---|---|---|
| `fake-service/` | a `service` and a `device` in one plugin: a tool, a `discover`, and a publisher that arrives and leaves | `tests/supervisor.rs` |

Each one is copied into a temporary plugins directory by the test, installed
with the loader, and removed again, so nothing here is ever loaded by a running
core.
