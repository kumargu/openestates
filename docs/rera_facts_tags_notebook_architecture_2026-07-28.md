# RERA Facts, Tags, Notebook Architecture - 2026-07-28

Status: research/design note only. No implementation in this pass.

## Decision

Use RERA facts before tags in the backend and tags before facts in the UI.

That sounds contradictory, but it is the right split:

- Backend/parser: RERA produces scoped, typed, source-backed facts.
- Product/API: selected facts become buyer-facing decision atoms.
- UI: tags organize those atoms into Notebook and Compare.

Tags should not be the source of truth. Tags are a buyer workflow index.

## Why This Matters

The localhost Notebook mock already has the right product shape:

```text
Property evidence row
  -> Save / Remember
  -> Notebook item with tag
  -> Compare row by tag
```

Compare explicitly does not parse prose. It builds rows from tags. Therefore
RERA cannot arrive as raw strings like "2 complaints" or "site plan available"
and expect the UI to infer meaning later.

RERA must arrive as structured facts with enough metadata for the UI to decide:

- how to display it
- whether it is saveable
- whether it is compareable
- which tag is suggested
- whether it is a concern, proof, question, or neutral fact
- what source tab, document, or complaint row supports it

## Canonical Fact Layer

The parser should first emit canonical records with source scope preserved.

Examples:

```text
rera_project_summary
rera_project_scale
rera_tower_schedule
rera_uploaded_document_manifest
rera_complaint_rows
rera_complaint_summary
rera_completion_details
```

These are not UI rows. They are durable records we can re-rank, re-tag, and
re-present later.

Important fields:

```text
fact_key
project_entity_id
promoter_entity_id
registration_number
scope: project | promoter | tower | unit_config | document | complaint
value
unit
source_tab
source_label
source_record_id
source_url
confidence
observed_at
parser_version
```

This avoids losing essential meaning. For example, project complaints and
promoter complaints are different facts even if both are "complaints".

## Decision Atom Layer

Promote only buyer-useful facts into decision atoms.

```text
DecisionAtom
  id
  property_id
  source_type: RERA
  canonical_fact_key
  label
  short_value
  detail
  buyer_tag
  mark: fact | concern | question
  source_label
  source_url
  scope
  compare_behavior: numeric | categorical | theme | evidence
  notebook_default: suggested | quiet | hidden
  privacy_policy
```

Example atoms:

| Canonical fact | UI atom | Suggested tag | Compare behavior |
| --- | --- | --- | --- |
| `project.rera_status` | `RERA approved` | Legal | categorical |
| `project.delivery_delay_months` | `Delivery extended by 18 months` | Delivery | numeric |
| `project_complaint_count` | `15 project complaints` | Legal | numeric/theme |
| `promoter_complaint_count` | `107 promoter complaints across projects` | Builder record | numeric/theme |
| `project.open_area_pct` | `52% open area declared` | Open space | numeric |
| `project.units_per_acre` | `92 homes per acre` | Project scale | numeric |
| `document.site_plan_count` | `Site plan available` | Plans | evidence |
| `document.affidavit_only_visible` | `Only affidavit visible in current RERA docs` | Legal | question |

## UI Placement

RERA should not be one giant card or table. It should be a source section with
small decision surfaces:

1. Snapshot strip
   - status
   - completion target
   - complaint summary
   - project scale
   - plan/document availability

2. Buyer question groups
   - Registration
   - Timeline
   - Project scale
   - Documents and plans
   - Complaints
   - Utilities and approvals
   - Builder record

3. Document shelf
   - grouped by document kind, not raw filename
   - source labels first, OCR second
   - previews only for plan/media-safe documents

4. Complaint digest
   - project complaints separate from promoter complaints
   - theme summary like reviews
   - row evidence available on expand

Every important row should be saveable. The save action should not create a
generic "RERA note"; it should create a typed Notebook item with source and tag.

## Tag Strategy

Keep the tag set small and buyer-language oriented.

Recommended RERA-capable tags:

- Legal
- Delivery
- Builder record
- Plans
- Project scale
- Open space
- Water
- Utilities
- Visit

Not every parser fact gets a visible tag. Some facts are used only to derive a
better atom. For example, land area and unit count can derive density; all three
can be available in evidence, but only density may be promoted into Compare.

## Notebook Behavior

When a buyer saves a RERA atom, the Notebook item should preserve:

```text
property_id
label
detail
tag
source_type = RERA
source_label
source_url
canonical_fact_key
source_record_id
scope
mark
compare_behavior
```

This makes handwritten notes and RERA facts share one Notebook stream while
still keeping machine-readable meaning.

Examples:

```text
15 project complaints
Tag: Legal
Source: RERA - Complaints on this Project
Scope: project
Compare: numeric/theme
```

```text
Site plan available
Tag: Plans
Source: RERA - Uploaded Documents - Approved Layout Plan
Scope: document
Compare: evidence
```

## Compare Behavior

Compare should have two inputs:

1. User-selected Notebook atoms.
2. Canonical auto rows when both homes have comparable RERA facts.

This avoids a weak outcome where Compare only shows facts the buyer manually
saved. For example, if two homes both have RERA scale facts, Compare can show:

```text
Project scale
Waterford: 52% open area, 72 homes/acre
Other home: 38% open area, 118 homes/acre
```

But if the buyer saved a specific concern, that concern should be elevated.

Rule:

```text
manual Notebook selection > promoted RERA atom > raw canonical fact
```

## Parser Requirements Before UI Expansion

Do this before making a polished RERA UI:

1. Align Karnataka RERA search fields with the live form:
   `project`, `firm`, `appNo`, `regNo`, `district`, `subdistrict`.
2. Parse uploaded-document labels into a document manifest before OCR.
3. Split complaint parsing into project scope and promoter scope.
4. Store complaint themes and complaint status groups.
5. Add lifecycle context for new projects where only affidavit documents are
   visible.
6. Add decision-atom metadata: tag, mark, compare behavior, scope, source label,
   privacy policy.

## Product Rule

The UI should answer buyer questions, not expose RERA bureaucracy.

Good:

```text
What is officially approved?
What changed in delivery?
What documents can I verify?
What have buyers complained about?
How dense is this project?
What should I save for comparison?
```

Avoid:

```text
Raw RERA field table
One unscoped complaints count
OCR-first document scanning
Tag-only backend facts
Legal labels that hide project vs promoter scope
```

