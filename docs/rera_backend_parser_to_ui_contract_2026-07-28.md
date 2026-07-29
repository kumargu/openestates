# RERA Backend Parser To UI Contract - 2026-07-28

Status: design note only. This connects parser/backend work to the Notebook and
Compare UI direction.

## Core Point

The RERA UI cannot be intelligent unless the backend stops emitting mostly flat
source-shaped facts. The UI should not infer whether `21 complaints` means
project complaints, promoter complaints, disposed complaints, or unrelated
builder history. The parser and backend need to produce scoped, normalized,
typed records first, then promote a smaller set of buyer-facing decision facts.

The end-to-end contract should be:

```text
RERA HTML / docs
  -> raw source snapshots
  -> normalized RERA tables
  -> validation + derived facts
  -> promoted decision facts
  -> SourceItem / DecisionAtom API
  -> Property evidence, Notebook, Compare
```

## Current Gap In Repo

Current shape:

- `pipeline/skills/fetch_rera.py` fetches listing/search/detail pages and emits
  `SourcedFact`s.
- `backend/src/assets/rera.rs` stores listing rows plus `detail_facts`.
- `backend/src/assets/skill_facts.rs` stores generic `SkillFactRecord` rows:
  `entity_id`, `fact_key`, `value_json`, `confidence`, `source_type`,
  `source_url`, `learned_at`, etc.
- `backend/src/routes/properties.rs` projects a compact `ReraInfo` object to
  frontend fields like `registration_number`, `total_units`, `open_area_pct`,
  `complaints_count`, and `complaints_resolved_pct`.
- `frontend/src/components/evidence/ReraProjectFacts.tsx` groups the compact
  `ReraInfo` into Registration, Schedule, Project scale, and Buyer checks.

This is useful, but insufficient for the dynamic Notebook/Compare idea because
it loses:

- nested complaint scope: promoter vs project
- complaint themes and row status
- uploaded-document source field labels
- document kind groups beyond a few plan counts
- lifecycle context for recent projects with only affidavits visible
- source section/tab evidence
- suggested notebook tags
- compare row type and behavior
- evidence privacy / buyer visibility policy

## Parser Architecture Required

The RERA parser should become table-aware and section-aware. It should not go
directly from HTML labels to final product facts.

### L0 Raw Snapshot

Persist raw source material:

```text
rera_project_detail_snapshots
  registration_number
  ack_number
  numeric_detail_id
  project_name
  promoter_name
  fetched_at
  source_url
  detail_html_sha256
  raw_html_storage_key
```

Raw snapshots allow reprocessing when parser logic improves.

### L1 Normalized Source Tables

Parse the RERA page into tables that preserve RERA meaning:

```text
rera_project_summary
rera_project_registration_extensions
rera_project_scale
rera_tower_schedule
rera_unit_configuration_schedule
rera_uploaded_document_manifest
rera_enquired_document_manifest
rera_complaint_rows
rera_complaint_summary
rera_development_work_schedule
rera_completion_details
```

This is the missing backend layer. It should exist even before all fields are
promoted to UI.

### L2 Promoted Decision Facts

Promote only buyer-useful facts:

```text
project.land_area_acres
project.open_area_pct
project.unit_count
project.tower_count
project.max_floor_count
project.units_per_acre
project.rera_status
project.rera_completion_target
project.rera_original_completion_target
project.delivery_delay_months
project_complaint_count
project_complaint_open_count
project_complaint_disposed_count
project_complaint_theme_counts
promoter_complaint_count
promoter_complaint_theme_counts
document.site_plan_count
document.sanction_plan_count
document.floor_plan_count
document.noc_count
document.legal_land_doc_count
document.affidavit_only_visible
```

Keep source-shaped `rera_*` aliases only for compatibility.

## Normalized Record Shapes

### Uploaded Document Manifest

Required because document labels are already meaningful and should drive UI
before OCR.

```text
project_entity_id
registration_number
numeric_detail_id
source_tab: uploaded_documents | enquired_documents
source_section_heading
source_field_label
uploaded_filename
href
doc_id
document_kind
document_group
buyer_visibility
preview_policy
confidence
observed_at
parser_version
```

Example document groups:

- `plans`
- `legal_land`
- `approvals_nocs`
- `buyer_templates`
- `promoter_financials`
- `affidavits`
- `other`

Example visibility policy:

- `preview_allowed`: site plan, sanction plan, floor plan, section plan,
  development plan, brochure
- `list_only`: EC, title, JDA, khata, NOCs, commencement certificate
- `private_or_sensitive`: PAN, financial statements, customer templates,
  affidavits

### Complaint Rows

Required because UI must distinguish project complaints from promoter history.

```text
project_entity_id
promoter_entity_id
registration_number
scope: project | promoter
complaint_number
complainant_name_hash_or_redacted
complaint_date
complaint_subject
complaint_project_name
complaint_promoter_name
status_raw
status_group: disposed | under_enquiry | posted_for_orders | other
order_by
theme_tags
observed_at
parser_version
```

Do not expose complainant names in primary UI. They can be redacted or omitted.

### Complaint Summary

```text
project_entity_id
promoter_entity_id
scope
total_count_from_tab_label
row_count_parsed
disposed_count
open_count
theme_counts_json
sample_subjects_json
confidence
validation_notes
```

The count from the tab label and parsed row count should both be stored. If they
disagree, lower confidence and add validation notes.

### Decision Fact Metadata

The generic `SkillFactRecord` is not enough for Notebook/Compare by itself. We
need either an extended record or a sidecar metadata table:

```text
fact_key
source_record_id
source_tab
source_section
source_label
scope
buyer_tag
compare_behavior
display_group
privacy_policy
source_document_ref
validation_notes
```

This metadata is what lets the UI show:

```text
15 project complaints
RERA · Complaints on this Project
Tag: Legal
Compare: numeric/theme
```

instead of a generic source row.

## Parser Rules That Matter

### Search And Discovery

Use live-form-compatible parameters:

```text
project
firm
appNo
regNo
district
subdistrict
```

Exact registration number search should be the preferred lookup when available.
District+subdistrict search should be used for coverage. Name search should be
fallback.

### Section Boundaries

Do not scan the whole detail page for documents and complaints. Boundaries
matter:

- `menu2` -> uploaded documents
- `menu4` -> enquired documents
- `menu5` -> appeals / FNH
- `menu-complaints` -> complaint tabs
- `menu-comp` -> promoter complaints
- `menu-comp2` -> project complaints

The current all-page scan can bleed from complaints into tower details and
documents. That creates wrong counts and wrong manifests.

### Document Classification

Classification should use field label plus filename:

```text
document_kind = classify(source_field_label, uploaded_filename, href)
```

Field label should usually outrank filename. Example:

- field `Approved Layout Plan` + generic filename -> `site_plan`
- field `BESCOM` -> `noc`
- field `Encumbrance Certificate` -> `encumbrance_certificate`
- field `Affidavit (Annexure - 49)` -> `affidavit`

OCR only runs after this step for selected kinds.

### Complaint Theme Classification

Start deterministic, not LLM-only:

- delay / possession
- refund / cancellation
- compensation / interest
- additional demand / holding charges
- agreement / registration
- plan / amenity / parking mismatch
- OC / completion certificate
- other

LLM summarization can later generate friendlier copy, but deterministic tags
should drive Compare.

### Validation

Examples:

- `open_area_pct = open_area_sqm / land_area_sqm * 100`
- compare RERA tab complaint count vs parsed row count
- compare tower count summary vs tower schedule row count
- compare unit count summary vs tower/unit schedule totals
- mark recent projects as `document_lifecycle_state = initial_affidavit_only`
  when only affidavit is visible soon after approval

## Backend API Contract For UI

The frontend needs more than current `ReraInfo`.

### Property Evidence API

Add structured RERA blocks, either inside `PropertyEvidenceResponse.sections` or
as a sibling object:

```text
rera_decision_sections: [
  {
    id: "rera_snapshot",
    atoms: [...]
  },
  {
    id: "rera_documents",
    groups: [...]
  },
  {
    id: "rera_complaints",
    project_summary: ...
    promoter_summary: ...
  }
]
```

### Decision Atom

Suggested serving type:

```text
DecisionAtom {
  id
  entity_id
  source_type
  source_label
  source_url
  fact_key
  label
  value
  unit
  display_value
  tag
  scope
  confidence_pct
  compare_behavior
  notebook_default: saved_fact | none
  source_ref
  evidence_group
  privacy_policy
  learned_at
}
```

The same atom can render on property page, Notebook, and Compare.

### Compare Rows

The backend should provide canonical compare rows, not force frontend to infer
everything from arbitrary source items:

```text
CompareRow {
  row_id
  tag
  label
  row_type
  values_by_home_id
  sort_priority
  source_refs
}
```

Rows can be produced from:

- canonical facts present on multiple homes
- user-pinned Notebook atoms
- shared Notebook tags
- warning facts where one home has risk and another is unknown

## How This Changes Existing `ReraInfo`

Keep `ReraInfo` for the compact panel, but evolve it:

Current:

```text
complaints_count
complaints_resolved_pct
```

Needed:

```text
project_complaints_count
project_complaints_open_count
project_complaints_disposed_count
project_complaint_themes
promoter_complaints_count
promoter_complaint_themes
document_groups
document_lifecycle_state
```

Current:

```text
total_units
open_area_pct
units_per_acre
```

Keep these, but prefer canonical keys:

```text
project_unit_count
project_open_area_pct
project_units_per_acre
```

## Implementation Sequence

### Phase 1: Parser Correctness

- Align search POST fields with live RERA form.
- Add table/section parser for `menu2`, `menu4`, `menu-comp`, `menu-comp2`.
- Emit document manifest records.
- Emit scoped complaint rows and summaries.
- Add parser fixtures from cached detail HTML for:
  - Purva Palmbeach
  - Prestige Waterford
  - one recent affidavit-only project
  - one project with floor-plan label
  - one project with NOCs

### Phase 2: Backend Materialization

- Add assets for RERA detail snapshots, normalized tables, document manifests,
  complaint summaries, and promoted RERA decision facts.
- Preserve raw HTML/document refs for reprocessing.
- Add compatibility emission for existing `rera_*` facts.

### Phase 3: API Shape

- Extend property evidence API with `DecisionAtom`s or equivalent metadata.
- Extend `ReraInfo` with scoped complaint and document summary fields.
- Add compare-row construction from promoted facts.

### Phase 4: UI

- Replace static RERA fact grid with snapshot + buyer-question cards.
- Add save/compare actions to facts.
- Add Notebook saved-fact support.
- Add Compare rows from canonical RERA facts and Notebook tags.

## Non-Negotiables

- UI must not parse raw RERA text.
- Compare must not infer complaint scope from a label string.
- OCR must not be the first document classifier.
- Complaint counts must always be scoped.
- Sensitive documents should not become media previews by default.
- Recent projects need lifecycle-aware missing-document language.

