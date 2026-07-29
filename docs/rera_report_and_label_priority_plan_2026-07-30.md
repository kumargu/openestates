# RERA Report And Label Priority Plan - 2026-07-30

Status: design note. No implementation in this pass.

## Decision

Build a dedicated RERA report page later, but keep the property detail page
minimal.

Property detail should show:

- one `RERA` action
- at most 3-4 high-signal labels near the property context
- a compact RERA popup with useful tags

RERA report page should show:

- full calm dump for one focused property
- copyable facts
- no table-heavy layout
- no raw/promoted gap language in buyer mode
- internal/admin-only visibility for raw facts that were not promoted

Do not create a broad `Diligence` workspace tab yet. Compare already covers
cross-home diligence. The deep RERA report is a single-property legal/project
fact page, closer to a plan/report page than a general workspace tab.

## Where Intelligence Should Sit

The frontend should not decide which RERA facts matter. It should render a
ranked fact view from the backend.

The clean pipeline is:

```text
raw RERA cache
  -> canonical promoted facts
  -> label candidates from config
  -> isolated backend priority policy
  -> surface-specific API views
  -> UI renders tags/report sections
```

This keeps the UI calm and prevents frontend branches like:

```text
if complaints_open_count == 0 then show "No open complaints"
if noc_count > 0 then show "NOCs available"
```

Those are product decisions. They belong in fact promotion + label policy.

## Generic Promotion Gaps Found From RERA

The RERA parser can already produce facts that are more specific than the
current compact serving view. Promotion should be generic across all projects,
not project-specific. If a raw RERA run has these fields, the asset promotion
step should carry them into serving facts with preserved scope and provenance.

| Raw fact | Why promote |
| --- | --- |
| `rera_project_complaints_count` | separates project complaints from aggregate complaints |
| `rera_project_complaints_disposed_count` | enables `3 closed` / closure labels |
| `rera_project_complaints_open_count` | enables `No open complaints` or `N open complaints` |
| `rera_promoter_complaints_count` | builder-scope relation, not same as project issue |
| `rera_promoter_complaints_disposed_count` | builder record quality |
| `rera_promoter_complaints_open_count` | current unresolved builder-side issue count |
| `rera_complaint_summary_manifest` | enables complaint theme tags |
| `brochure_asset_count` | report/document availability |
| `rera_noc_document_count` | `NOCs available` |
| `rera_affidavit_document_count` | legal document availability |
| `rera_document_manifest` | source for grouped report document sections |

For a project like Godrej Air, this generic promotion would unlock:

```text
No open complaints
3 complaints closed
Complaint themes: agreement/payment, amenities, builder conduct, cancellation, compensation
NOCs available
OC available
Agreement draft available
Brochure available
```

## Label Priority Policy

Create a small isolated backend module later, not scattered UI logic. It should
rank labels from generic promoted facts for every project. Working name:

```text
backend/src/decision_labels/priority.rs
```

Input:

```text
property_id
society_id
promoted facts
label definitions from app/config/dag/rera_decision_labels.json
surface profile
```

Output:

```text
DecisionLabelView
  primary_labels
  popup_labels
  report_sections
  hidden_positive_count
  hidden_admin_gap_count
```

The policy should be deterministic and config-driven.

## Promotion Algorithm

The promotion layer should be a generic typed transformation:

```text
for each RERA project snapshot:
  preserve scoped raw facts:
    project complaints
    promoter complaints
    complaint themes
    document counts
    document manifest
    project scale
    timeline
    legal declarations

  normalize each fact into:
    entity_id
    scope
    fact_key
    value
    unit
    source_tab
    source_url
    confidence
    learned_at

  emit only validated facts into serving
```

Rules:

- Never special-case a property name.
- Preserve project versus promoter scope.
- Preserve document kind rather than only filename.
- Preserve complaint status counts separately from total complaints.
- Promote raw manifests only when they are parseable and bounded enough for
  serving.
- Let the label policy decide what to show per surface.

## Surface Profiles

Use one candidate pool, but different caps by surface.

| Surface | Cap | Behavior |
| --- | ---: | --- |
| Property detail inline | 3-4 | risk/caution first; hide positives unless no cautions |
| RERA popup | 8-14 | compact tags; include risk, project shape, docs, quiet positives |
| RERA report | uncapped curated sections | full calm dump; copyable facts; grouped by buyer question |
| Compare | only comparable facts | numeric/category rows, not source-document inventory |
| Notebook suggestions | selective | only facts useful to remember or compare |

## Ranking Rules

Start with simple scoring. Keep weights in config if they start changing often.

```text
score =
  severity_weight
  + actionability_weight
  + outlier_weight
  + scope_weight
  + evidence_weight
  + surface_fit_weight
  - duplicate_penalty
```

Suggested behavior:

- Risk beats caution.
- Caution beats neutral project-shape facts on property detail.
- Project-specific facts beat builder-wide facts on a property page.
- Builder-wide facts are useful in Compare and RERA report.
- Positive proof is hidden on property detail when there are cautions.
- Positive proof can show in popup/report if it adds context.
- Counts matter only past thresholds. `1 complaint` is usually hidden; `3+` can show; `10+` should show.
- “No issue” labels show only when they answer a buyer question, such as `No land litigation` or `No open complaints`.
- Document inventory should collapse into useful labels: `Site plan available`, `NOCs available`, `Agreement draft available`.

## Too Many Tags

When many labels qualify:

1. Keep highest severity labels.
2. Cap per family so one area does not dominate.
3. Prefer one representative per family:
   - complaints
   - delivery/timeline
   - land/legal
   - project shape
   - documents
   - builder record
4. Collapse related positives:
   - `OC available`, `NOCs available`, `Fire NOC available` can become `Approvals available` in compact surfaces.
5. Keep the full detail for the RERA report page.

## Too Few Tags

When there are no risks/cautions:

1. Show nothing on property detail, or one quiet positive if useful.
2. Popup can still show a basic fact set:
   - registered
   - RERA number
   - no land litigation
   - project shape
   - site/plan document availability
3. RERA report should still render the full fact sheet.

## RERA Report Page Shape

Buyer mode:

```text
RERA

Godrej Air
PRM/KA/RERA/1251/446/PR/170819/000006   Open

Key facts
8 month delay   3 project complaints   10% open area
No land litigation   Site plan available

Project shape
487 homes   5.3 acres   92 homes/acre
1 parking/home   8 towers   16 floors

Complaints
3 total   3 closed   0 open
Themes: cancellation, agreement/payment, amenities

Documents
Site plan available
NOCs available
Agreement draft available
Brochure available
```

Admin/internal mode can show:

```text
Raw facts not promoted
Promoted facts
Skipped labels and why
Source artifact manifest
```

Do not expose “not promoted”, “raw cache”, “serving gap”, or parser details in
buyer mode.

## Next Backend Step

Before building the RERA report UI:

1. Promote the missing complaint split, complaint theme manifest, and document
   counts from raw RERA facts into serving facts.
2. Extend `rera_decision_labels.json` for:
   - open/closed complaint status
   - project versus promoter complaint scope
   - complaint theme labels
   - grouped document availability
3. Add an isolated priority module that returns surface-specific views.
4. Keep the property detail page consuming only the compact view.
