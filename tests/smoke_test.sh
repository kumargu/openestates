#!/usr/bin/env bash
# OpenEstates Integration Smoke Tests
# Run after every task to verify nothing is broken.
#
# Usage:
#   ./tests/smoke_test.sh              # default: backend at localhost:4000
#   ./tests/smoke_test.sh 8080         # custom port
#
# Prerequisites: backend must be running. curl + jq required.

set -euo pipefail

PORT="${1:-4000}"
BASE="http://localhost:${PORT}"
PASS=0
FAIL=0
ERRORS=""

red()   { printf "\033[31m%s\033[0m" "$1"; }
green() { printf "\033[32m%s\033[0m" "$1"; }
bold()  { printf "\033[1m%s\033[0m" "$1"; }

check() {
  local name="$1"
  local url="$2"
  local jq_expr="$3"        # jq expression that must return truthy
  local desc="$4"           # human-readable expectation

  local http_code body
  http_code=$(curl -s -o /tmp/oe_test_body.json -w "%{http_code}" "$url" 2>/dev/null) || {
    FAIL=$((FAIL + 1))
    ERRORS+="  FAIL: $name — connection refused (is backend running on port $PORT?)\n"
    printf "  %s %s — connection refused\n" "$(red "✗")" "$name"
    return
  }

  body=$(cat /tmp/oe_test_body.json)

  if [[ "$http_code" != "200" ]]; then
    FAIL=$((FAIL + 1))
    ERRORS+="  FAIL: $name — HTTP $http_code (expected 200)\n"
    printf "  %s %s — HTTP %s\n" "$(red "✗")" "$name" "$http_code"
    return
  fi

  if [[ -n "$jq_expr" ]]; then
    local result
    result=$(echo "$body" | jq -r "$jq_expr" 2>/dev/null) || result="jq_error"
    if [[ "$result" == "false" || "$result" == "null" || "$result" == "jq_error" || -z "$result" ]]; then
      FAIL=$((FAIL + 1))
      ERRORS+="  FAIL: $name — assertion failed: $desc\n"
      printf "  %s %s — %s\n" "$(red "✗")" "$name" "$desc"
      return
    fi
  fi

  PASS=$((PASS + 1))
  printf "  %s %s\n" "$(green "✓")" "$name"
}

echo ""
bold "OpenEstates Smoke Tests"
echo " (${BASE})"
echo ""

# ── Health ──
echo "Health"
check "GET /api/health returns ok" \
  "${BASE}/api/health" \
  '.status == "ok"' \
  "expected status=ok"

# ── Properties List ──
echo ""
echo "Properties"
check "GET /api/properties returns array" \
  "${BASE}/api/properties" \
  'type == "array"' \
  "expected JSON array"

check "GET /api/properties has items" \
  "${BASE}/api/properties" \
  'length > 0' \
  "expected non-empty array"

check "GET /api/properties items have required fields" \
  "${BASE}/api/properties" \
  '.[0] | has("id", "title", "area", "price", "bhk")' \
  "expected id, title, area, price, bhk fields"

check "GET /api/properties items have transparency_tags" \
  "${BASE}/api/properties" \
  '.[0] | has("transparency_tags")' \
  "expected transparency_tags field"

# ── Property Detail ──
echo ""
echo "Property Detail"

# Get first property ID for detail test
FIRST_ID=$(curl -s "${BASE}/api/properties" 2>/dev/null | jq -r '.[0].id // empty' 2>/dev/null || echo "")

if [[ -n "$FIRST_ID" ]]; then
  check "GET /api/properties/:id returns property" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '.property.id != null' \
    "expected property.id"

  check "Property detail has themes" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '.themes | has("value", "commute", "society", "risk")' \
    "expected themes with value, commute, society, risk"

  check "Property detail has tradeoffs" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '.tradeoffs | has("headline", "strengths", "cautions")' \
    "expected tradeoffs with headline, strengths, cautions"

  check "Property detail has market_activity" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '.market_activity | has("interest_level", "days_on_market")' \
    "expected market_activity with interest_level, days_on_market"

  check "Property detail has similar_properties array" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '.similar_properties | type == "array"' \
    "expected similar_properties array"

  check "Theme labels are valid" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '.themes.value.label | IN("strong", "good", "mixed", "weak")' \
    "expected label in strong/good/mixed/weak"
else
  echo "  $(red "✗") Skipping detail tests — no property ID available"
  FAIL=$((FAIL + 1))
fi

# ── Property 404 ──
echo ""
echo "Property Not Found"
NOT_FOUND_CODE=$(curl -s -o /dev/null -w "%{http_code}" "${BASE}/api/properties/nonexistent-id-xyz" 2>/dev/null || echo "000")
if [[ "$NOT_FOUND_CODE" == "404" ]]; then
  PASS=$((PASS + 1))
  printf "  %s GET /api/properties/bad-id returns 404\n" "$(green "✓")"
else
  FAIL=$((FAIL + 1))
  printf "  %s GET /api/properties/bad-id — expected 404, got %s\n" "$(red "✗")" "$NOT_FOUND_CODE"
fi

# ── Search ──
echo ""
echo "Search"
check "GET /api/search?q=3BHK returns results" \
  "${BASE}/api/search?q=3BHK%20Whitefield" \
  '.results | length > 0' \
  "expected non-empty results"

check "Search results have match fields" \
  "${BASE}/api/search?q=3BHK%20Whitefield" \
  '(.results[0] | has("match_score", "match_label", "match_reason"))' \
  "expected match_score, match_label, match_reason"

check "Search response has intent" \
  "${BASE}/api/search?q=3BHK%20Whitefield" \
  '.intent | has("area", "bhk")' \
  "expected intent with area, bhk"

check "Search response has query echo" \
  "${BASE}/api/search?q=hello" \
  '.query == "hello"' \
  "expected query echo"

check "Empty search returns empty results" \
  "${BASE}/api/search?q=" \
  '.results | length == 0' \
  "expected empty results for empty query"

# ── Areas ──
echo ""
echo "Areas"
check "GET /api/areas returns array" \
  "${BASE}/api/areas" \
  'type == "array"' \
  "expected JSON array"

check "GET /api/areas has items" \
  "${BASE}/api/areas" \
  'length > 0' \
  "expected non-empty array"

check "Area items have required fields" \
  "${BASE}/api/areas" \
  '.[0] | has("id", "name", "median_price_per_sqft", "trend_direction")' \
  "expected id, name, median_price_per_sqft, trend_direction"

# ── Knowledge Graph ──
echo ""
echo "Knowledge Graph"
check "GET /api/knowledge/stats returns counts" \
  "${BASE}/api/knowledge/stats" \
  '.total_nodes > 0' \
  "expected total_nodes > 0"

check "Knowledge stats has fact count" \
  "${BASE}/api/knowledge/stats" \
  'has("total_facts")' \
  "expected total_facts field"

check "GET /api/knowledge/nodes?type=society returns nodes" \
  "${BASE}/api/knowledge/nodes?type=society" \
  'length > 0' \
  "expected society nodes"

# ── Shortlist ──
echo ""
echo "Shortlist"
check "GET /api/shortlist returns object with array" \
  "${BASE}/api/shortlist" \
  '.shortlist | type == "array"' \
  "expected shortlist array"

# ── Summary ──
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
TOTAL=$((PASS + FAIL))
if [[ $FAIL -eq 0 ]]; then
  printf "%s All %d tests passed\n" "$(green "✓")" "$TOTAL"
else
  printf "%s %d/%d passed, %d failed\n" "$(red "✗")" "$PASS" "$TOTAL" "$FAIL"
  echo ""
  printf "%b" "$ERRORS"
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Clean up
rm -f /tmp/oe_test_body.json

exit $FAIL
