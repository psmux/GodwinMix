# The client libraries

Three thin libraries over the control protocol, and the generator that keeps
them honest.

| Directory | Package | What it is |
|---|---|---|
| `typescript/` | `@godwinmix/client` | ESM, no runtime dependencies, browsers and Node 20 or later |
| `python/` | `godwinmix` | asyncio, standard library only, Python 3.10 or later |
| `../crates/godwinmix-client/` | `godwinmix-client` | tokio and tokio-tungstenite, in the Cargo workspace |
| `gen/` | | the generator: `protocol.json` in, typed code out |

To write a surface with one, read
[docs/how-to/write-a-ui.md](../docs/how-to/write-a-ui.md). What each library
exposes is in [docs/reference/clients.md](../docs/reference/clients.md).

## The generator

`protocol.json` at the repository root is the contract. The core writes it out
of `core.api`, CI checks it, and the generator reads it. One command puts a new
method into all three libraries:

```bash
python3 clients/gen/generate.py
```

It writes three files and nothing else:

```
clients/typescript/src/generated/protocol.ts
clients/python/godwinmix/_generated.py
crates/godwinmix-client/src/generated.rs
```

Each holds the types, a typed signature per method, the event list, the `ext`
table and `API_LEVEL`. Everything else in each library is hand written.

Python's standard library is the whole toolchain. There is no new dependency to
install and nothing to build, which is deliberate: a generator that needs its
own stack is a generator nobody runs.

```bash
python3 clients/gen/generate.py --check           # report drift, write nothing
python3 clients/gen/generate.py --lang ts         # one language
python3 clients/gen/generate.py --protocol other.json
```

### Drift tests

Each library has a test that runs `--check` for its own language and fails when
the committed output is stale, naming the command to run. A method added to the
core therefore breaks three test suites until somebody regenerates, which is
the point.

| Library | Test |
|---|---|
| TypeScript | `clients/typescript/test/generated.test.ts` |
| Python | `clients/python/tests/test_generated.py` |
| Rust | `crates/godwinmix-client/tests/generated.rs` |

The same tests also check the generated tables against `protocol.json`
directly: every method present, every event, every `ext` key, and `api_level`
matching.

### Adding a method to the core

1. Add it in `src/api/` and let the core regenerate `protocol.json`.
2. Run `python3 clients/gen/generate.py`.
3. Commit what changed. The three libraries now have it, typed.

A method that needs more than a signature (a friendlier name, a shortcut, a
helper that folds two calls into one) gets that by hand beside the generated
file, never inside it: the file is overwritten every time.

### How the generator is put together

| File | What it does |
|---|---|
| `gen/generate.py` | the command: reads, renders, writes or checks |
| `gen/model.py` | one reading of `protocol.json`: types, methods, events, ext |
| `gen/names.py` | `program.take` to `programTake`, `program_take`, `ProgramTake` |
| `gen/ts.py`, `gen/py.py`, `gen/rs.py` | one backend per language, syntax only |

The backends share the model, so the three languages cannot disagree about what
a method takes. Adding a fourth language is one file and one line in `TARGETS`.

## Running the tests

```bash
cd clients/typescript && npm install && npm test   # typescript is the only devDependency
python3 -m unittest discover -s clients/python/tests
cargo test -p godwinmix-client
```

Each suite runs against a fake core that speaks the real protocol over a real
socket. The TypeScript and Python fakes are written on their standard
libraries, so those two suites need nothing installed at all.

## End to end, against a real core

```bash
clients/typescript/e2e/run.sh
clients/python/e2e/run.sh
clients/rust/e2e/run.sh
```

Each starts a core on a free port with a token and no sources, the way
`dev/smoke.sh` does, then connects, subscribes, waits for the snapshot and the
flush, adds a `test://smpte` source, takes it, waits for `event/program.took`,
removes the source and disconnects. `clients/e2e.sh` is the shared runner and
takes any command:

```bash
clients/e2e.sh node my-surface/smoke.mjs     # $1 is the URL, $2 is the token
```

## Publishing

Not done yet: nothing here has been pushed to a registry. When it is, the
version is the `api_level` the library speaks, so all three are 1.x for
`api_level` 1, and a library is released only after its end to end run passes
against a core built from the same commit.

```bash
# TypeScript
cd clients/typescript
npm test && npm run build
npm pack                      # check what is in the tarball, and its size
npm publish --access public   # @godwinmix is a scoped package

# Python
cd clients/python
python3 -m unittest discover -s tests
python3 -m build              # pip install build
python3 -m twine upload dist/*

# Rust
cargo test -p godwinmix-client
cargo publish -p godwinmix-client --dry-run
cargo publish -p godwinmix-client
```

Before any of that, regenerate and run the drift tests: a library published
from a stale `protocol.json` is a library that lies about what the core can do.
