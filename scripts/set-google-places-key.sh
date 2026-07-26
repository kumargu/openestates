#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
env_file="$repo_root/.env.local"

usage() {
  cat <<'USAGE'
Usage:
  scripts/set-google-places-key.sh
  scripts/set-google-places-key.sh '<GOOGLE_PLACES_API_KEY>'

Writes GOOGLE_PLACES_API_KEY to .env.local with 0600 permissions.
With no argument, reads GOOGLE_PLACES_API_KEY from the current environment.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -gt 1 ]]; then
  usage >&2
  exit 2
fi

key="${1:-${GOOGLE_PLACES_API_KEY:-}}"

if [[ -z "${key}" ]]; then
  echo "GOOGLE_PLACES_API_KEY cannot be empty" >&2
  exit 1
fi

if [[ "${key}" == *$'\n'* || "${key}" == *$'\r'* ]]; then
  echo "GOOGLE_PLACES_API_KEY must be a single line" >&2
  exit 1
fi

umask 077
touch "$env_file"
chmod 600 "$env_file"

tmp_file="$(mktemp "$env_file.tmp.XXXXXX")"
chmod 600 "$tmp_file"
grep -v '^GOOGLE_PLACES_API_KEY=' "$env_file" > "$tmp_file" || true
printf "GOOGLE_PLACES_API_KEY=%q\n" "$key" >> "$tmp_file"
mv "$tmp_file" "$env_file"
chmod 600 "$env_file"

echo "Wrote GOOGLE_PLACES_API_KEY to $env_file"
echo "Load it with: set -a && source .env.local && set +a"
