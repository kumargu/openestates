# Godrej Air RERA Fact Dump - 2026-07-30

Status: implementation audit for the compact RERA label surface.

## Inputs Checked

- Raw cached skill output:
  `data/cache/skills/fetch_rera_a252a3da236512f8.json`
- Cached RERA detail HTML:
  `data/cache/skills/rera_details/25.html`
- Silver RERA facts:
  `data/lake/silver/rera_legal_facts/source=rera/dt=2026-07/run_id=3549ec10-bd9b-4588-8171-175639d7a33c/facts/part-00000.parquet`
- Promoted serving bundle:
  `data/lake/serving/search_bundle/version=whitefield-15-folium-strict-osm-2026-07-27/facts/part-00000.parquet`

## Identity

| Field | Value |
| --- | --- |
| Project | Godrej Air |
| Society id | `society:godrej-air` |
| RERA-rooted society id | `society:rera-c1af3dd6c1581e3e` |
| Registration number | `PRM/KA/RERA/1251/446/PR/170819/000006` |
| Acknowledgement number | `PR/KN/170725/000006` |
| Portal URL | `https://rera.karnataka.gov.in/projectViewDetails` |
| Promoter | Godrej Housing Projects LLP |
| RERA status | APPROVED |

## Promoted RERA Facts In Parquet

These are available in the current silver/serving Parquet for
`society:godrej-air` and can drive labels now.

| Fact key | Value | Current label use |
| --- | --- | --- |
| `rera_number` | `PRM/KA/RERA/1251/446/PR/170819/000006` | shown at top of RERA modal |
| `rera_registered` | `true` | `Registered project` |
| `rera_status` | `APPROVED` | not shown; redundant with registration |
| `rera_promoter_name` | Godrej Housing Projects LLP | not shown in modal; builder already has its own surfaces |
| `rera_approved_on` | `19/08/2017` | not shown; low decision value |
| `rera_start_date` / `project_start_date` | `2017-07-26` | not shown |
| `rera_original_completion_date` / `project_original_completion_date` | `2022-12-31` | used through delay label |
| `rera_completion_date` / `project_revised_completion_date` | `2023-09-30` | used through delay label |
| `rera_delay_months` | `8` | `8 month delay` |
| `rera_land_litigation` | `false` | `No land litigation` |
| `rera_complaints_count` | `3` | `3 project complaints` through fallback |
| `rera_complaints_resolved_pct` | `100` | not shown yet; could become `Complaints closed` after better paired logic |
| `rera_builder_revocations` | `0` | hidden; zero is not useful unless no cautions |
| `rera_builder_states` | Maharashtra | not shown; not buyer-decision label by itself |
| `rera_project_type` | Residential/Group Housing | not shown |
| `rera_project_address` | Khatha No. 365, Sy.No. 13/6, 14/1, 16/4 and 16/5 of Hoodi Village, K.R. Puram Hobli, Bangalore East | not shown; page has location context elsewhere |
| `rera_survey_numbers` | `16/4, 16/5` | not shown; useful for deeper legal proof later |
| `rera_total_units` / `project_unit_count` | `487` | `487 homes` |
| `rera_num_towers` / `project_tower_count` | `8` | `8 towers` |
| `project_max_floor_count` | `16` | `16 floors` |
| `parking_total_car_count` | `487` | `1 parking/home` as ratio with units |
| `parking_offered_for_sale_count` | `68` | not shown; needs clearer buyer meaning |
| `rera_total_land_area_sqm` / `project_land_area_sqm` | `21448` | not shown; acres is easier |
| `project_land_area_acres` | `5.3` | `5.3 acres` |
| `project_units_per_acre` | `91.89` | `92 homes/acre` |
| `project_open_area_pct` | `10.01` | `10% open area` |
| `site_plan_asset_count` | `1` | `Site plan available` |
| `rera_plan_artifact_manifest` | one `site_plan` artifact | not directly shown; count drives label |
| `available_configurations` | `1BHK`, `2BHK`, `2.5BHK`, `3BHK` | not shown in RERA modal; better near plans/listings |
| `configuration_count` | `4` | not shown |
| `has_1bhk`, `has_2bhk`, `has_3bhk` | `true` | not shown |
| `stp_count` | `1` | not shown |

## Raw Source Facts Not Fully Promoted

The cached `fetch_rera` output has richer facts that are not all present in the
current promoted serving bundle.

| Source fact | Raw value | Gap |
| --- | --- | --- |
| `rera_project_complaints_count` | `3` | serving uses `rera_complaints_count`; project/promoter split is lost |
| `rera_project_complaints_disposed_count` | `3` | not promoted, so `No open complaints` cannot render yet |
| `rera_project_complaints_open_count` | `0` | not promoted |
| `rera_promoter_complaints_count` | `3` | not promoted as builder-scope relation |
| `rera_promoter_complaints_disposed_count` | `3` | not promoted |
| `rera_promoter_complaints_open_count` | `0` | not promoted |
| `rera_complaint_summary_manifest` | project/promoter themes: agreement/payment, amenities, builder conduct, cancellation, compensation, other | not promoted; good candidate for future issue-type tags |
| `brochure_asset_count` | `2` | not promoted in serving |
| `rera_noc_document_count` | `6` | not promoted in serving |
| `rera_affidavit_document_count` | `1` | not promoted in serving |
| `rera_document_manifest` | 22 document artifacts | not promoted in serving |

## Document Artifacts In Raw Source

Raw source grouped artifacts:

| Kind | Count | Examples |
| --- | ---: | --- |
| `affidavit` | 1 | Air Nxt and Air- affidavit.pdf |
| `brochure` | 2 | Godrej Air Whitefield-flipchart.pdf; CA - GODREJ AIR_compressed.pdf |
| `customer_contract_template` | 2 | RERA - ATS.pdf; Proforma Allotment Letter.pdf |
| `noc` | 6 | OC for Air - Final.pdf; BESCOM NOC dated 31-03-17.pdf; BWSSB NOC dated 28-03-2017.pdf |
| `promoter_financial_document` | 10 | Balance Sheet, Profit Loss, Audit Report, ITR files |
| `site_plan` | 1 | Site plan detected |

Do not show this as a table in the buyer UI. The useful label candidates are:
`Site plan available`, `OC available`, `NOCs available`, `Agreement draft
available`, and `Brochure available`, but only after those document facts are
promoted cleanly.

## Labels This Change Can Render For Godrej Air

From current promoted facts and config:

```text
8 month delay
3 project complaints
10% open area
487 homes
5.3 acres
92 homes/acre
1 parking/home
8 towers
16 floors
Registered project
No land litigation
Site plan available
```

Blocked until promotion catches up:

```text
No open complaints
Complaint themes: agreement/payment, amenities, builder conduct, cancellation, compensation
NOCs available
OC available
Agreement draft available
Brochure available
```

