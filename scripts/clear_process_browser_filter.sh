#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  MYTHIC_TOKEN=<token> scripts/clear_process_browser_filter.sh [graphql_url]
  scripts/clear_process_browser_filter.sh <token> [graphql_url]

Defaults:
  graphql_url: https://127.0.0.1:7443/graphql/

Clears the saved Mythic Process Browser column filters for the operator
identified by the supplied API token.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

TOKEN="${MYTHIC_TOKEN:-}"
GRAPHQL_URL="https://127.0.0.1:7443/graphql/"

if [[ -n "${TOKEN}" ]]; then
  GRAPHQL_URL="${1:-${GRAPHQL_URL}}"
else
  TOKEN="${1:-}"
  GRAPHQL_URL="${2:-${GRAPHQL_URL}}"
fi

if [[ -z "${TOKEN}" ]]; then
  echo "error: missing Mythic API token" >&2
  usage >&2
  exit 1
fi

curl -ksS "${GRAPHQL_URL}" \
  -H "apitoken: ${TOKEN}" \
  -H "Content-Type: application/json" \
  --data-binary '{"query":"mutation ClearProcessBrowserFilter { updateOperatorPreferences(preferences: {process_browser_filter_options: {}}) { status error } }"}'
echo
