#!/usr/bin/env bash
# One end to end run of @godwinmix/client against a core this script starts.
#   clients/typescript/e2e/run.sh
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$REPO/clients/e2e.sh" node "$REPO/clients/typescript/e2e/e2e.ts"
