# RERA Live Parser Audit - 2026-07-28

Status: backend/parser audit after structured document and complaint parsing.

## Scope

Ran the upgraded `fetch_rera` parser against a small live Karnataka RERA sample:

- older/rich projects: Prestige Waterford, Godrej Splendour, Godrej United,
  Prestige Kew Gardens
- newer 2026 projects: Amberstone Vectra, Arvind Sylva, Purva Heritage
- fuzzy-name checks that did not match direct project search: Purva Palm Beach,
  Sobha Dream Acres, Assetz KVN Niwa Neo

The audit used the RERA search/detail flow and local detail-page cache where
available.

## Result

The parser now emits:

- `rera_document_manifest`
- document group counts for plans, approvals/NOCs, legal land docs, affidavits,
  buyer templates, and promoter financials
- `rera_complaint_summary_manifest`
- scoped complaint counts for project and promoter
- validation notes when tab-label complaint counts and parsed row counts differ

## Sample Findings

| Query | RERA match | Documents | Project complaints | Promoter complaints | Notes |
| --- | --- | ---: | ---: | ---: | --- |
| Prestige Waterford | PRESTIGE WATERFORD | 66 | 1 | 75 | Promoter row parse partial: 61 rows parsed |
| Godrej Splendour | Godrej Splendour | 84 | 2 | 58 | Project tab count present, rows not fully parsed |
| Godrej United | Godrej United | 30 | 3 | 3 | Row parse partial |
| Prestige Kew Gardens | Prestige Kew Gardens | 23 | 0 | 75 | Promoter row parse partial |
| Amberstone Vectra | AMBERSTONE VECTRA | 63 | 0 | 0 | Recent project, no complaints |
| Arvind Sylva | ARVIND SYLVA | 44 | 0 | 0 | Recent project, no complaints |
| Purva Heritage | Purva Heritage | 59 | 0 | 107 | Promoter row parse partial: 54 rows parsed |

Direct name search did not find:

- Purva Palm Beach
- Sobha Dream Acres
- Assetz KVN Niwa Neo

This reinforces that project-name search should be fallback only. Exact
registration number or listing-derived numeric IDs are needed for reliable
collection.

## Parser Fixes From Audit

The first live pass exposed false project complaint counts on recent projects.
Root cause: complaint tab extraction could include unrelated tables after the
tab body, and dated rows without complaint numbers were treated as complaints.

Fixes made:

- tab extraction now prefers balanced content-pane `<div id="...">` blocks
- complaint rows require complaint IDs like `CMP/...` or `COMP/...`
- complaint tab-label counts are preserved separately from parsed row counts
- partial row coverage adds `tab_count_and_row_count_disagree`

## Document Coverage

Real pages produce useful document groups:

- `plans`: site plans, floor plans, sanction plans, development plans, brochures
- `approvals_nocs`: commencement certificate, BESCOM, BWSSB, fire, airport,
  KSPCB, SEIAA, BMRCL, NOCs
- `legal_land`: EC, title/JDA/conversion/land documents
- `buyer_templates`: agreement for sale, allotment letter, sale deed proforma
- `promoter_financials`: PAN, balance sheet, P&L, auditor/ITR-like docs
- `affidavits`: Annexure 49, Form B, declarations

This is enough to support a future RERA document shelf without OCR-first
classification.

## Remaining Backend Work

- Improve direct discovery using exact `regNo`, listing entries, and district /
  taluk enumeration. Name search is not reliable enough.
- Improve complaint row parsing for accordion variants where tab count is known
  but only part of the row set is parsed.
- Keep tab-label count as the buyer-facing total until row coverage is exact.

