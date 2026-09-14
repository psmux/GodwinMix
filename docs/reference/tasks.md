# Tasks

No method blocks for more than five seconds, which is the tightest client
default in the wild. A method whose work would run past that answers at once
with a handle, the work carries on, and `task.get` reads the outcome later.

A call that timed out on the caller's side is **indeterminate**, never failed.
A plugin that keeps installing after the caller was told it failed is worse
than an honest "unknown, read back before retrying".

## The handle

```json
{
  "task_id": "media-convert-3",
  "poll_interval_ms": 1000,
  "outcome": "indeterminate",
  "state": "running",
  "next": "the work is still running. Call task.get with task_id 'media-convert-3' every 1000 ms until state is completed, failed or cancelled. Over MCP: tasks/get if your client speaks the tasks extension, otherwise search_tools for \"task\" and call task_get.",
  "…": "whatever the method already knew"
}
```

Anything the method knew when it started rides along in the same body, so
`media.convert` answers with the conversion state and the handle together and
a caller that only wanted to know it started needs nothing else.

Ids are legible slugs made from the method name, never UUIDs, so a task can be
read back over a telephone.

## Reading one

```
task.get {"task_id": "media-convert-3"}
GET /api/v1/tasks/media-convert-3
```

```json
{
  "task_id": "media-convert-3",
  "kind": "media.convert",
  "state": "running",
  "progress": 0.42,
  "age_secs": 17,
  "poll_interval_ms": 1000
}
```

| Field | What it is |
|---|---|
| `state` | `running`, `completed`, `failed` or `cancelled` |
| `progress` | 0 to 1, where the work can say. Absent where it cannot |
| `result` | the body the method would have returned, once it is `completed` |
| `error` | why it stopped, once it is `failed` |
| `age_secs` | seconds since it started |
| `poll_interval_ms` | present only while it is running |

`task.list` returns every task this core knows about, newest first.

## Stopping one

```
task.cancel {"task_id": "media-convert-3"}
```

Cooperative. The answer says the request landed, not that the work has
stopped: the work checks and stops at the next point it tidily can. Read
`task.get` afterwards to see that it did. Nothing already finished is undone.

## How long they last

A finished task is readable for one hour. The table holds at most 256 tasks
and drops the oldest finished ones first; a running task is never dropped.
An id that has aged out answers `-32004` with the ids that exist and says so.

## Over MCP

The server declares the tasks extension in `initialize`:

```json
{"capabilities": {"experimental": {
  "io.modelcontextprotocol/tasks": {"ttlMs": 3600000, "pollIntervalMs": 1000}
}}}
```

A client that speaks it reads a long running call through the protocol. One
that does not gets the same `{task_id, poll_interval_ms}` body and calls the
`task_get` tool, which `search_tools` finds. `task_get` is deliberately not in
the hot list: that list is capped at twelve tools and charged for on every
call.

## For a method author

`spawn_task` in `crates/godwinmix/src/control/methods/tasks.rs`:

```rust
let body = spawn_task(&call.app.tasks, "plugin.add", Some(json!({ "plugin": name })), |ctx| async move {
    for step in steps {
        if ctx.cancelled() {
            return Err("asked to stop before …. Nothing was installed.".into());
        }
        ctx.progress(step.fraction);
        step.run().await.map_err(|e| e.to_string())?;
    }
    Ok(json!({ "plugin": name, "installed": true }))
});
Ok(body)
```

The future runs on the Tokio runtime the call is on and must not block; work
that would goes on `spawn_blocking` inside it. The `Ok` value is the body the
method would have answered with had it been quick enough, so a client reading
`result` gets exactly what a fast call would have given it. The `Err` string is
an error message, and like every other error it names the next step.

The table itself is `godwinmix_core::tasks`, in the engine rather than in the
control plane, so the plugin loader and the converter can both put work on it
without depending on a server.

## Related

* [`docs/reference/agent-state.md`](agent-state.md)
* [`docs/how-to/use-with-an-ai-agent.md`](../how-to/use-with-an-ai-agent.md)
