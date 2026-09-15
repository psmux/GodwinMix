#!/usr/bin/env bash
# Build the first party network plugins and put each binary beside its manifest.
#
# This is a shortcut, not a requirement. `gmx plugin add <dir>` runs the
# manifest's `[build]` section itself when the binary `[run] bin` names is not
# there, so a clean checkout installs any of these in one command. What this
# script buys you is doing all four builds at once, ahead of time, so that the
# install is instant and so that a failing build shows up here rather than
# inside an install.
#
# The binary has to sit at `plugins/<name>/bin/<binary>` rather than in the
# workspace target directory, because the install copies the plugin directory
# and skips anything named `target`. `gmx plugin test` and `gmx plugin test
# --offline` want the same thing, because both resolve `[run]` before they
# spawn anything, and neither of them builds.
#
#   dev/harness/stage-plugins.sh            build in release and stage
#   dev/harness/stage-plugins.sh --debug    build in debug, which is quicker
#
# plugins/*/bin/ is in .gitignore.
set -uo pipefail

here=$(cd "$(dirname "$0")/../.." && pwd)
profile=release
flag=--release
if [ "${1:-}" = "--debug" ]; then
  profile=debug
  flag=
fi

# The plugin directory, the cargo package, and the binary it produces.
plugins="srt:gmx-srt whip:gmx-whip ingest:gmx-ingest ndi:gmx-ndi"

exe=""
if [ "${OS:-}" = "Windows_NT" ]; then exe=".exe"; fi

failed=0
for entry in $plugins; do
  dir=${entry%%:*}
  pkg=${entry##*:}
  if [ ! -d "$here/plugins/$dir" ]; then
    printf '%-24s %s\n' "$dir" "not in this checkout, skipped"
    continue
  fi
  printf '%-24s' "$dir"
  # shellcheck disable=SC2086
  if ! (cd "$here" && cargo build $flag -p "$pkg" --quiet) 2>"$here/plugins/$dir/.build.log"; then
    printf 'FAILED\n'
    tail -5 "$here/plugins/$dir/.build.log" >&2
    failed=$((failed + 1))
    continue
  fi
  rm -f "$here/plugins/$dir/.build.log"
  mkdir -p "$here/plugins/$dir/bin"
  cp "$here/target/$profile/$pkg$exe" "$here/plugins/$dir/bin/$pkg$exe"
  printf 'staged bin/%s\n' "$pkg$exe"
done

if [ "$failed" -gt 0 ]; then
  echo "$failed plugin(s) did not build" >&2
  exit 1
fi
echo "every network plugin is staged. Next: gmx plugin test plugins/srt --quick"
