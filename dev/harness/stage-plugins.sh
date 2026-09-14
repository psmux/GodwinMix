#!/usr/bin/env bash
# Build the first party network plugins and put each binary beside its manifest.
#
# `gmx plugin add <dir>` copies the plugin directory (skipping `target`) and the
# core then runs `[run] bin` inside the copy, so the binary has to sit at
# `plugins/<name>/bin/<binary>` rather than in the workspace target directory.
# `gmx plugin test` and `gmx plugin test --offline` want the same thing, because
# both resolve `[run]` before they spawn anything.
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
