# RERA evidence graph rebuild

## Purpose

Replace the flattened RERA model with a versioned evidence pipeline:

```text
immutable receipts -> typed source records -> scoped claims
-> deterministic evidence products -> serving projection -> RERA report
```

The graph reports what the registry and parties recorded, when they recorded
it, and where records differ. It does not produce legal, safety, title, or
investment verdicts.

## Implemented pipeline

- **L0 `rera_receipts`** stores content-addressed raw bytes, canonical URLs,
  capture times, crawl runs, and stable registration identities. Scoped runs
  reuse provenance-complete captures unless refresh is explicitly forced.
- **L1 `rera_source_records`** preserves raw labels and values in typed Parquet
  tables. Every accepted row requires a registration number, parser version,
  source locator, and exact receipt/capture lineage. Exact catalog relations
  are stored separately from registration facts.
- **L2 `rera_claims`** materializes typed, registration-scoped public claims
  with assertion mode, trust, validation state, visibility, and complete
  receipt evidence.
- **Serving projection** stores RERA evidence inside the existing versioned
  search bundle. It contains entities, claims, timelines, quarterly series,
  inventory reconciliation, coverage, and a public source index.
- **Rust API** serves `GET /api/properties/{id}/rera` as `{ evidence, surface }`.
  Property Detail receives only `rera_report_ref` and no longer reconstructs
  a flattened dossier.
- **Dedicated report** renders only configured, present sections and provides
  keyboard-accessible source drill-down. Property Detail keeps one link to it.

Rules:

- Registration, phase, tower, and unit claims stay registration-scoped.
- A promoter declaration is displayed as a declaration, never independent
  verification of absence or safety.
- Complaint allegations and authority orders remain distinct assertion modes.
- No accepted claim may lack complete L1 and L0 lineage.
- Restricted financial or personal data can exist in the restricted lake but
  never reaches a public projection.

## Evidence correlations, not scores

Correlation products are deterministic views over accepted claims. They show
relationships and discrepancies without compressing them into a score or
recommendation.

| Product | Inputs | Reader-facing insight |
| --- | --- | --- |
| Filing and milestone timeline | registration, completion, extensions, approvals | Shows the sequence of recorded dates and later changes. |
| QPR progress series | dated quarterly progress claims | Shows reported progress over time, including missing periods. |
| Inventory reconciliation | tower, unit, parking, and aggregate claims | Shows where totals agree, differ, or cannot be reconciled. |
| Document chronology | approvals, certificates, and linked documents | Connects each document to the reported project stage. |
| Complaint and order chronology | allegations, case metadata, authority orders | Separates allegations from orders and preserves timing. |
| Water/service matrix | dated borewell, STP, water, and service declarations | Compares declarations by filing date without treating them as proof of service. |
| Conflict ledger | same predicate with differing accepted values | Identifies the records that differ and links to both receipts. |
| Coverage and freshness | expected source sections and latest filing period | Makes the report's evidence breadth explicit without exposing pipeline jargon. |

Every product must expose its rule version and input claim IDs. Missing input
means the product is omitted or marked partial; it never becomes a favourable
default.

## Ten-project proof

An unpromoted scoped run proved the pipeline over ten registrations while
retaining the current knowledge-graph snapshot in the same serving bundle:

```text
run_id: ea929fb4-0a0e-4274-9607-36a88d97e0bf
receipts: 11
source records: 152
claims: 597
RERA serving rows: 10
promoted: false
```

Observed richness:

| Project | Claims | Inventory configurations | QPR points | Documents | Differences |
| --- | ---: | ---: | ---: | ---: | ---: |
| Godrej Lakeside Orchard | 107 | 7 | 3 | 7 | 1 |
| Birla Tisya | 69 | 4 | 3 | 4 | 0 |
| SNN Clermont | 336 | 55 | 0 | 0 | 0 |
| Seven other registrations | 85 | 6 | 0 | 0 | 0 |

Godrej demonstrates the intended neutral correlation behavior: 698 declared
homes reconcile exactly, while 65,100 m² project carpet and 65,096 m²
inventory carpet remain separate source claims with a displayed 4 m²
difference. No score or legal/investment conclusion is produced.

Eight of the ten registrations map to societies in the current runtime
catalog. Godrej Lakeside Orchard and Prestige Jindal City Phase-I remain
data-only until those entities enter the canonical catalog; the pipeline does
not fabricate property listings for them.

## Remaining expansion

The current parser deliberately records partial coverage. Next source-depth
work should extend the same typed path for complaints and authority orders,
extensions, approvals beyond QPR document metadata, and allowlisted financial
declarations. Unknown source rows must remain retained or explicitly covered
by a source warning.

When comparing old and new output, use a machine-readable audit artifact with:

```text
registration_id
facet
legacy_value / legacy_fact_ids
new_source_values / source_record_ids
new_claim_ids
classification
resolution
```

Classifications are `legacy_missing`, `new_missing`, `same_value`,
`normalization_change`, `scope_correction`, `source_changed`, `conflict`, or
`requires_review`. Counts are reported by parser version and receipt capture.
Any difference that changes a buyer-facing value needs an explicit resolution;
the new evidence pipeline may correctly retain multiple values where the old
model had flattened one.

The old flattened value is diagnostic input only; it is never authoritative
and no compatibility adapter is required.

## Promotion sequence

1. Extend L1 coverage while preserving all unknown rows and source warnings.
2. Add complaint/order and extension fixtures with privacy assertions.
3. Run the scoped proof against existing catalog societies and review the
   resulting report at desktop and mobile widths.
4. Compare supported facets against the old flattened fields and explain each
   difference without requiring parity.
5. Run full Rust, Python, frontend, deterministic, privacy, and smoke gates.
6. Promote the complete search bundle atomically; do not publish a separate
   RERA pointer.
7. Remove the remaining flattened `ReraInfo` and decision-label consumers when
   unrelated Home Plan work has migrated to durable generic facts.

Property Detail keeps only a compact link to the report until the dedicated
report proves the data is rich enough for carefully selected reuse.
