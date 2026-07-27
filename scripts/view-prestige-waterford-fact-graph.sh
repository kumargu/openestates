#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-tmp/fact_graph/prestige-waterford.html}"

cd "${ROOT}/backend"
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo run --bin openestates-fact-graph-dashboard -- \
  --project-root "${ROOT}" \
  --target "prestige waterford" \
  --out "${OUT}"

HTML_PATH="${ROOT}/${OUT}"
echo "Fact graph dashboard: ${HTML_PATH}"

if command -v open >/dev/null 2>&1; then
  open "${HTML_PATH}"
fi
