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

## Implemented foundations

- **L0 `rera_receipts`** stores content-addressed raw bytes, canonical URLs,
  capture times, crawl runs, and stable registration identities.
- **L1 `rera_source_records`** preserves raw labels and values in typed Parquet
  tables. Every accepted row requires a registration number, parser version,
  source locator, and exact receipt/capture lineage.
- Both assets are manual, parallel backfill roots. They do not alter the
  legacy serving bundle or property pages.

## Next: L2 canonical claims

Each claim will contain a typed subject, predicate, scalar or relation value,
effective time, assertion mode, source trust, validation state, visibility,
and receipt evidence. Source claims are immutable assertions; derived claims
will name their rule version and every input claim.

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

## Evidence migration comparison

Before the new report replaces the legacy RERA model, each backfill emits a
machine-readable comparison artifact. It is an audit tool, not a production
fallback and not a requirement that the old flattened data "wins."

For each registration and supported facet, it records:

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

## Cutover sequence

1. Backfill K-RERA listing, detail, QPR, and document receipts.
2. Extend L1 parsers for every receipt type and retain unknown rows.
3. Promote L2 claims only after lineage, identity, privacy, and fixture gates.
4. Materialize correlation products and a consumer-neutral evidence bundle.
5. Human-review evidence richness against the fixture corpus.
6. Replace `/property/:id/rera` with the dedicated report.
7. Remove `ReraInfo`, `ReraDossier`, legacy decision labels, and their
   flattened fact materializers once the new report is promoted.

Property Detail keeps only a compact link to the report until the dedicated
report proves the data is rich enough for carefully selected reuse.
