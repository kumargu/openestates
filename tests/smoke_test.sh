#!/usr/bin/env bash
# Live production-router smoke checks; controlled semantics run in the contract suites.
set -euo pipefail
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
exec python3 "$SCRIPT_DIR/api_smoke.py" "${1:-4000}"
