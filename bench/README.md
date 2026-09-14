# bench

Measured footprint, one file per machine per run.

Every performance number GodwinMix publishes comes from `gmx bench`, and every
file in `results/` names the machine, the CPU, the RAM, the operating system,
the GStreamer version, the commit, and the command that produced each row. A
number without those is not a number anybody can plan against, which is the
state of the public record for every other mixer.

## Producing a file

```
cargo build --release
./target/release/gmx bench --machine m4pro
```

That writes `results/<machine>-<date>.md` and prints the same table on stdout.
Build in release first: a debug build makes every CPU figure wrong by a large
factor, and the table says so at the top when it detects one.

Useful flags:

* `--machine <id>` names the machine. Use a reference machine id (`pi4`, `pi5`,
  `n100`, `laptop`, `gpu`) when it is one of them, because several budgets are
  per machine. Defaults to the hostname.
* `--only <row>` runs one row or one group. Resident memory on an "added" row
  is a difference between two readings in one process, so a row measured on its
  own gives the cleaner figure.
* `--json` for CI.
* `--budget` exits non zero when a row is over its target for the named
  machine.
* `--quick` measures two seconds a row instead of thirty five. For working on
  the bench itself. The numbers are too noisy to publish.
* `--seconds`, `--warmup` to change the window.

The rows, what they mean and how they are sampled are in
`docs/explanation/footprint.md`.

## What goes in results/

Committed runs, one per machine per interesting commit. Keep the old ones: the
point of the directory is that a regression is visible as a difference between
two files rather than an argument.

The reference machines the plan commits to are `pi4`, `pi5`, `n100`, `laptop`
and `gpu`. Only the machines that have been run are here, and a missing machine
is a missing measurement rather than a passing one.
