# The control protocol: where the reference lives

The reference is generated, not written. Three artefacts sit at the repository
root, all built from the method table in `crates/godwinmix-protocol/src/`, all
checked by a test that regenerates them and fails on any difference.

| File | What it is | Read it when |
|---|---|---|
| [`../../protocol.md`](../../protocol.md) | every method, event and type, obs-websocket style | you are writing a client, or wondering what a method answers |
| [`../../protocol.json`](../../protocol.json) | the same thing as JSON Schema | you are generating a client, or checking a payload |
| [`../../openapi.json`](../../openapi.json) | the REST layer as OpenAPI 3.1 | you want Swagger UI, or a generated HTTP client |

A running mixer serves the same documents, so a client can ask the box in
front of it rather than trusting a file it shipped with:

```sh
curl -s localhost:8080/api/v1/core/api | jq .api_level     # 1
```

And a binary will print them with no config, no GStreamer and no mixer
running, which is what CI uses:

```sh
godwinmix --api-info                  # protocol.json
godwinmix --api-info --markdown       # protocol.md
godwinmix --api-info --openapi        # openapi.json
```

## Regenerating them

If a test tells you the committed files are out of date, that is the drift
check doing its job. Run:

```sh
cargo run --quiet -- --api-info > protocol.json
cargo run --quiet -- --api-info --markdown > protocol.md
cargo run --quiet -- --api-info --openapi > openapi.json
```

and commit the result with the change that caused it.

## Version

`api_level` is what this build speaks and `api_compatible` is the oldest level
it still answers. Both are 1. A client that speaks level 1 works against every
core from here until `api_compatible` moves, and `core.info` reports both
along with the features and limits of the particular box you are talking to.

Every method and every event carries a `since`, so a client can tell what it
may use against an older core without probing for a 404.

## Adding to it

Methods are rows in a table, and the table is the only place a method is
declared: `/rpc`, `/api/v1`, these three documents and the MCP tool list are
all built from it. `crates/godwinmix-protocol/README.md` is the how to, and it is short.

## The rest of the shelf

* `docs/how-to/control-the-mixer.md`: curl and a twenty line Python client.
* `docs/how-to/use-with-an-ai-agent.md`: the MCP server and the agent surface.
* `docs/agents.md`: a director loop, end to end.
