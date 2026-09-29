# Rules for an agent working on GodwinMix

You are working in your own git worktree. Read AGENTS.md at its root first and
obey it: nothing may block a streaming thread or the bus handler, small
functions and modules (150 lines), ids are slugs, errors name the next step
and carry `data`, docs land with the code, and no dash punctuation anywhere
(no em dash, no en dash, no double hyphen as punctuation) in code comments,
docs, commit messages or your report.

Your worktree may be cut from an older commit. Run `git merge
work/composer-live` in it first and check that dev/plans/channels-contract.md
exists. That file is the contract for this work; read it before anything
else. Nothing you build may tell a person to edit a file or run a command:
the GUI does it for them (dev/audits/2026-09-22-click-through/PLAN.md).

## How to work

* Reproduce or observe before changing. Add code, do not rewrite code that
  works. Keep diffs in the style of the file around them.
* Every change gets tests: `cargo test -p <crate>` for Rust (real GStreamer,
  no mocks), `ui/test/run.js` for the page (run with `dev/ui-tests.sh`, a
  real core in headless Chrome). `cargo clippy --all-targets` clean on what
  you touched. `node --check` on every JS file you touched.
* A protocol method goes into the protocol crate's tables (methods, scopes,
  errors), then regenerate: `target/release/godwinmix --api-info >
  protocol.json`, `--api-info --markdown > protocol.md`, `--api-info
  --openapi > openapi.json`, `python3 clients/gen/generate.py`. A reference
  page under docs/reference and a how to under docs/how-to, CHANGELOG entry.
* The composer page has a byte budget (`cargo test -p godwinmix --lib ui::`).
  Anything heavy loads on first use with `import()` and is added to the lazy
  lists in that test, as ui/panels/welcome/after.js is.
* Run things in named tmux sessions (`gmx-<yourname>-...`), on the ports your
  brief gives you, from a COPY of godwinmix.example.toml with `bind` changed.
  Never touch godwinmix.example.toml itself. Kill your sessions when done.
* Browser checks: headless Chrome on your DevTools port,
  `'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
  --headless=new --remote-debugging-port=<port> --user-data-dir=<scratch>
  --window-size=1440,900 about:blank`, driven with dev/drive.mjs. Take
  screenshots and look at them: it must look good, not only work.
* An RTMP publisher for tests: `dev/harness/publish.sh` or ffmpeg with
  `-re -f lavfi -i testsrc2=size=1280x720:rate=30 -f lavfi -i
  sine=frequency=440 -c:v libx264 -g 60 -c:a aac -f flv <url>`.
* Never run git stash, git reset --hard, git checkout -- . or git clean.
  Commit on your branch as you go, one plain sentence per commit saying what
  changed and why. Commit before you stop. Do not push.
* Other agents work in parallel in their own worktrees; your brief says what
  they own. Stay out of those files so the merge is clean.

Finish with a short report: what you built (files), how you verified it
(tests, screenshots by file name), where you departed from the contract and
why, what is left. Plain sentences, no bold lead ins, no dash punctuation.
Name any wording you wrote that the owner should reword in his own voice.
