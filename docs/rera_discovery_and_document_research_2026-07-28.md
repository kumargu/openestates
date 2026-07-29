# RERA Discovery And Document Research - 2026-07-28

Status: research note only. This extends
`docs/rera_fact_organization_research_2026-07-28.md`.

## Why This Pass

The earlier RERA pass sampled named projects. This pass inspected how the
Karnataka RERA site itself expects projects to be found, then sampled projects
from that flow. The main conclusion is that discovery should start from the
RERA search/listing structure, and document extraction should start from the
uploaded-document labels before OCR.

## RERA Project Discovery

The project search form posts to `projectViewDetails` with these form fields:

```text
project       project name
firm          promoter / firm name
appNo         acknowledgement number
regNo         registration number
district      district name
subdistrict   taluk name
```

The detail modal is loaded by JavaScript:

```text
POST projectDetails
data: { action: <numeric detail id> }
```

The district dropdown is populated in HTML. The taluk dropdown is populated by:

```text
POST getSubDistrictByDstName
data: DSTID=<district name>&CID=SubDistrict-projectDist
```

For Bengaluru Urban, RERA returns:

| District | District code | Taluk | Taluk code in registration number |
| --- | ---: | --- | ---: |
| Bengaluru Urban | 1251 | Anekal | 308 |
| Bengaluru Urban | 1251 | Bengaluru East | 446 |
| Bengaluru Urban | 1251 | Bengaluru North | 309 |
| Bengaluru Urban | 1251 | Bengaluru South | 310 |
| Bengaluru Urban | 1251 | Yelahanka | 472 |

This means `PRM/KA/RERA/1251/446/...` is not just an opaque string. It can be
used to target Bengaluru Urban / Bengaluru East projects from the full listing
payload.

## Better Discovery Strategy

Recommended crawler order:

1. Fetch `viewAllProjects?language=en` and parse the embedded listing payload.
   It currently provides acknowledgement number, registration number, project
   name, and promoter name.
2. Decode district/taluk from the registration number where present.
3. Use exact `regNo` search to get the numeric detail ID. This is the cleanest
   lookup when a registration number exists.
4. Use district+subdistrict search to enumerate projects by RERA geography.
5. Use project-name search only as a fallback for fuzzy matching from external
   property/listing names.
6. Use promoter search only after verifying parameter behavior. In this pass,
   district+taluk search behaved predictably; promoter filtering needs separate
   validation before depending on it.

Current implementation note: `pipeline/skills/fetch_rera.py` posts fields named
`promoter`, `registrationNo`, `applicationNo`, and `taluk` in one path. The
live form uses `firm`, `regNo`, `appNo`, and `subdistrict`. Project-name search
works today, but non-project filters should be aligned with the live form before
we rely on them.

## Search Coverage Snapshot

Using the RERA district+taluk search for Bengaluru Urban:

| Taluk | RERA result rows |
| --- | ---: |
| Bengaluru East | 1,279 |
| Anekal | 863 |
| Bengaluru North | 695 |
| Bengaluru South | 1,075 |
| Yelahanka | 159 |

The cached RERA listing has 9,793 projects total. For Bengaluru Urban codes in
that listing:

| Query/code | Listing hits |
| --- | ---: |
| `1251/446` Bengaluru East | 1,276 |
| `1251/308` Anekal | 863 |
| `Godrej` | 37 |
| `Prestige` | 92 |
| `Brigade` | 78 |
| `Assetz` | 44 |
| `Sobha` | 123 |
| `Purva` | 25 |

## Systematic Detail Sample

Sampled 20 approved residential projects across the five Bengaluru Urban taluks
using district+taluk search. This is separate from the earlier named-project
sample.

| Taluk | Sampled projects |
| --- | --- |
| Bengaluru East | Godrej United, Godrej Air, Prestige Boulevard, Prestige Kew Gardens |
| Anekal | Uniworld Resorts, Sowparnika Tharangini Phase I, Signature Classic, Radiant Spencer Annex |
| Bengaluru North | Sky Asta, Prestige Dejavu, BCD Paradiso, Vajram Tiara |
| Bengaluru South | Sobha Silicon Oasis Phase 1, Chartered Hummingbird, Mahendra Aarna, Peninsula Heights |
| Yelahanka | Pelican Grove, Dharani Homes, Fort House, Velpula Pride |

Observed uploaded-document field labels were much richer than our current
document classifier. Common labels in all or most old-style detail pages:

- PAN Card
- Commencement Certificate
- Approved Building/Plotting Plan
- Approved Layout Plan
- Proforma For Sale Deed
- Proforma of Agreement for Sale
- Existing Layout Plan
- Existing Section Plan and Specification
- Land documents and Location
- Approved Section Of Building/Infrastructure Plan of Plotting
- Area Development Plan Of Project Area
- Performa of Allotment Letter
- Brochure of Current Project
- All NOCs from Authority
- Project Specification
- Encumbrance Certificate
- Section 3(1) Notarized Affidavit
- JD Affidavit Cum Declaration
- Title Deed
- Joint Development Agreement
- Conversion Certificate
- Fire Force Department
- Airport Authority of India
- BESCOM
- BWSSB
- KSPCB
- SEIAA
- BMRCL
- Structural safety certificate
- Sectional Drawing of the apartments
- Advocate Search Report
- Declaration (Form B)

This is enough structure to avoid blind OCR for the first classification pass.
The source field label often matters more than the filename. For example,
`Approved Building/Plotting Plan` should influence classification even when the
uploaded filename is generic or wrong.

## Recent Registration Sample

Sampled 10 recent Bengaluru Urban projects from the RERA listing by exact
registration number:

| Project | Taluk | Approved | Completion | Observed uploaded docs |
| --- | --- | --- | --- | --- |
| Amberstone Vectra | Anekal | 23/07/2026 | 31/07/2032 | Affidavit (Annexure - 49) only |
| Assetz KVN Niwa & Neo | Yelahanka | 22/07/2026 | 30/09/2031 | Affidavit (Annexure - 49) only |
| The Roots by Elegance Infra | Anekal | 17/07/2026 | 31/07/2027 | Affidavit (Annexure - 49) only |
| Maithri Springwoods | Bengaluru East | 23/07/2026 | 15/07/2031 | Affidavit (Annexure - 49) only |
| Arvind Sylva | Bengaluru East | 09/07/2026 | 31/08/2031 | Affidavit (Annexure - 49) only |
| Shree Gruha Kalpa | Bengaluru South | 18/07/2026 | 31/12/2028 | Affidavit (Annexure - 49) only |
| Fiorana at Beaumont Estate Phase 1 | Yelahanka | 20/07/2026 | 31/03/2032 | Affidavit (Annexure - 49) only |
| Purva Heritage | Bengaluru South | 09/07/2026 | 31/08/2030 | Affidavit (Annexure - 49) only |
| Vasundra SS Valley | Bengaluru North | 09/07/2026 | 31/12/2028 | Affidavit (Annexure - 49) only |
| Havena by KSR | Bengaluru North | 23/06/2026 | 31/12/2029 | Affidavit (Annexure - 49) only |

This suggests document richness is lifecycle-dependent. Fresh registrations may
not expose the same uploaded-document set as older projects, or the public page
may show a reduced initial set. A product surface should avoid saying "missing
site plan" without considering project age and RERA page lifecycle.

## Document Extraction Implications

The uploaded-document tab should become a typed document manifest before any
PDF/OCR work:

```text
project_entity_id
registration_number
numeric_detail_id
source_tab = uploaded_documents
source_section_heading
source_field_label
uploaded_filename
href
doc_id
document_kind
buyer_visibility
observed_at
```

Classification should use both field label and filename:

| Source labels / names | Candidate kind | Buyer use |
| --- | --- | --- |
| Approved Layout Plan, Existing Layout Plan, site plan | site_plan | site layout proof |
| Approved Building/Plotting Plan, sanction plan, approved plan | sanction_plan | approval proof |
| Sectional Drawing, Existing Section Plan | section_plan | technical evidence, preview optional |
| Area Development Plan | development_plan | project layout / infra evidence |
| Brochure of Current Project, brochure | brochure | buyer media, but lower authority |
| Floor Plan, Typical Floor Plan | floor_plan | BHK comparison when mappable |
| Project Specification, Specifications | specification | evidence-only or compact feature summary |
| Commencement Certificate | commencement_certificate | legal/timeline evidence |
| Encumbrance Certificate, EC | encumbrance_certificate | legal evidence |
| Title Deed, Sale Deed, JDA, GPA, Khata | title_or_land_document | evidence-only |
| BESCOM, BWSSB, Fire, Airport, KSPCB, SEIAA, BMRCL | noc | infrastructure/legal evidence |
| Agreement for Sale, Allotment Letter, Sale Deed proforma | customer_contract_template | not primary buyer media |
| PAN, balance sheet, P&L, auditor report, ITR | promoter_financial_document | not buyer primary |
| Affidavit Annexure 49, Form B | affidavit | evidence-only |

OCR should be reserved for:

- extracting plan previews from a document already classified as plan-like
- detecting floor-plan pages inside brochures
- reading project specification text where labels are not enough
- validating if a generic uploaded filename is mislabeled

OCR should not be the first classifier for uploaded RERA documents.

## Complaint Extraction Implications

Complaint tabs should be parsed by nested tab scope:

```text
menu-comp   -> complaints on this promoter
menu-comp2  -> complaints on this project
```

The tab label itself carries counts, for example:

```text
Complaints On this Promoter (107)
Complaints On this Project (15)
```

The parser should capture:

- scope: promoter or project
- count from tab label
- row-level complaint number, date, subject, project name, status, order by
- status groups: disposed, under enquiry, posted for orders, other
- theme classification from complaint subject

Primary UI should show scoped summaries, not one raw complaint count.

## Product Takeaways

- RERA discovery can be deterministic: listing payload, registration number,
  district/taluk search, then detail ID.
- Search by external society name should be fallback, not the main source of
  truth.
- Uploaded-document labels are already meaningful and should drive the first
  document taxonomy.
- Recent projects need lifecycle-aware messaging because they may expose only
  affidavits initially.
- The document manifest should be first-class even if no preview is generated.
- OCR is still useful, but only after label-based classification and only for
  selected document kinds.

