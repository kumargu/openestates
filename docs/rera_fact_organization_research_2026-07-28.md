# RERA Fact Organization Research - 2026-07-28

Status: research note only. No UI or pipeline changes proposed as final.

## Research Goal

RERA currently exposes useful buyer facts, but the source page is organized for
regulatory filing, not buyer decision-making. The product problem is not only
"show RERA data"; it is to answer buyer questions in one place:

- Is this project legally registered and still within declared timelines?
- How big, dense, and vertical is the project?
- What documents exist as proof?
- Are complaints about this project, the builder, or unrelated builder projects?
- What facts are stable enough for search and comparison?
- What should remain as evidence-only because it is too technical, private, or
  inconsistent?

## Live Source Check

Headless Chrome was run against:

```text
https://rera.karnataka.gov.in/viewAllProjects?language=en
```

The listing page is a large HTML page with embedded JavaScript/data arrays, not
a clean public JSON API. Detail pages are fetched through the same session by
posting to `projectViewDetails` and `projectDetails`.

The page shape observed in the screenshots is representative:

- top-level project registration header
- tabs for promoter, project details, uploaded documents, enquired documents,
  complaints, quarterly updates, and completion details
- nested complaint tabs for promoter-level and project-level complaints
- many PDF/image document links under uploaded/enquired document sections
- repeated tower/development tables on richer project pages

## Fresh Sample

Sampled projects from live/cached Karnataka RERA detail pages:

| Project | RERA status | Land sqm | Open % | Units | Towers | Max floors | Docs detected | Complaint rows observed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- | ---: |
| Purva Palmbeach | Approved | 78,819 | 52.00 | 1,171 | 15 | 19 | site plan, brochure | promoter 107, project 15 shown in UI |
| Prestige Waterford | Approved | 66,823 | 88.86 | 689 | 7 | 24 | site plan, sanction plan, brochure | complaint table present |
| Godrej Splendour | Approved | 30,898 | 46.94 | 1,161 | 9 | 29 | site plan, brochure, floor plan | complaint table present |
| SBR ONE RESIDENCE | Approved | 41,752 | 72.28 | 931 | 13 | 20 | sanction plan | no project complaints found |
| Assetz Marq | Approved | 18,700 | 20.00 | 469 | 4 | 26 | sanction plan, site plan | complaint table present |
| Sobha Insignia | Approved | 3,946 | 79.04 | 33 | 1 | 8 | parser missed docs in quick pass | complaint table present |
| Prestige Raintree Park | Approved | 112,652 | 65.19 | 1,520 | 18 | 19 | brochure in quick pass | complaint table present |
| Brigade Lakecrest | Approved | 28,024 | 85.59 | 604 | 4 | 21 | site plan, brochure, floor plan | complaint table present |

Important caveat: the current scraper and quick research parser are still
shallow. The counts above prove availability and rough consistency, not final
production extraction accuracy.

## Common Fact Families

### 1. Registration and Timeline

Common, high-confidence:

- RERA registration number
- acknowledgement number
- project name
- promoter name
- approved/registration date
- declared start date
- current proposed completion date
- original completion date
- status

Buyer use:

- "Is this RERA registered?"
- "Is the date extended from the original commitment?"
- "Is this completed, ongoing, or still pending?"

Recommended surface:

- keep as "RERA file"
- show compact status, RERA number, approval date, current target, original
  target, and delay if current target changed

### 2. Project Scale and Density

Common enough to normalize:

- land area
- open area
- covered area
- unit count
- tower/block count
- max floor count
- parking count, with caveats
- units per acre as a derived OpenEstates fact

Buyer use:

- "Is this a large campus?"
- "Is it high density?"
- "How vertical is it?"
- "How much open space is declared?"

Recommended surface:

- move out of the generic RERA panel into "Project scale"
- use buyer terms: acres, open area %, homes, towers, max floors, homes/acre
- compare view should prefer these facts over raw RERA labels

### 3. Documents and Plans

Common document kinds observed:

- site plan
- sanction/approved plan
- brochure
- floor plan or typical floor plan, but not always
- development plan and section drawings in richer pages
- status/progress images or certificates

Not consistently available:

- unit-level floor plans
- clean floor-plan-to-BHK mapping
- OCR-readable plan contents
- buyer-safe previews for every uploaded PDF

Buyer use:

- "Can I see the site/layout proof?"
- "Is there a sanctioned plan?"
- "Do we have floor plans for this BHK?"

Recommended surface:

- promote counts and preview refs only after offline processing
- show "Site plan available", "Sanction plan available", "Floor plan available"
  only when confidently classified
- keep raw document filenames in evidence, not search snippets
- do not render escrow/customer/legal documents as buyer media by default

### 4. Complaints

The screenshot pattern is important: RERA separates:

- complaints on this promoter
- complaints on this project

For Purva Palmbeach, the UI shows 107 promoter complaints and 15 project
complaints. The project complaints are mostly about refunds, delay possession,
delay compensation, holding charges/additional demands, and a parking/allocation
issue. Promoter-level complaints include many rows from other Puravankara
projects, so they are builder-track-record evidence, not project-specific
evidence.

Recommended model:

- `project_complaint_count`
- `project_complaint_open_count`
- `project_complaint_disposed_count`
- `project_complaint_theme_counts`
- `promoter_complaint_count`
- `promoter_complaint_theme_counts`
- `complaint_sample_subjects`, evidence-only

Recommended buyer summary:

- "15 project complaints; common themes: refund, delay compensation,
  possession delay."
- "107 promoter-level complaints across the builder's Karnataka RERA record."

Avoid:

- one raw `rera_complaints_count` without scope
- treating disposed complaints as automatically good or bad
- showing every complaint row in primary UI

### 5. Water, STP, Borewell, External Development

The pages often contain water/STP/borewell evidence, but it appears in different
places:

- scalar fields in development work tables
- document links
- status images
- descriptive text

Buyer use:

- "What is the water setup?"
- "Does the project have STP?"
- "Is it dependent on borewells?"

Recommended surface:

- use a separate "Water setup" or operating context section
- promote only stable typed facts like STP count/capacity, water supply mode,
  borewell count, when parsed confidently
- keep document-only evidence as proof, not as a confident scalar fact

### 6. Financial and Escrow Details

RERA exposes:

- land cost
- construction cost
- total project cost
- escrow bank/account/IFSC
- borrowing and mortgage flags

Buyer use is limited:

- borrowing/mortgage can be a risk signal
- escrow bank can help verify payment account
- cost numbers should not be treated as market price or construction quality

Recommended policy:

- show escrow bank and borrowing/mortgage only in a legal/payment checks area
- do not promote raw cost fields into buyer search
- never expose account number/IFSC prominently unless there is a clear payment
  verification workflow

## What Goes Where

Suggested buyer-facing organization:

| Section | Purpose | Facts |
| --- | --- | --- |
| RERA file | registration and legal timeline | status, RERA number, approval date, current target, original target, delay, land litigation |
| Project scale | physical size and density | acres, open area %, units, towers, max floors, homes/acre, parking summary |
| Documents | proof assets | site plan, sanction plan, floor plan availability and previews |
| Complaints | buyer risk themes | project complaint count/themes, promoter complaint count/themes, open/disposed split |
| Water setup | operating reality | water source, STP, borewell, drainage/sewage mode when confident |
| Builder record | promoter-level history | RERA project count, revocations, promoter complaints, delay pattern |

## What Should Not Go Into Primary UI

- raw RERA table dumps
- every uploaded document filename
- every complaint row
- escrow account number and IFSC
- raw RERA document IDs
- per-floor tower schedule rows
- construction cost, land cost, total project cost as prominent buyer facts
- customer documents, sale agreements, affidavits, or private attachments

These can remain in audit/evidence layers.

## Search and Comparison Candidates

Strong search/compare facts:

- RERA status
- completion timeline and delay
- land area/acres
- open area %
- unit count
- towers
- max floors
- units per acre
- site/sanction/floor plan availability
- project complaint count and themes
- land litigation
- builder revocations

Weak or later-stage facts:

- water supply mode
- STP capacity
- borewell count
- FAR sanctioned
- parking counts

Do not promote initially:

- raw document names
- raw complaint subjects
- escrow account details
- project cost fields
- every tower/floor row

## Extraction Implications

The current scraper is directionally right but needs table-aware parsing:

- separate nested complaint tabs by scope
- parse tables into normalized records before producing facts
- classify document links with source section, label, filename, and href
- capture tower schedule separately from top-level scale facts
- validate derived facts like open area % and units per acre
- preserve raw HTML and document manifests for audit/reprocessing

The product should not depend on RERA labels directly. RERA is a source; the
buyer-facing model should use generic concepts such as project scale, complaint
themes, documents, and water setup.

## Open Research Questions

- How many projects expose true unit-level floor plans vs only typical floor
  plans, brochures, or sanction drawings?
- Can OCR reliably classify plan pages without introducing false confidence?
- Which complaint statuses should count as currently unresolved?
- Should promoter complaint density be normalized by project count and project
  age?
- Should delay risk use RERA date extension, complaint themes, QPR freshness,
  or all three?
- Can we safely detect OC/completion details across completed projects?

