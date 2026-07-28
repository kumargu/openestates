# RERA, Notebook, And Compare UI Design - 2026-07-28

Status: design research only. No implementation in this pass.

## Product Principle

RERA should not become another table in the property page. It should become a
set of clickable, source-backed buyer facts that can move into Notebook and then
into Compare.

The user flow should be:

```text
Property page fact
  -> save / pin / question it
  -> Notebook item with tag and source
  -> Compare row when another selected home has the same tag or comparable fact
```

Notebook is the buyer's memory. Compare is the buyer's decision table. RERA is
one of the strongest sources feeding both.

## Current UI Signals

The current mock direction is good:

- Notebook has explicit tags.
- Compare can build one row per shared tag.
- Compare is not trying to parse prose; the tag is the contract.
- Notes can be personal (`You`), plan-derived (`Plan`), or fact-derived
  (`Saved fact`).

For RERA this is exactly the right mental model. Every RERA fact should be
saveable as a typed note/fact, with a stable tag and a source trail.

## The Core UI Object

Introduce a generic "decision atom" shape. This is not a UI component name; it
is the mental model.

```text
id
home_id / society_id / project_id
source_type: RERA | Google | Map | Plan | User | Seller
fact_key
label
value
unit
tag
scope: home | tower | society | project | promoter | area | buyer
confidence
source_url / document_ref
notebook_state: saved | pinned | dismissed
compare_behavior: numeric | categorical | evidence | note
```

The same object can render as:

- a chip on the property page
- a row in a source section
- a notebook item
- a compare row
- a warning or question prompt

This avoids hardcoding "RERA UI" separately from "Notebook UI".

## RERA UI Structure On Property Page

Use an evidence-board structure, not a flat table.

### 1. RERA Snapshot Strip

Top of the RERA section should answer the buyer's first scan:

```text
Approved
Target Mar 2019
15 project complaints
52% open area
Site plan available
```

Each chip should be clickable:

- click opens detail/evidence
- save icon adds it to Notebook
- compare icon pins it for Compare

Do not show internal labels like `rera_total_land_area_sqm`.

### 2. Buyer Question Cards

Below the strip, group RERA facts by buyer question:

| Card | Buyer question | Example facts |
| --- | --- | --- |
| Registration | Is this legally registered? | status, RERA number, approval date |
| Timeline | Is delivery delayed? | start, current target, original target, delay |
| Scale | What kind of project is this? | acres, homes, towers, max floors, density |
| Documents | What proof is available? | site plan, sanction plan, floor plan, EC, NOCs |
| Complaints | What went wrong for buyers? | project complaints, promoter complaints, themes |
| Water / utilities | What operating setup is declared? | BWSSB/BESCOM/NOCs/STP/borewell when confident |

Each card should have a compact read state and an expanded evidence state.
Expanded state can show rows, documents, and source links.

### 3. Document Shelf

Documents should render as classified evidence, not a dump of filenames.

Suggested groups:

- Plans: site plan, sanction plan, floor plan, section plan, development plan
- Legal land: EC, title deed, JDA, khata, conversion
- Approvals and NOCs: commencement certificate, BESCOM, BWSSB, fire, airport,
  KSPCB, SEIAA, BMRCL
- Buyer templates: agreement for sale, allotment letter, sale deed proforma
- Promoter documents: PAN, balance sheet, P&L, auditor report
- Affidavits: Annexure 49, Form B

Plan documents can have previews. Most legal/customer/promoter documents should
stay as source rows, not visual media.

### 4. Complaint Digest

RERA complaints should render like review themes, but with legal status.

Example:

```text
Project complaints
15 filed · 14 disposed
Common themes: refund, delay compensation, possession delay

Promoter record
107 complaints across Karnataka RERA projects
Common themes: refund, delay, agreement/payment disputes
```

Clicking a theme opens sample complaint subjects. Saving a theme adds a
Notebook item under `Legal` or `Builder record`.

Avoid showing all complaint rows by default.

## Notebook Integration

Notebook should support three item types:

### User Note

Example:

```text
I can pay 20L on this property
Tag: Down payment
Source: You
```

### Saved Fact

Example:

```text
RERA says 52% open area
Tag: Project scale
Source: RERA · Project Details
```

### Plan Pin

Example:

```text
Comfortable EMI is Rs 1.35L/month
Tag: EMI
Source: Plan
```

RERA facts should mostly become Saved Facts. A buyer should be able to save:

- `15 project complaints`
- `107 promoter complaints`
- `52% open area`
- `1,171 homes`
- `15 towers`
- `RERA target Mar 2019`
- `Site plan available`
- `BWSSB NOC available`
- `EC document available`

Each saved fact should retain the source and the exact home/project.

## Compare Integration

Compare should not only compare raw numbers. It should compare shared concerns.

### Auto Rows From Shared Tags

If both selected homes have facts/notes under `Legal`, Compare shows a Legal
row. If only one has it, show the other as empty with a prompt:

```text
Legal
Waterford: 15 project complaints · EC available
Park Retreat: -
```

### Canonical RERA Compare Rows

Some rows should appear automatically when both homes have RERA facts:

| Row | Waterford example | Park Retreat example |
| --- | --- | --- |
| RERA status | Approved | Approved |
| Delivery target | Sep 2024, extended | Dec 2028 |
| Project complaints | 21 parsed / scoped pending | 0 |
| Builder complaints | 75 promoter-level | 4 promoter-level |
| Land | 16.5 acres | 8.9 acres |
| Open area | 88.9% | 72.0% |
| Density | 42 homes/acre | 96 homes/acre |
| Towers | 7 towers, max 24 floors | 6 towers, max 18 floors |
| Plans | site + sanction + brochure | site only |
| Utilities proof | BWSSB/BESCOM/fire NOCs | affidavit only / unknown |

The row should show the human value first and source after:

```text
88.9% open area
RERA Project Details
```

### Compare Row Types

Use different layouts by fact type:

- numeric: land, density, open area, complaints count
- categorical: status, home state, available document kinds
- evidence: site plan, NOCs, EC, OC
- theme: complaint themes, Google review themes, notebook concerns
- personal: down payment, EMI, commute note

This makes Compare feel authored rather than like a database export.

## Interaction Details

### Clickable Fact Actions

Every important fact should support:

- Save to Notebook
- Add to Compare
- Open source
- Ask / mark uncertain

This can be a small action tray on hover/tap:

```text
[+] Save   [Compare]   [Source]
```

Mobile can use a bottom sheet after tapping the fact.

### Tags

RERA facts should map to stable tags:

| RERA fact | Notebook tag |
| --- | --- |
| registration status, RERA number | Legal |
| completion date, delay | Delivery |
| complaints | Legal / Builder record |
| land, units, towers, density | Project scale |
| site/floor/sanction plans | Plans |
| EC/title/JDA/khata | Legal docs |
| BWSSB/BESCOM/fire/KSPCB NOC | Utilities |
| STP/borewell/water supply | Water |
| mortgage/borrowing/escrow | Payment safety |

Do not make the user manually tag every source fact. System facts should arrive
with a suggested tag that the user can change.

### Compare Eligibility

Not every saved note should become a compare row. Compare eligibility should be:

- user pinned it, or
- same tag appears for at least two selected homes, or
- canonical compare fact exists for at least two selected homes, or
- one selected home has a strong warning and the other is unknown

This keeps Compare dense but not noisy.

## RERA-Specific UI Warnings

### Lifecycle-Aware Missing Docs

Recent 2026 projects often showed only `Affidavit (Annexure - 49)` in uploaded
documents. UI should say:

```text
Only affidavit visible in current RERA upload snapshot
```

not:

```text
No site plan
```

unless we have enough lifecycle context.

### Complaint Scope

Never show a single ambiguous complaint count. Always label:

- project complaints
- promoter complaints

### Document Privacy

Do not preview customer agreements, sale deeds, affidavits, PAN, financial
statements, or land title documents as visual media by default. They can be
listed as evidence rows.

## Suggested Visual Direction

For property page:

- compact source cards, not large dashboards
- chips for high-value facts
- document shelf with small grouped pills and only plan previews
- complaint digest as themes, similar to review themes
- save/compare actions on each fact

For Notebook:

- maintain the list/table view from the mock
- add source-backed saved facts alongside user notes
- show small source badges: `RERA`, `Plan`, `You`, `Google`, `Map`
- keep tags as the primary organizing mechanism

For Compare:

- keep editorial columns for selected homes
- add sections from canonical facts first, notebook tags second
- make missing values useful: `Not found`, `Not checked`, `Only affidavit visible`
- allow user to hide rows, because a buyer's compare table should stay personal

## Example End-To-End Flow

1. Buyer opens Waterford.
2. RERA snapshot shows `21 complaints`, `88.9% open area`, `site plan`.
3. Buyer clicks `21 complaints`.
4. Complaint digest opens: delay compensation and possession delay are common.
5. Buyer saves `Delay compensation complaints` to Notebook under `Legal`.
6. Buyer opens Park Retreat and saves `0 project complaints`.
7. Compare now shows a `Legal` row:

```text
Waterford: 21 project complaints · delay compensation theme
Park Retreat: 0 project complaints
```

This is much better than a static RERA section because it helps answer the real
question: "Which home has the risk I care about?"

