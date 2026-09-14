# src/api: the control surface, as data

Every method the core answers is a row in a table. The table is walked to
dispatch a call on `/rpc`, to build the `/api/v1` router, to write
`protocol.json` and `protocol.md`, and to build the MCP tool list. Adding a
row adds the method to all four.

## Adding a method from another module

Your module owns its methods. control.rs does not need to know they exist
beyond one call that hands your module the registry.

```rust
use godwinmix::api::method::{MethodDef, Registry, schema_of, Tier};
use godwinmix::api::scope::Scope;
use godwinmix::control::Call;              // the call context
use std::sync::Arc;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "log.set",
            Scope::Admin,
            "Change one instance's log level while it runs.",
            Arc::new(|call, params| Box::pin(async move {
                let req: LogSetRequest = crate::control::parse_params(&params)?;
                // ... do the work, return a serde_json::Value
                Ok(serde_json::json!({ "instance": req.instance, "level": req.level }))
            })),
        )
        .params(schema_of::<LogSetRequest>)
        .result(schema_of::<LogSetResult>),
    );
}
```

Then one line in `control::registry()`:

```rust
crate::observe::register(&mut reg);
```

That is the whole integration. In return you get:

* `POST /api/v1/log/set`, generated from the name by `method::rest_transform`.
  The rule is in 03 section 6 and `rest_transform` is the only implementation
  of it, so you never choose a path.
* `log.set` callable on `/rpc` by its dotted name.
* A row in `protocol.json` and `protocol.md`, with your params and result
  schemas, on the next `cargo test`. The drift test fails until you commit the
  regenerated files: run `cargo test protocol -- --nocapture` and follow what
  it prints.
* `trace_id`, `idempotency_key`, `dry_run` and `confirm` handled for you.
  A `Scope::Admin` method is refused for a token without admin before your
  handler runs.

## What the flags mean

| Field | Effect |
|---|---|
| `scope` | The least a token needs. `-32002` below it, before the handler runs. |
| `.destructive()` | Accepts `dry_run`, and needs a confirm token on a `confirm = required` token. |
| `mutating` | Accepts `idempotency_key` and is replayed for 24 hours. Set from the scope; override with `.mutating(false)` for an admin method that only reads. |
| `idempotent` | Calling twice with the same arguments leaves the same state. Becomes `idempotentHint` on the MCP tool, and the server is expected to honour it. |
| `.tool(name, tier, description)` | Makes it an MCP tool. `Tier::Minimal` is one of the five, `Tier::Standard` one of the twelve, `Tier::Search` reachable only through `search_tools`. |

Do not add a `Tier::Minimal` or `Tier::Standard` tool without reading the
budget test in mcp.rs first. The hot list has a hard byte budget and the test
fails rather than letting a prompt grow quietly.

## Errors

Return `RpcError`. Use the constructors: `RpcError::not_found` lists the ids
that would have worked, `RpcError::scope` names the scope to ask for. The
house rule from 01 is that every message names the current state and the next
step; a test asserts that no error message in the table is a bare refusal.

## Types

Put request and result structs in `requests.rs` or in your own module with
`#[derive(Serialize, Deserialize, JsonSchema)]`, and point `.params()` and
`.result()` at `schema_of::<T>`. Shared types land in `$defs` once. The MCP
tool list inlines them, because most clients do not resolve `$ref`.

## Where the expensive streams hang

Nothing in this module. A stream that costs CPU is held up by whoever is
watching it: `AppState::multiview` hands out a `MultiviewSubscription`, the
mosaic pipeline exists for as long as one is alive, and it is taken down
again a couple of seconds after the last one drops. A `/rpc` connection that
subscribed with `ext.multiview` holds one for as long as it wants frames. See
`src/multiview.rs`.
