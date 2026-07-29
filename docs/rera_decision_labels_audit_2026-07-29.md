# RERA Decision Labels Audit - 2026-07-29

Status: design/audit note only. No implementation in this pass.

## Context

The current RERA surface is still too source-shaped. Counts and tabbed detail
lose the memory of what a buyer actually needs to compare:

```text
Which society had land litigation?
Which one had refund complaints?
Which builder has many delayed projects?
Which issue is project-specific versus builder-history?
```

The useful product object is not a tab. It is a compact, source-backed label
bundle that travels with the society into search, shortlist, Notebook, and
Compare.

Example target labels:

```text
rera_land_litigation
litigation:12
builder_overall_litigation:45
builder_ongoing_projects:12
builder_delayed_projects:2
builder_ontime_projects:5
```

These labels should be easy for buyers to remember, but they must be generated
from scoped facts and relations, not hand-authored UI text.

## Audit Input

I read:

- `75` cached Karnataka RERA detail HTML pages from
  `data/cache/skills/rera_details/`
- the current promoted `rera_legal_facts` materialization:
  `run_id=3549ec10-bd9b-4588-8171-175639d7a33c`
- parser logic in `pipeline/skills/fetch_rera.py`
- existing design notes for RERA decision atoms and Notebook/Compare

Important finding:

- promoted `rera_legal_facts` is too thin for this product surface
- it mostly preserves counts, land litigation, mortgage, and simple dates
- richer complaint theme tags are recoverable from raw cached RERA HTML, but
  are not promoted as first-class rows yet

## Current Parser Theme Vocabulary

The parser already classifies complaint subjects into deterministic tags:

```text
refund
cancellation
delay
compensation
possession
agreement_payment
interest_demand
quality
amenities
parking
maintenance
title_land
khata
approval_oc_cc
registration_document
builder_conduct
other
```

These are good internal fact tags. Buyer-facing labels should usually compress
or group them:

| Parser tag | Buyer label family |
| --- | --- |
| `refund`, `cancellation`, `interest_demand` | Money dispute |
| `delay`, `possession`, `compensation` | Delay / possession |
| `agreement_payment`, `registration_document` | Agreement / payment |
| `title_land`, `khata` | Land / title |
| `approval_oc_cc` | Approval / OC |
| `quality`, `amenities`, `maintenance`, `parking` | Handover quality |
| `builder_conduct` | Builder conduct |

Do not expose every parser tag as a visible chip. Use parser tags as the
evidence vocabulary and promote only memorable decision labels.

## Audit Breakdown

Project complaint totals are RERA tab counts. Parsed rows are the subset where
the current parser could extract complaint rows and theme tags.

| Property | Project complaints | Project theme tags | Other legal labels |
| --- | ---: | --- | --- |
| Purva Palmbeach | 15 total / 14 parsed | refund, agreement/payment, delay, possession, compensation, parking, other | promoter history: 107 complaints / 54 parsed |
| Serene at Brigade Cornerstone Utopia | 11 / 4 | refund, cancellation, agreement/payment, other | promoter history: 21 complaints / 5 parsed |
| Assetz Marq | 10 / 10 | delay, compensation, agreement/payment, possession, other | 1 open project complaint |
| Mahendra AARNA | 6 / 4 | compensation, agreement/payment, amenities, maintenance, refund, cancellation | promoter history: 10 complaints / 5 parsed |
| Radiant Spencer Annex | 6 / 0 | project themes not parsed | promoter history: other |
| Provident Capella 1 | 5 / 0 | project themes not parsed | promoter: refund, delay, compensation, possession, agreement/payment, interest demand |
| Godrej Woodscapes | 5 / 0 | project themes not parsed | promoter: refund, cancellation, title/land, compensation, agreement/payment |
| Pursuit of a Radical Rhapsody Phase 2 | 4 / 3 | delay, possession, title/land, other | promoter history: 20 complaints / 5 parsed |
| Eden at Brigade Cornerstone Utopia | 4 / 1 | other | promoter: refund, cancellation, agreement/payment |
| Assetz Marq Building 2 - Tower 5 | 4 / 0 | project themes not parsed | promoter tab count exists, no parsed themes |
| Godrej Air | 3 / 3 | cancellation, agreement/payment, builder conduct, compensation, amenities, other | same themes at promoter scope |
| Godrej United | 3 / 1 | compensation | same at promoter scope |
| Uniworld Resorts | 2 / 2 | refund, compensation, agreement/payment, interest demand | same at promoter scope |
| Prestige Lakeside Habitat | 2 / 2 | delay, compensation, agreement/payment, builder conduct, amenities, maintenance | same at promoter scope |
| BCD Paradiso | 2 / 2 | quality, refund, cancellation, agreement/payment, interest demand | same at promoter scope |
| Chartered Hummingbird | 2 / 1 | other | same at promoter scope |
| Tranquil at Brigade Cornerstone Utopia | 2 / 0 | project themes not parsed | promoter: refund, cancellation, agreement/payment |
| Paradise at Brigade Cornerstone Utopia | 2 / 0 | project themes not parsed | promoter: refund, cancellation, agreement/payment |
| Halcyon at Brigade Cornerstone Utopia | 2 / 0 | project themes not parsed | promoter: refund, cancellation, agreement/payment |
| Godrej Woodscapes Phase 2 | 2 / 0 | project themes not parsed | promoter: refund, cancellation, title/land, compensation, agreement/payment |
| Godrej Splendour | 2 / 0 | project themes not parsed | promoter: refund, cancellation, title/land, compensation, agreement/payment |
| Assetz Marq Building 3 - Tower 6 | 2 / 0 | project themes not parsed | promoter tab count exists, no parsed themes |
| Prestige Waterford | 1 / 1 | refund | promoter: delay, possession, approvals/OC, compensation, quality, refund, title/land |
| Brigade Lakefront - Crimson | 1 / 1 | other | promoter: delay, compensation, agreement/payment, amenities, title/land, refund, possession |
| Peninsula Heights | 1 / 1 | maintenance | same at promoter scope |
| Candeur Signature | 1 / 1 | refund | same at promoter scope |
| Sobha Windsor Phase 1 Wing 1 and 2 | 1 / 1 | other | promoter history: 222 complaints / 127 parsed |
| Sobha Windsor Phase 2 Wing 3, 4 and 5 | 1 / 1 | other | promoter history: 222 complaints / 127 parsed |
| Sobha Silicon Oasis Phase 1 | 1 / 1 | other | promoter history: 222 complaints / 127 parsed |
| Sobha Windsor Phase 4 Wing 9, 10 and 11 | 1 / 0 | project themes not parsed | promoter history: 222 complaints / 127 parsed |
| Sobha Insignia | 1 / 0 | project themes not parsed | promoter history: 222 complaints / 127 parsed |
| Prestige Raintree Park | 1 / 0 | project themes not parsed | promoter: delay, possession, approvals/OC, compensation, quality, refund |
| Godrej Tiara | 1 / 0 | project themes not parsed | promoter: refund, cancellation, title/land, compensation, agreement/payment |
| Folium by Sumadhura Phase I | 1 / 0 | project themes not parsed | mortgage, promoter agreement/payment |
| Folium by Sumadhura Phase II | 1 / 0 | project themes not parsed | promoter agreement/payment |
| Assetz Marq Phase 3B | 1 / 0 | project themes not parsed | mortgage |
| Vaswani Starlight | 0 / 0 | none | land litigation |
| Pursuit of a Radical Rhapsody Tower 8 | 0 / 0 | none | land litigation, promoter title/land + delay/possession |
| Elysium at Brigade Cornerstone Utopia | 0 / 0 | none | land litigation, promoter refund/cancellation/agreement-payment |
| Brigade Lakecrest | 0 / 0 | none | land litigation, promoter delay/compensation/agreement/title-land |
| Assetz Marq Phase 3A | 0 / 0 | none | mortgage |

No litigation tags were found in the cached parse for:

```text
Velpula Pride
Vasundra SS Valley
Vajram Tiara
The Roots by Elegance Infra
Sumadhura Elysium Phase-I
Sky Asta
Shree Gruha Kalpa
SBR One Residence
Pelican Grove
Havena by KSR
Fort House
Fiorana at Beaumont Estate Phase-1
Dharani Homes
Candeur Landmark
Assetz Muse & Maison
Assetz KVN Niwa & Neo
Arvind Sylva
Amberstone Vectra
```

## Refined Product Idea

The product should build a label graph, not a tab UI.

Tabs answer:

```text
Where did RERA put this information?
```

Labels answer:

```text
What should I remember when comparing this home?
```

The buyer should see compact handles such as:

```text
Land litigation
12 project complaints
Refund disputes
Builder: 45 complaints
Builder: 12 ongoing projects
Builder: 2 delayed, 5 on-time
```

Each label must know its scope:

| Scope | Meaning | Example |
| --- | --- | --- |
| society/project | This exact RERA project | `project_complaints:12` |
| promoter/builder | Builder history across RERA projects | `builder_overall_litigation:45` |
| related project | Other projects by same promoter | `builder_delayed_projects:2` |
| document | Evidence attached to this RERA project | `legal_doc_available:ec` |
| derived | Computed from several source facts | `delivery_delayed:18_months` |

This distinction is the whole product value. A buyer can tolerate builder
history if the exact project is clean; they should not be misled by a single
unscoped complaint number.

## Label Bundle Contract

Create a generated serving object per society:

```text
ReraDecisionLabelBundle
  society_id
  project_entity_id
  promoter_entity_id
  labels[]
  relations[]
  computed_at
  source_watermarks[]
```

Label shape:

```text
DecisionLabel
  label_id
  label_key
  buyer_label
  scope: project | promoter | related_project | document | derived
  category: legal | delivery | builder_record | docs | scale | finance
  severity: positive | neutral | caution | risk
  value_number
  value_text
  unit
  confidence
  evidence_count
  source_fact_keys[]
  source_record_ids[]
  compare_behavior: count | boolean | category | theme | timeline | evidence
```

Relation shape:

```text
DecisionRelation
  from_entity_id
  relation_type
  to_entity_id
  evidence_key
  confidence
```

Important relations:

```text
society -> rera_project
rera_project -> promoter
promoter -> rera_project[]
rera_project -> complaint_row[]
rera_project -> document[]
rera_project -> completion_timeline
```

The label is what the buyer remembers. The relation is how the system proves it.

## Candidate Label Keys

Legal/project labels:

```text
rera_land_litigation
project_complaints_count
project_open_complaints_count
project_refund_dispute
project_delay_dispute
project_possession_dispute
project_agreement_payment_dispute
project_title_land_dispute
project_approval_oc_dispute
project_quality_handover_dispute
project_mortgage_declared
```

Builder/promoter labels:

```text
builder_overall_litigation_count
builder_open_litigation_count
builder_refund_dispute_history
builder_delay_dispute_history
builder_title_land_dispute_history
builder_approval_oc_dispute_history
builder_project_count
builder_ongoing_project_count
builder_completed_project_count
builder_delayed_project_count
builder_ontime_project_count
builder_average_delay_months
builder_max_delay_months
builder_defaulter
builder_rejection_count
builder_revoked_project_count
```

Delivery labels:

```text
project_delivery_delayed
project_delivery_delay_months
project_delivery_on_track
project_delivery_completed
builder_delivery_track_record
builder_delayed_projects
builder_ontime_projects
builder_average_delay_months
builder_max_delay_months
```

Document labels:

```text
site_plan_available
sanction_plan_available
floor_plan_available
legal_land_docs_available
ec_available
noc_available
only_affidavit_visible
```

Use config to map these labels to buyer text, thresholds, severity, and compare
behavior. Do not bake label thresholds into React or Rust match arms.

## How To Generate Labels

Use a layered pipeline. The durable shape should be Parquet rows of facts and
relations, not a hand-built runtime graph as source truth:

```text
raw RERA HTML
  -> normalized RERA tables
  -> canonical facts and relations
  -> decision label bundle
  -> serving bundle
  -> Rust API / Compare / Notebook
```

Concrete source tables needed:

```text
rera_project_summary
rera_promoter_registry
rera_complaint_rows
rera_complaint_summary
rera_completion_timeline
rera_uploaded_document_manifest
rera_project_relations
```

The current `builder_rera_aggregates` asset is the right place to expand
builder rollups. The current `home_state_signals` asset already derives
home/project state and should be reused for delivery labels.

## Parquet Rows Or Graph?

Use both, but at different layers.

### Durable Truth: Parquet Rows

The lake should store facts and relations as boring, appendable Parquet tables:

```text
rera_project_facts
  entity_id
  fact_key
  value_type
  value_number
  value_text
  value_bool
  value_tags
  source_record_id
  source_url
  confidence
  observed_at

rera_relations
  from_entity_id
  relation_type
  to_entity_id
  source_record_id
  confidence
  observed_at
```

This keeps the data crunchable. Builder-level insights are naturally aggregate
queries over rows:

```text
promoter -> all_projects
all_projects -> completion timelines
all_projects -> complaints
all_projects -> registration outcomes
```

### Offline Crunch: Builder And Project Rollups

Do not compute these on the property page request path. Materialize them:

```text
builder_rera_aggregates
  promoter_entity_id
  builder_project_count
  builder_ongoing_project_count
  builder_completed_project_count
  builder_delayed_project_count
  builder_ontime_project_count
  builder_average_delay_months
  builder_max_delay_months
  builder_complaint_count
  builder_open_complaint_count
  builder_rejection_count
  builder_revoked_project_count
  builder_defaulter
  theme_counts
```

Then generate labels from those rollups:

```text
rera_decision_labels
  society_id
  label_key
  scope
  category
  severity
  value_number
  value_text
  evidence_count
  source_fact_keys
  relation_keys
```

### Runtime: In-Memory Index Or Small Graph

Rust should load the promoted serving bundle into memory and build whatever
indexes make serving fast:

```text
society_id -> ReraDecisionLabelBundle
promoter_id -> builder labels
label_key -> societies
relation edge -> source evidence
```

This can feel graph-like in memory, but the graph is a serving/index view. The
source of truth remains typed Parquet facts and relation rows.

### Why Not Just A Graph?

A graph-only model makes aggregation and historical recomputation harder. The
labels we want are mostly aggregate facts:

```text
builder_average_delay_months
builder_delayed_project_count
builder_defaulter
builder_rejection_count
builder_overall_litigation_count
```

Those are easier to test, diff, version, and promote as tabular assets. The
graph relation is still important, but mainly to prove scope:

```text
this society -> this RERA project -> this promoter -> these other projects
```

The product answer is a label. The proof is a relation path. The durable data is
Parquet.

## Builder Insight Labels To Add

These labels are valuable because buyers cannot easily discover them by opening
one RERA page:

| Label key | Scope | Derived from | Buyer meaning |
| --- | --- | --- | --- |
| `builder_average_delay_months` | promoter | completion timelines across promoter projects | Typical delay pattern |
| `builder_max_delay_months` | promoter | max project delay | Worst observed delivery slip |
| `builder_delayed_project_count` | promoter | projects where revised date > original date | How often builder slips |
| `builder_ontime_project_count` | promoter | projects where revised date <= original date | Delivery discipline |
| `builder_ongoing_project_count` | promoter | project status / future completion targets | Current execution load |
| `builder_defaulter` | promoter | revoked/defaulted/penalty status if RERA exposes it | Serious regulatory warning |
| `builder_rejection_count` | promoter | rejected applications/registrations if available | Registration quality signal |
| `builder_revoked_project_count` | promoter | revoked projects | Severe track-record concern |
| `builder_overall_litigation_count` | promoter | promoter complaint summary | Builder-wide dispute load |
| `builder_open_litigation_count` | promoter | open/under-enquiry complaint rows | Unresolved dispute load |

Keep the labels exact and scoped. For example:

```text
Builder avg delay: 11 months
Builder delayed projects: 2
Builder on-time projects: 5
Builder defaulter: no
Builder rejections: 1
```

Buyer copy can be shorter later, but the machine label should preserve the
number, denominator, source scope, and relation path.

## Delay Label Collection Plan

The first delivery label should come from facts we already collect:

```text
original_completion_date
revised_completion_date
rera_delay_months
rera_status
project_timeline_state
home_timeline_state
```

Current parser support:

- `fetch_rera.py` reads original and current completion dates from search/detail
  result fields.
- it already computes `rera_delay_months` when both dates exist.
- `home_state_signals` already emits timeline states such as `delayed` and
  `on_track`.

Needed improvements:

1. Normalize completion dates into a `rera_completion_timeline` table:

   ```text
   project_entity_id
   promoter_entity_id
   registration_number
   original_completion_date
   revised_completion_date
   delay_months
   status
   observed_at
   parser_version
   source_url
   confidence
   ```

2. Add builder-level rollups over all RERA projects by promoter:

   ```text
   builder_project_count
   builder_ongoing_project_count
   builder_completed_project_count
   builder_delayed_project_count
   builder_ontime_project_count
   builder_average_delay_months
   builder_max_delay_months
   ```

3. Define delivery label thresholds in config:

   ```text
   project_delivery_on_track: delay_months == 0
   project_delivery_minor_delay: 1..6 months
   project_delivery_delayed: 7..18 months
   project_delivery_major_delay: >18 months
   builder_delivery_clean: delayed_project_count == 0 and project_count >= 3
   builder_delivery_mixed: delayed_project_count > 0
   builder_delivery_risky: delayed_project_count / project_count >= 0.4
   ```

4. Keep lifecycle context:

   - future target date with no original date is not a delay
   - completed project with revised date after original date is still a delivery
     history signal
   - very new projects should not be labeled late until the current target has
     passed or RERA itself records an extension

5. Store both exact numeric labels and buyer memory labels:

   ```text
   project_delivery_delay_months: 18
   project_delivery_delayed: true
   builder_delayed_project_count: 2
   builder_ontime_project_count: 5
   builder_delivery_track_record: "mixed"
   ```

## Product Examples

For Brigade Cornerstone Utopia variants:

```text
Elysium at Brigade Cornerstone Utopia
  rera_land_litigation
  builder_overall_litigation:21
  builder_refund_dispute_history
  builder_cancellation_dispute_history
  builder_agreement_payment_dispute_history
```

For Prestige Waterford:

```text
project_complaints_count:1
project_refund_dispute
builder_overall_litigation:75
builder_delay_dispute_history
builder_possession_dispute_history
builder_approval_oc_dispute_history
```

For Assetz Marq:

```text
project_complaints_count:10
project_open_complaints_count:1
project_delay_dispute
project_compensation_dispute
project_possession_dispute
builder_overall_litigation:10
```

For Godrej Splendour:

```text
project_complaints_count:2
builder_overall_litigation:58
builder_refund_dispute_history
builder_cancellation_dispute_history
builder_title_land_dispute_history
```

## Implementation Direction

Do not implement this as frontend chips first. Build the data contract first:

1. Promote parsed complaint rows and summaries from RERA HTML into normalized
   lake assets.
2. Expand `builder_rera_aggregates` to compute scoped complaint and delivery
   rollups.
3. Add a `rera_decision_labels` asset that consumes project facts, complaint
   summaries, delivery timelines, documents, and builder aggregates.
4. Add config for label definitions, thresholds, severity, buyer text, and
   compare behavior.
5. Serve labels through Rust as structured API data.
6. Later, UI can render these labels in search, property details, shortlist,
   Notebook, and Compare without reinterpreting RERA facts.

## Open Questions

- Should `litigation` include all RERA complaints, or only legal/land/title
  themes? Product copy probably needs two labels:
  `project_complaints_count` and `land_title_litigation`.
- Should promoter complaints be deduplicated across project variants? Sobha and
  Prestige show repeated promoter history across related projects.
- Should ongoing/open complaint count use parsed rows only, or tab count when
  row parsing is incomplete? For now, show total from tab and mark row-derived
  themes as partial.
- Should mortgage be a risk label or a neutral finance label? It needs buyer
  copy carefully; mortgage can be normal project financing, not automatically a
  defect.

## Near-Term Next Step

Before UI work, create a small generated `rera_decision_labels` prototype from
the cached RERA HTML:

```text
data/cache/rera_decision_labels/whitefield_rera_labels_2026-07-29.json
```

That prototype should contain one `ReraDecisionLabelBundle` per society and
should be compared against the table in this audit. Once the labels look right,
promote the same shape into DAG/lake assets.
