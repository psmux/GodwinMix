#!/usr/bin/env bash
# One end to end run of the godwinmix package against a core this script starts.
#   clients/python/e2e/run.sh
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$REPO/clients/e2e.sh" python3 "$REPO/clients/python/e2e/e2e.py"
