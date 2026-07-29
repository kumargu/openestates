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

check_post() {
  local name="$1"
  local url="$2"
  local body_json="$3"
  local jq_expr="$4"
  local desc="$5"

  local http_code body
  http_code=$(curl -s -X POST -H "Content-Type: application/json" -d "$body_json" -o /tmp/oe_test_body.json -w "%{http_code}" "$url" 2>/dev/null) || {
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

# ── Discovery ──
echo ""
echo "Discovery"
check "GET /api/discovery returns product promise" \
  "${BASE}/api/discovery" \
  '.product_promise | type == "string" and length > 0' \
  "expected non-empty product_promise"

check "GET /api/discovery returns quotes" \
  "${BASE}/api/discovery" \
  '.quotes | type == "array" and length > 0 and (.[0] | has("text", "tone"))' \
  "expected quote text and tone"

check "GET /api/discovery returns shelves with cards" \
  "${BASE}/api/discovery" \
  '.shelves | type == "array" and length > 0 and (.[0] | has("id", "title", "quote", "description", "search_query", "receipt_copy", "cards")) and (.[0].cards | type == "array" and length > 0)' \
  "expected shelf metadata and property cards"

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

  check "Property detail has similar_properties array" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '.similar_properties | type == "array"' \
    "expected similar_properties array"

  check "Property detail omits legacy compatibility fields" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '(. | has("themes") | not) and (. | has("tradeoffs") | not) and (. | has("market_activity") | not)' \
    "expected legacy themes/tradeoffs/market_activity to be absent"

  check "Property evidence returns dynamic sections" \
    "${BASE}/api/properties/${FIRST_ID}/evidence" \
    '.property_id != null and (.entity_refs | has("property_entity_id", "society_entity_id", "area_entity_id")) and (.sections | type == "array" and length > 0)' \
    "expected property_id, entity_refs, and non-empty sections"

  check "Property detail includes canonical evidence read model" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '.evidence.property_id == .property.id and (.evidence.sections | type == "array" and length > 0)' \
    "expected property detail to expose evidence.sections for one-call UI rendering"

  check "Property evidence sections have render fields" \
    "${BASE}/api/properties/${FIRST_ID}/evidence" \
    '.sections | all(has("kind", "title", "summary", "priority", "confidence_pct", "source_types", "entity_ids", "items", "missing"))' \
    "expected each evidence section to be UI-renderable"

  check_post "Property evidence batch returns matching result" \
    "${BASE}/api/properties/evidence/batch" \
    "{\"property_ids\":[\"${FIRST_ID}\"],\"limit\":1}" \
    '(.results | length == 1) and (.results[0].sections | type == "array") and (.missing_property_ids | length == 0)' \
    "expected one evidence result and no missing ids"

  RERA_ID=$(curl -s "${BASE}/api/properties" 2>/dev/null | jq -r '[.[] | select(.decision_check_summary != null)][0].id // empty' 2>/dev/null || echo "")
  if [[ -n "$RERA_ID" ]]; then
    check "Property RERA dossier returns report fields" \
      "${BASE}/api/properties/${RERA_ID}/rera" \
      'has("source", "fact_sections", "compare_items", "complaint_sections", "document_sections", "timeline") and (.source | has("registered")) and (.fact_sections | type == "array")' \
      "expected source and flexible RERA fact sections"
  else
    echo "  $(green "✓") Skipping RERA dossier shape — no RERA-backed property in this bundle"
  fi
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

SOCIETY_FIXTURE=$(curl -s "${BASE}/api/properties" 2>/dev/null | jq -r '
  [.[].society_name // empty | select(length > 0)] as $names
  | ($names | unique) as $unique_names
  | ($unique_names[]) as $name
  | [($name | ascii_downcase | scan("[a-z0-9]{6,}"))] as $tokens
  | $tokens[]? as $token
  | select(($unique_names | map(ascii_downcase | gsub("[^a-z0-9]+"; " ") | split(" ") | index($token) != null) | map(select(.)) | length) == 1)
  | [$name, $token] | @tsv
' 2>/dev/null | head -n 1 || true)
if [[ -z "$SOCIETY_FIXTURE" ]]; then
  FAIL=$((FAIL + 1))
  ERRORS+="  FAIL: Search society recall fixture — no loaded society had a unique 6+ character token\n"
  printf "  %s Search society recall fixture — no loaded society had a unique 6+ character token\n" "$(red "✗")"
else
  IFS=$'\t' read -r FIRST_SOCIETY_NAME FIRST_SOCIETY_TOKEN <<< "$SOCIETY_FIXTURE"
  export FIRST_SOCIETY_NAME
  FIRST_SOCIETY_QUERY=$(jq -rn --arg q "$FIRST_SOCIETY_NAME" '$q|@uri')
  FIRST_SOCIETY_TYPO="${FIRST_SOCIETY_TOKEN:0:2}${FIRST_SOCIETY_TOKEN:3}"

  check "GET /api/search recalls loaded society" \
    "${BASE}/api/search?q=${FIRST_SOCIETY_QUERY}" \
    '(.results | length > 0) and any(.results[]; (.society_name | ascii_downcase) == (env.FIRST_SOCIETY_NAME | ascii_downcase))' \
    "expected society-name recall for ${FIRST_SOCIETY_NAME}"

  check "GET /api/search tolerates society typo" \
    "${BASE}/api/search?q=${FIRST_SOCIETY_TYPO}" \
    '(.results | length > 0) and any(.results[]; (.society_name | ascii_downcase) == (env.FIRST_SOCIETY_NAME | ascii_downcase))' \
    "expected fuzzy recall for ${FIRST_SOCIETY_TYPO} typo"
fi

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

check "GET /api/areas/tracker returns markets" \
  "${BASE}/api/areas/tracker" \
  '.total_areas > 0 and (.markets | type == "array" and length > 0) and (.markets[0] | has("id", "name", "listing_count", "avg_price_per_sqft", "price_min", "price_max", "bhks", "ready_to_move", "near_metro", "top_builder", "societies", "demand_score", "recent_searches"))' \
  "expected backend area tracker market summaries"

# ── Area Detail ──
echo ""
echo "Area Detail"

FIRST_AREA_ID=$(curl -s "${BASE}/api/areas" 2>/dev/null | jq -r '.[0].id // empty' 2>/dev/null || echo "")

if [[ -n "$FIRST_AREA_ID" ]]; then
  check "GET /api/areas/:id returns area" \
    "${BASE}/api/areas/${FIRST_AREA_ID}" \
    '.id != null and .name != null' \
    "expected area with id and name"

  check "Area detail has enrichment fields" \
    "${BASE}/api/areas/${FIRST_AREA_ID}" \
    'has("metro_access_summary", "traffic_summary", "livability_summary")' \
    "expected metro_access_summary, traffic_summary, livability_summary"
else
  echo "  $(red "✗") Skipping area detail tests — no area ID available"
  FAIL=$((FAIL + 1))
fi

AREA_404_CODE=$(curl -s -o /dev/null -w "%{http_code}" "${BASE}/api/areas/nonexistent-area-xyz" 2>/dev/null || echo "000")
if [[ "$AREA_404_CODE" == "404" ]]; then
  PASS=$((PASS + 1))
  printf "  %s GET /api/areas/bad-id returns 404\n" "$(green "✓")"
else
  FAIL=$((FAIL + 1))
  printf "  %s GET /api/areas/bad-id — expected 404, got %s\n" "$(red "✗")" "$AREA_404_CODE"
fi

# ── Society Search ──
# Society search is local-first. Offline enrichment can improve evidence, but the
# endpoint should not require a live LLM/API key.
echo ""
echo "Society Search"
SOC_SEARCH_CODE=$(curl -s -o /tmp/oe_test_body.json -w "%{http_code}" "${BASE}/api/societies/search?q=best%20societies%20Whitefield" 2>/dev/null || echo "000")
if [[ "$SOC_SEARCH_CODE" == "200" ]]; then
  PASS=$((PASS + 1))
  printf "  %s Society search returns 200\n" "$(green "✓")"
  check "Society search has results array" \
    "${BASE}/api/societies/search?q=best%20societies%20Whitefield" \
    '.results | type == "array"' \
    "expected results array"
elif [[ "$SOC_SEARCH_CODE" == "503" ]]; then
  FAIL=$((FAIL + 1))
  printf "  %s Society search returned 503; local search should not require live Gemini\n" "$(red "✗")"
else
  FAIL=$((FAIL + 1))
  printf "  %s Society search — unexpected HTTP %s\n" "$(red "✗")" "$SOC_SEARCH_CODE"
fi

# ── Search: Intent Parsing ──
echo ""
echo "Search Intent Parsing"
check "Budget parsed from '3BHK Whitefield under 2Cr'" \
  "${BASE}/api/search?q=3BHK%20Whitefield%20under%202Cr" \
  '.intent.budget_max != null and .intent.budget_max > 0' \
  "expected budget_max to be parsed"

check "BHK parsed correctly" \
  "${BASE}/api/search?q=3BHK%20Whitefield%20under%202Cr" \
  '.intent.bhk == 3' \
  "expected bhk=3"

check "Area parsed correctly" \
  "${BASE}/api/search?q=3BHK%20Whitefield%20under%202Cr" \
  '.intent.area == "Whitefield"' \
  "expected area=Whitefield"

check "Search results sorted by score descending" \
  "${BASE}/api/search?q=3BHK%20Whitefield" \
  '[.results[].match_score] | . as $scores | ($scores == ($scores | sort | reverse))' \
  "expected results sorted by match_score desc"

# ── Property Detail: Canonical Evidence ──
echo ""
echo "Property Detail (deep)"
if [[ -n "$FIRST_ID" ]]; then
  check "Detail evidence sections are sorted by priority" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '.evidence.sections | map(.priority) as $p | ($p == ($p | sort))' \
    "expected evidence sections sorted by priority"

  check "Detail evidence exposes source lineage" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '.evidence.sections | all((.source_types | type == "array") and (.entity_ids | type == "array"))' \
    "expected evidence sections to expose source_types and entity_ids"

  check "Detail omits source panel compatibility payload" \
    "${BASE}/api/properties/${FIRST_ID}" \
    'has("source_panels") | not' \
    "expected source_panels compatibility payload to be absent"

  check "Detail has recommendations envelope" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '.recommendations | has("status", "items")' \
    "expected recommendations status and items"

  check "Property has society or null" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '.society == null or (.society | has("id", "name"))' \
    "expected society to be null or have id+name"

  check "Property has area or null" \
    "${BASE}/api/properties/${FIRST_ID}" \
    '.area == null or (.area | has("id", "name", "city"))' \
    "expected area to be null or have id+name+city"
fi

# ── Shortlist ──
echo ""
echo "Shortlist"
check "GET /api/shortlist returns object with array" \
  "${BASE}/api/shortlist" \
  '.shortlist | type == "array"' \
  "expected shortlist array"

# ── Cross-cutting: Data Consistency ──
echo ""
echo "Data Consistency"
PROP_COUNT=$(curl -s "${BASE}/api/properties" 2>/dev/null | jq 'length' 2>/dev/null || echo "0")
if [[ "$PROP_COUNT" -gt 0 ]]; then
  PASS=$((PASS + 1))
  printf "  %s Properties count = %s (consistent)\n" "$(green "✓")" "$PROP_COUNT"
else
  FAIL=$((FAIL + 1))
  printf "  %s Properties count is 0 or unavailable\n" "$(red "✗")"
fi

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
