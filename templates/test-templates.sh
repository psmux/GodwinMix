#!/bin/sh
# Prove every template actually runs on this machine.
#
# For each template: copy it to a temp directory, fill in the placeholders the
# way `gmx plugin new` will, run its own ./check --quick, then feed it a
# handshake and a start and check that a Matroska cluster of the right size
# comes back within one second.
#
#   ./templates/test-templates.sh            every template
#   ./templates/test-templates.sh python go  only these
#
# A template whose toolchain is missing is skipped with a line saying so, not
# failed: Go is not installed everywhere and neither is cargo.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
templates=${*:-rust python node go shell}
python=${GMX_PYTHON:-python3}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

name=my-cam
width=160
height=90
fps=30
failed=0
ran=0

fill() {
    # The placeholder set. Keep this in step with templates/README.md.
    sed -e "s/{{name}}/$name/g" \
        -e "s/{{name_snake}}/my_cam/g" \
        -e "s/{{description}}/A source that shows a test picture./g" \
        -e "s/{{author}}/A Person/g" \
        -e "s/{{license}}/MIT/g" \
        -e "s/{{year}}/2026/g" \
        -e "s|{{sdk}}|{ path = \"$root/crates/godwinmix-sdk\" }|g"
}

have() { command -v "$1" >/dev/null 2>&1; }

# What a template needs on PATH before it can be tested at all.
tool_for() {
    case "$1" in
        rust) echo cargo ;;
        python) echo "$python" ;;
        node) echo node ;;
        go) echo go ;;
        shell) echo gst-launch-1.0 ;;
        *) echo false ;;
    esac
}

# Build, if the language has a build step, and print the command that starts
# the plugin. Run from inside the filled copy.
launch_for() {
    case "$1" in
        rust) cargo build --release --quiet >&2; echo "./target/release/$name" ;;
        python) echo "$python main.py" ;;
        node) echo "node main.js" ;;
        go) go build -o plugin . >&2; echo "./plugin" ;;
        shell) echo "sh run.sh" ;;
    esac
}

copy_filled() {
    src=$1
    dst=$2
    mkdir -p "$dst"
    for file in $(cd "$src" && find . -type f | sed 's|^\./||'); do
        mkdir -p "$dst/$(dirname "$file")"
        fill < "$src/$file" > "$dst/$file"
        if [ -x "$src/$file" ]; then chmod +x "$dst/$file"; fi
    done
}

for template in $templates; do
    src="$root/templates/$template"
    if [ ! -d "$src" ]; then
        echo "--- $template: no such template, skipped"
        continue
    fi
    tool=$(tool_for "$template")
    if ! have "$tool"; then
        echo "--- $template: $tool is not installed on this machine, skipped"
        continue
    fi
    echo "--- $template"
    ran=$((ran + 1))
    dst="$work/$template"
    copy_filled "$src" "$dst"
    if [ ! -f "$dst/gmx-plugin.toml" ]; then
        echo "  FAILED: no manifest was copied"
        failed=1
        continue
    fi

    if ! (cd "$dst" && ./check --quick); then
        echo "  FAILED: ./check --quick"
        failed=1
        continue
    fi

    if ! command=$(cd "$dst" && launch_for "$template" 2>"$work/$template.build"); then
        echo "  FAILED: the build"
        sed 's/^/    /' "$work/$template.build" | tail -20
        failed=1
        continue
    fi

    start=$(date +%s)
    {
        printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"core\":\"godwinmix\",\"version\":\"0.0.0\",\"api_level\":1,\"api_compatible\":1,\"canvas\":{\"width\":$width,\"height\":$height,\"fps\":$fps},\"transport\":\"container\",\"media\":\"\",\"instance\":\"t\",\"provide\":\"source\",\"params\":{}}}"
        printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"start\",\"params\":{\"canvas\":{\"width\":$width,\"height\":$height,\"fps\":$fps},\"transport\":\"container\",\"media\":\"\"}}"
        sleep 1
        printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"shutdown","params":{"reason":"template test"}}'
        sleep 1
    } | (cd "$dst" && exec $command) > "$work/$template.mkv" 2> "$work/$template.log" || true
    stop=$(date +%s)

    if ! "$python" "$root/templates/check-first-cluster.py" \
        "$work/$template.mkv" "$work/$template.log" "$width" "$height"; then
        echo "  FAILED: the media stream"
        sed 's/^/    /' "$work/$template.log" | head -20
        failed=1
        continue
    fi
    echo "  ok: $((stop - start))s from a cold start to a cluster and a clean exit"
done

if [ "$ran" -eq 0 ]; then
    echo "no template could be tested on this machine"
    exit 1
fi
if [ "$failed" -eq 0 ]; then
    echo "every template that could run on this machine ran"
else
    echo "at least one template failed"
fi
exit "$failed"
