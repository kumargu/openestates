# RERA Deep Extraction Design

Status: draft design for review before implementation.

## Why This Matters

RERA is not just a registration checkbox for OpenEstates. For several projects,
the Karnataka RERA detail pages expose project facts that are either missing or
inconsistent on listing portals: land extent, open area, inventory count,
tower/floor schedules, parking schedules, external development work, plan
documents, and delivery history.

For Prestige Waterford, the existing repo scraper fetched the project page and
emitted 24 facts, but the current parser missed fields that are visible in the
same RERA HTML:

- total land area: `66823 sqm`, which is about `16.51 acres`
- total open area: `59380 sqm`, which is about `88.86%` of the project land
- inventory: `689` apartments
- structure: description says `5 Blocks with 7 wings`, and the tower section
  has `7` tower rows
- parking for sale: `106` parks, `1458 sqm`
- tower details: each tower carries floors, basements, units, and parking
- plan documents: development plan, site plan, section drawings, sanction plan,
  and brochure links are present as downloadable RERA attachments

This means the immediate problem is not that RERA lacks the information. The
problem is that our extractor is still a shallow label parser. It reads some
top-level labels but does not yet understand RERA sections, tables, repeated
plan blocks, tower schedules, or document attachments.

## Additional RERA Sampling

I sampled six more Karnataka RERA detail pages after Waterford to check whether
the data model is stable across builders and project sizes. The sample covered:

- Sobha Insignia
- Prestige Raintree Park
- Godrej Tiara
- Brigade Lakecrest
- Sumadhura Elysium Phase-I
- Assetz Muse & Maison

The exact fields vary, but the useful project spine is consistent enough to
model. Every sampled page had land area, open area, inventory/unit count, tower
or wing schedules, parking evidence, plan documents, and water/STP/borewell
terms or documents.

| Project | Acres | Open % | Units | Tower rows | Max floors | Parking evidence | Plan/STP/Borewell docs | Water/STP/Borewell evidence |
| --- | ---: | ---: | ---: | ---: | ---: | --- | ---: | --- |
| Prestige Waterford | 16.51 | 88.86 | 689 | 7 | 24 | yes | 7 | yes |
| Sobha Insignia | 0.98 | 79.04 | 33 | 1 | 8 | yes | 6 | yes |
| Prestige Raintree Park | 27.84 | 65.19 | 1520 | 18 | 19 | yes | 2 | yes |
| Godrej Tiara | 4.93 | 83.49 | 346 | 3 | 30 | yes | 1 | yes |
| Brigade Lakecrest | 6.92 | 85.59 | 604 | 4 | 22 | yes | 13 | yes |
| Sumadhura Elysium Phase-I | 5.18 | 84.92 | 319 | 3 | 18 | yes | 15 | yes |
| Assetz Muse & Maison | 2.14 | 79.45 | 128 | 1 | 16 | yes | 14 | yes |

This sample also shows why the UI must be fact-driven and sparse. Some projects
have rich plan documents; some have only one or two obvious plan artifacts.
Some expose simple water-source text such as `BoreWell,BWSSB`; others expose
STP drawings, borewell/water NOCs, or feasibility reports. That variation should
not block the model. The property page should render sections only when useful
facts exist, and compare/search should use promoted facts with clear evidence
rather than showing empty placeholders.

The sample also exposed current extractor gaps:

- Waterford has 689 units and 7 tower rows in HTML, but the current extractor
  misses units and towers.
- Waterford coordinate parsing is unsafe: the current parser can collapse
  DMS-like boundary coordinates into `12.0,77.0`.
- Open area is present in all sampled pages, but the current extractor does not
  emit it as a fact.
- Water/STP/borewell evidence often appears as documents, not only as scalar
  HTML fields, so the document manifest must be part of the first-class model.

## Current Repo Shape

The current flow is already close to the right architecture:

```text
RERA portal fetch
  -> pipeline/skills/fetch_rera.py
  -> pipeline/collect_asset_sources.py detail_facts
  -> backend/src/assets/rera.rs Parquet materialization
  -> rera_legal_facts
  -> kg_society_view
  -> serving bundle
  -> Rust API + UI
```

`pipeline/skills/fetch_rera.py` can fetch the Karnataka listing page, search a
project, fetch the multi-tab detail page, and emit `SourcedFact`s. The detail
model currently covers core registration, dates, land area, costs, basic
parking, escrow, litigation, complaints, builder track record, and coordinates.

`pipeline/collect_asset_sources.py` already scopes detail collection to selected
source entities, writes facts with `triggered_by=asset_dag`, and duplicates
facts onto alias entity IDs. That is the right place to keep this work scoped
and idempotent.

`backend/src/assets/rera.rs` already stores the monthly RERA snapshot as Parquet
and has a `detail_facts/part-00000.parquet` path. It also merges newer detail
facts over the base listing facts. This means richer RERA extraction can enter
the existing lake and serving-bundle path without a new runtime database.

The weak points are:

- fact names are still mostly source-shaped, such as `rera_total_land_area_sqm`
  and `rera_num_towers`
- product evidence sections still include a hardcoded RERA source panel in
  `backend/src/routes/properties.rs`
- the parser uses text-label extraction where table-aware extraction is needed
- RERA documents are discovered as links, but not yet stored as first-class
  document or media artifacts
- image/PDF preview generation is not available in this checkout today

## Goals

- Extract high-confidence RERA project facts from sections and tables, not only
  from flat labels.
- Store source-native pages, documents, normalized rows, and derived facts in
  appendable Parquet/S3-ready lake paths.
- Promote only buyer-useful facts into search and compact UI surfaces.
- Keep the full RERA file available as evidence for property pages, comparison,
  and future audit workflows.
- Keep the Rust request path local: no RERA fetch, PDF parsing, OCR, or image
  compression while a buyer is waiting.
- Drive fact semantics, search promotion, and UI placement from config.

## Non-Goals

- Do not build legal due diligence or document validation.
- Do not replace listing or transaction price data with RERA cost data.
- Do not OCR every drawing in the first slice.
- Do not parse floor-plan geometry into room dimensions in the first slice.
- Do not expose every RERA label in buyer UI.
- Do not make RERA the only source of project truth. RERA should be the
  strongest regulatory input, but the buyer-facing fact model should also accept
  builder sites, seller proof, Google, transaction records, and resident signals.

## Extraction Model

We should split RERA extraction into three levels.

### L0: Raw Source Snapshot

Store the original evidence exactly as fetched:

```text
data/lake/raw/source=rera/state=ka/dt=YYYY-MM-DD/project_id=<id>/detail.html
data/lake/raw/source=rera/state=ka/dt=YYYY-MM-DD/project_id=<id>/listing_result.json
data/lake/raw/source=rera/state=ka/documents/sha256=<hash>/original.pdf
data/lake/raw/source=rera/state=ka/documents/sha256=<hash>/metadata.json
```

The raw snapshot should include:

- project registration number and acknowledgement number
- numeric RERA portal ID
- source URL and fetch timestamp
- HTML content hash
- document link manifest with filename, RERA `DOC_ID`, source label, MIME type,
  content hash, content length, and storage key

The raw layer is for auditability and reprocessing. It should not be rendered
directly in the buyer UI.

### L1: Normalized RERA Tables

Parse the raw HTML into typed source-native tables. These tables preserve RERA
meaning but avoid HTML shape.

Suggested tables:

```text
rera_project_summary
rera_inventory_schedule
rera_tower_schedule
rera_tower_floor_schedule
rera_parking_schedule
rera_external_development_schedule
rera_water_infra_schedule
rera_document_manifest
rera_complaints_summary
```

The important design point is that RERA labels should not become durable product
fact keys one by one. The parser should map labels into typed columns.

Example `rera_project_summary` columns:

- `project_entity_id`
- `registration_number`
- `ack_number`
- `project_name`
- `promoter_name`
- `status`
- `project_type`
- `project_status`
- `start_date`
- `completion_date`
- `original_completion_date`
- `land_area_sqm`
- `covered_area_sqm`
- `open_area_sqm`
- `open_area_pct`
- `construction_cost_inr`
- `land_cost_inr`
- `total_project_cost_inr`
- `far_sanctioned`
- `approving_authority`
- `source_url`
- `source_html_sha256`
- `observed_at`

Example `rera_tower_schedule` columns:

- `project_entity_id`
- `tower_entity_id`
- `tower_name`
- `building_name`
- `tower_type`
- `floor_count`
- `basement_count`
- `stilt_count`
- `slab_count`
- `unit_count`
- `parking_count`
- `source_section`
- `observed_at`

Example `rera_document_manifest` columns:

- `project_entity_id`
- `document_id`
- `document_kind`
- `source_label`
- `filename`
- `source_href`
- `mime_type`
- `byte_size`
- `content_sha256`
- `raw_storage_key`
- `preview_storage_key`
- `thumbnail_storage_key`
- `page_count`
- `observed_at`

Keep `rera_tower_floor_schedule` out of the hot serving bundle unless a product
surface needs it. It is valuable evidence, but it is too granular for search.

### L2: OpenEstates Facts

Derive buyer-facing facts from the normalized tables. These should use generic
project keys where possible, with `source_type=Rera` and source links pointing
back to the RERA artifact.

Candidate canonical facts:

| Fact key | Source | Search? | Main surface |
| --- | --- | --- | --- |
| `project.land_area_sqm` | summary | yes | project specs |
| `project.land_area_acres` | derived | yes | project specs, compare |
| `project.open_area_pct` | derived from land/open area | yes | project specs, compare |
| `project.covered_area_sqm` | summary | no | project specs |
| `project.unit_count` | inventory | yes | project specs |
| `project.tower_count` | tower schedule | yes | project specs |
| `project.units_per_acre` | derived | yes | search, compare |
| `project.units_per_tower` | derived | maybe | compare |
| `project.max_floor_count` | tower schedule | yes | project specs |
| `project.parking.total_sanctioned` | tower/parking schedule | yes | parking |
| `project.parking.for_sale_count` | project details | no | parking |
| `project.parking.covered_count` | project details when present | yes | parking |
| `project.parking.open_count` | project details when present | yes | parking |
| `project.water_supply_mode` | external development | yes | water supply |
| `project.sewage_drainage_mode` | external development | maybe | water supply |
| `project.stp.capacity_kld` | water infra schedule, if present | yes | water supply |
| `project.borewell.proposed_count` | water infra schedule, if present | maybe | water supply |
| `project.borewell.existing_count` | water infra schedule, if present | maybe | water supply |
| `project.documents.site_plan_available` | document manifest | yes | evidence/media |
| `project.documents.floor_plan_available` | document manifest | yes | evidence/media |
| `project.documents.sanction_plan_available` | document manifest | yes | legal/RERA |

Existing `rera_*` keys can stay as compatibility aliases during migration, but
new UI and search config should prefer generic project keys. For example,
`rera_total_land_area_sqm` can map to `project.land_area_sqm`.

## Parser Requirements

The first implementation should be deterministic and table-aware.

Use a small HTML token stream parser rather than regexing the whole page. The
parser needs to preserve enough structure to understand:

- tab or section ID
- section heading
- rows of label/value pairs
- table headers and cells
- repeated tower panels
- document link label and filename

The parser should produce intermediate records before facts. That makes tests
clearer and keeps source parsing separate from product semantics.

Important validation rules:

- `open_area_pct = open_area_sqm / land_area_sqm * 100`, only when both numbers
  are positive and finite.
- `covered_area_sqm + open_area_sqm` should be close to `land_area_sqm`; if it
  is not, keep raw values but lower confidence on derived open-area metrics.
- `tower_count` from the summary should match the distinct tower schedule count
  when both are present.
- `unit_count` from inventory should be compared with tower unit totals.
- `parking_count` from tower rows should be compared with project parking rows.
- Coordinates should parse DMS-like strings explicitly. A loose numeric match
  should not turn `12o 98'26.66" N` into `12.0`.
- Repeated or duplicated tower sections should be deduped by
  `(tower_name, floor_count, unit_count, parking_count, source_section)`.

Each extracted value should carry extraction metadata:

- `source_tab`
- `source_section`
- `source_label`
- `source_url`
- `source_html_sha256`
- `observed_at`
- `parser_version`
- `confidence`
- optional `validation_notes`

## Document And Image Handling

RERA attachments should become first-class document artifacts. The raw PDF is
the receipt; compressed previews are a serving convenience.

The pipeline should:

1. collect document links from the RERA page
2. classify each document into a configured `document_kind`
3. download the file with the same RERA session cookie
4. store the original by content hash
5. write a document manifest row
6. generate previews only for allowed buyer-visible document kinds
7. write preview media rows that can flow into `image_media_facts`

Initial document kinds:

- `site_plan`
- `floor_plan`
- `sanction_plan`
- `development_plan`
- `section_plan`
- `brochure`
- `completion_or_extension_certificate`
- `legal_affidavit`
- `insurance_or_policy`
- `customer_document`
- `other`

Buyer-visible preview kinds should be restricted at first:

- `site_plan`
- `floor_plan`
- `sanction_plan`
- `development_plan`
- `section_plan`
- `brochure`

Do not render customer documents, escrow details, sale deeds, or agreements as
buyer-facing media unless we explicitly decide the privacy and product rules.

Preview generation should run offline in Python. This checkout does not
currently have `pdfinfo`, `pdftoppm`, ImageMagick, PIL, pypdfium2, or PyMuPDF
installed, so this cannot be assumed as an ambient capability. Add it as an
explicit pipeline dependency or feature.

Suggested preview policy:

- keep original PDF under raw document storage
- render first useful page, or first N pages for plan documents
- create `webp` previews at about 1600 px max width
- create thumbnails at about 480 px max width
- keep content hash, source hash, and preview hash in the manifest
- never rasterize documents in Rust

## Config Ownership

All durable semantics should be configured.

Add or extend config in:

```text
app/config/dag/asset_registry.json
app/config/dag/fact_registry.json
app/config/dag/search_intent.json
app/config/dag/ui_surfaces.json
app/config/product/evidence_sections.json
```

The asset registry should gain explicit RERA detail assets, for example:

```text
rera_project_detail_snapshots       raw
rera_project_detail_tables          silver
rera_document_artifacts             raw/silver
rera_project_spec_facts             gold
```

The product evidence config should gain a `project_specs` section rather than
adding more hardcoded RERA fields in Rust:

```text
Project specs:
  project.land_area_acres
  project.open_area_pct
  project.unit_count
  project.tower_count
  project.units_per_acre
  project.max_floor_count
  project.parking.total_sanctioned
```

The existing `RERA file` section should stay focused on registration and legal
timeline:

```text
RERA file:
  rera_number
  rera_status
  rera_completion_date
  rera_original_completion_date
  rera_delay_months
  rera_land_litigation
  rera_complaints_count
```

Water and operating context should use the water surface:

```text
Water setup:
  project.water_supply_mode
  project.sewage_drainage_mode
  project.stp.capacity_kld
  project.borewell.proposed_count
  project.borewell.existing_count
```

Media/document previews should use evidence media definitions rather than
property-specific frontend constants.

## Search Promotion

Not every extracted RERA field belongs in search. Promote only facts that help a
buyer express intent or compare options.

Promote strongly:

- acres / land area
- open area percentage
- units per acre
- tower count
- max floor count
- parking availability
- site/floor/sanction plan availability
- RERA status and completion timeline
- land litigation and complaints

Promote lightly:

- water supply mode
- sewage and drainage mode
- STP capacity
- borewell count
- FAR sanctioned

Do not promote initially:

- raw document filenames
- exact RERA `DOC_ID`
- per-floor unit rows
- escrow account number and IFSC
- customer documents
- every complaint row
- every construction-progress row

Search intent examples this should support:

- `large campus 3bhk`
- `low density project near Whitefield`
- `open space above 80 percent`
- `not too many towers`
- `project with site plan proof`
- `good parking`
- `avoid delayed RERA projects`
- `water setup with STP`

The search engine should consume the same serving-bundle facts as property
details. It should not call the RERA portal or parse documents at query time.

## UI Shape

The buyer UI should show the value, not the internal extraction process.

Good compact chips:

- `16.5 acres`
- `89% open area`
- `689 homes`
- `7 towers`
- `Max 24 floors`
- `Site plan`
- `Sanction plan`

Avoid buyer-facing copy such as:

- `RERA extraction completed`
- `source-backed`
- `RERA file parsed`
- `document artifact available`
- `enrichment queued`

Recommended surfaces:

- Result tile: one or two compact distinctions, such as `89% open area` or
  `16.5 acres`.
- Property detail: a `Project specs` evidence section with land, density,
  towers, floors, units, and parking.
- Evidence stack: source rows with RERA as the source and plan preview media
  where available.
- Compare: rows for land acres, open area percentage, units per acre, max
  floors, parking count, and plan availability.
- Area Tracker: crawl freshness, RERA coverage, evidence strength, and area
  density distribution.

## Backend Runtime Changes

The backend should remain a generic renderer of configured facts.

Concrete changes:

- stop expanding the hardcoded `rera_items` list in `build_source_panels`
- prefer `app/config/product/evidence_sections.json` for RERA and project-spec
  source panels
- add generic media kind resolution for RERA document previews, similar to the
  approach-road media path but backed by serving facts rather than live external
  URL generation
- keep `ReraInfo` as a compatibility summary for existing page fields, but do
  not use it as the only way to vend rich RERA data
- keep all derived metrics in the DAG or serving materializer, not in route
  handlers

The request path should remain:

```text
current.json
  -> load serving bundle Parquet and Tantivy
  -> in-memory entity/fact/edge lookup
  -> route assembles configured sections
```

## First Implementation Slice

The first slice should prove the model with three RERA fixtures:

- Prestige Waterford
- Sobha Insignia
- Prestige Raintree Park

Acceptance for the first slice:

- parser extracts Waterford land, open area, inventory, tower count, tower rows,
  parking for sale, water/sewage modes, and plan documents
- parser does not produce bad `12.0,77.0` coordinates from DMS text
- normalized Parquet tables are written with ZSTD compression
- derived facts include acres, open-area percentage, units per acre, tower
  count, max floor count, total units, and site-plan availability
- fact registry owns display templates and search semantics
- evidence sections render the new project-spec facts from config
- search can match at least `large campus`, `open space`, `low density`,
  `parking`, and `site plan proof`
- buyer UI does not show parser state, missing-data essays, or internal RERA
  labels

## Test Plan

Add parser fixture tests before changing the DAG:

- Waterford fixture: assert `66823 sqm`, `59380 sqm`, `88.86%`, `689` units,
  `7` towers, `106` parking-for-sale count, and site-plan document detection.
- Sobha Insignia fixture: assert small-project land/open area, tower schedule,
  parking, and plan document detection.
- Raintree fixture: assert large project inventory, open area, tower count, and
  covered parking.
- Validation tests: open plus covered area reconciliation, tower schedule dedupe,
  coordinate parsing, and sum-of-units comparison.

Add asset contract tests:

- raw detail snapshot manifest includes HTML hash and RERA IDs
- document manifest rows include content hash and storage key
- normalized rows round-trip through Parquet
- derived fact rows carry source metadata and registry annotations

Add runtime tests:

- property source panels include project-spec facts without hardcoding a new
  Rust fact list
- property detail exposes acres/open-area percentage from serving facts
- search ranking can consume promoted project facts

## Rollout

Start with a scoped backfill for selected societies only. Do not crawl every
RERA project until the parser and document policy are stable.

Suggested sequence:

1. Add parser fixtures and normalized records.
2. Add raw detail snapshot and document manifest assets.
3. Add derived project-spec facts and registry entries.
4. Add configured project-spec evidence section.
5. Add document preview generation behind an explicit pipeline feature.
6. Backfill selected known societies.
7. Compare search and property pages against current production behavior.
8. Broaden the crawl after extraction quality is measured.

## Open Questions

- Should `project.land_area_sqm` replace `rera_total_land_area_sqm` immediately,
  or should we run both keys for one serving-bundle version?
- Should document previews be stored as `image_media_facts`, or should we add a
  separate `document_media_facts` asset with richer document metadata?
- What is the first buyer-visible rule for plan documents: show only preview
  thumbnails, or also expose the original RERA PDF link?
- Should STP and borewell details be extracted from HTML first, or from
  attached plan/specification PDFs where HTML does not expose them?
- Should the UI show `units per acre` directly, or translate it into a label
  such as `low density` with the raw value in evidence?
