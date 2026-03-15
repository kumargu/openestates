# Phase 0 Gate Summary

**Date:** 2026-03-13 (Day 75)
**Gate Decision:** PASS
**Final Metrics:** 207 PASS, 5 WARN, 0 FAIL

## Improvement Timeline

| Day | PASS | WARN | FAIL | Key Changes |
|-----|------|------|------|-------------|
| 73  | 178  | 34   | 0    | Baseline validation harness created |
| 74  | 200  | 12   | 0    | Deduped 101 facts, normalized RERA, enriched market pricing on 8 societies |
| 75  | 207  | 5    | 0    | Fixed total_units key mismatch, Google reviews on Sarang, search + seller verification |

## Verification Results

### Data Validation (validate_phase0.py)
- 10/10 properties: 0 FAIL
- 9/10 properties: 0 WARN (clean across all checks)
- 1/10 property (NBR Group): 5 WARN (accepted exception, see below)

### Search Verification (verify_search.py)
- 10/10 properties found via NL search queries
- Each property found in 3/3 query variants (BHK+area, society name, BHK+society)
- Gate: PASS

### Seller Matching (verify_sellers.py)
- 7/7 seller-property pairs verified
- All 4 checks per pair passed: seller API, property link, interest POST, interest count
- 6/10 validation properties have sellers (remaining 4 are discoverd-only, no seller expected)
- Gate: PASS

## Accepted Exceptions

All 5 remaining WARNs are on **NBR Group Apartments Near Wipro Sarjapur Road** (1 property):

| WARN | Reason |
|------|--------|
| kg_society_rera | Only rera_registered fact, no rera_number or rera_ack_number |
| kg_society_project_status | No project_status fact |
| kg_society_total_units | No total_units or rera_total_units fact |
| reviews | Reddit facts present but no Google sentiment (Gemini could not find Google reviews data) |
| trust_badges_rera | rera_registered value is not true (insufficient evidence) |

**Root cause:** NBR Group was discovered via Gemini live discovery. It has 17 facts (vs 44-66 for RERA-sourced societies). No RERA registration could be verified, and Google has limited review data for this project.

**Mitigation:** This is a known limitation for Gemini-discovered societies without RERA evidence. The system correctly assigns lower confidence and "verification pending" status. Phase 1 enrichment will attempt to find additional data sources.

## Phase 1 Readiness

Phase 0 validates that the full pipeline works end-to-end for 10 representative properties:

- **Seed data**: All properties have complete required fields, hero images, society mappings
- **Knowledge graph**: 9/10 societies have 44-66 facts with RERA, builder, pricing, reviews
- **Provenance**: All facts have complete source_type, skill_id, and learned_at
- **Trust badges**: 9/10 societies are RERA badge eligible
- **Search**: All 10 properties discoverable via NL queries (BHK+area and society name)
- **Seller matching**: All seller-property links verified, interest flow functional
- **Embeddings**: All 10 properties have computed embeddings for semantic search

Phase 1 can now scale to 100 societies with confidence that:
1. The enrichment pipeline (RERA, Reddit, Google reviews, market pricing) works reliably
2. Data quality checks catch real issues (the NBR Group WARNs are genuine data gaps)
3. Search and seller APIs serve the data correctly
4. The validation harness provides a repeatable quality gate
