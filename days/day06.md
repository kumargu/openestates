# days/day06.md

# OpenEstates v2
## Day 6 – Seed Dataset Build, Selective Listing Extraction, and Area/Society Profiles

Before starting today, read:
- `CLAUDE.md`
- `LEARNING.md`
- `DECISIONS_INDEX.md`
- `docs/openestates_v2_surfaces_and_data.md`
- `docs/day05_cleanup_note.md`

Today is a **data-shaping day**.

We are not building the full product yet. We are building the first **credible seed dataset** that will make the web UI feel real enough to evaluate product direction.

The purpose of Day 6 is to create a small, high-quality dataset that supports:
- homepage sample searches
- results page cards
- property detail pages
- shortlist / compare flows
- transparency widgets

This dataset should be realistic enough to make the product feel believable, but small enough to be manually reviewed and corrected.

---

## 1. Day 6 Goal

By the end of Day 6, OpenEstates should have a usable seed dataset consisting of:

- `data/seed/properties.json`
- `data/seed/area_profiles.json`
- `data/seed/societies.json`

This dataset should include:
- 15–25 properties
- around 5 Bengaluru micro-markets / areas
- a small set of named societies / projects
- enough fields to support the transparency widgets defined in Day 5

We are explicitly **not** building a general-purpose crawler today.

We are building a **small, curated or semi-curated seed corpus**.

---

## 2. Product reason for today

The product cannot be judged properly with abstract schemas alone.

To evaluate whether OpenEstates v2 feels right, we need:
- believable listings
- believable market context
- believable society context
- enough images and metadata to render real-looking pages

Pure synthetic data is useful for engine experiments, but not enough for product feel.

So Day 6 is about creating a dataset that is:
- realistic enough to support design and product judgment
- small enough to inspect manually
- structured enough to support later ranking and enrichment

---

## 3. Source strategy for Day 6

### 3.1 dm8.in as selective reference / extraction source
Use `dm8.in` as a **reference listing source and selective extraction candidate**, especially for Bengaluru listing structure and project names.

Important:
- Do **not** build broad crawling infrastructure
- Do **not** attempt to scrape large volumes
- Do **not** try to mirror the website
- Do **not** optimize for scale

Instead, use dm8 selectively to:
- identify plausible Bengaluru listings / projects / builders / price ranges
- capture a small sample of listing-style records
- normalize them into the OpenEstates schema

The goal is seed quality, not extraction scale.

### 3.2 Google reviews and images
Do **not** implement Google API integration today.

However, the schema should leave room for later enrichment with:
- Google review summaries
- Google rating / count
- place or society images
- other place-level signals

Today, you may include placeholder fields or a small amount of manually added summary text where needed, but do not build API integration or scraping.

### 3.3 Reddit signals
Do not build Reddit fetching today either.

However, area and society schemas should include fields like:
- `reddit_signals`
- `common_concerns`
- `community_notes`

These may be empty or lightly seeded today.

---

## 4. Deliverables

### 4.1 `data/seed/properties.json`
Create a list of 15–25 seed property objects.

These properties should be spread across about 5 Bengaluru micro-markets such as:
- Whitefield
- Sarjapur Road
- Bellandur
- HSR Layout
- North Bangalore / Hebbal / Yelahanka cluster

Each property should feel like a plausible resale or discoverable listing.

Each property should include enough fields to power:
- results cards
- property detail pages
- transparency widgets

### 4.2 `data/seed/area_profiles.json`
Create 5 area profiles.

Each area profile should capture:
- area name
- city
- median price/sqft estimate
- trend direction
- metro access profile
- airport noise / externality profile
- traffic / commute profile
- waterlogging or other local concern profile
- short summary text
- optional `reddit_signals` field

These area profiles do not need to be “fact perfect.” They need to be **reasonable and believable** enough for product design.

### 4.3 `data/seed/societies.json`
Create a small set of society / project profiles tied to properties.

Each society entry should include:
- society or project name
- area
- builder (if known)
- society quality summary
- maintenance sentiment summary
- livability summary
- optional review count / placeholder count
- optional future Google enrichment hooks
- optional common positives / common complaints

These society records are critical because OpenEstates property pages should feel richer than generic listing pages.

---

## 5. Required schema guidance

### 5.1 Property schema
Each property should include at minimum fields like:

- `id`
- `title`
- `area`
- `city`
- `society_id`
- `builder_name`
- `property_type`
- `bhk`
- `price_in_inr`
- `price_per_sqft`
- `super_builtup_sqft`
- `floor`
- `facing`
- `possession_status`
- `listing_type` (resale / ready-to-move / under-construction if applicable)
- `metro_distance_mins`
- `airport_noise_score`
- `waterlogging_risk_score`
- `traffic_score`
- `society_quality_score`
- `document_trust_score` (placeholder synthetic field is okay)
- `images`
- `hero_image`
- `description_summary`
- `transparency_tags`
- `match_reason_placeholders`
- `tradeoff_notes`
- `source_reference`

Use the Day 5 blueprint as the source of truth if it contains a fuller schema.

### 5.2 Area profile schema
Each area profile should include enough fields to render:
- area chips / summaries
- “Price vs area median”
- “Area signals”
- trend summaries

Suggested fields:
- `id`
- `name`
- `city`
- `median_price_per_sqft`
- `trend_direction`
- `trend_summary`
- `metro_access_summary`
- `airport_noise_summary`
- `traffic_summary`
- `waterlogging_summary`
- `livability_summary`
- `reddit_signals`
- `community_notes`

### 5.3 Society schema
Each society/profile should include enough fields to support:
- society/livability widget
- review summary area
- common complaints / positives
- future review enrichment

Suggested fields:
- `id`
- `name`
- `area`
- `builder_name`
- `summary`
- `maintenance_sentiment`
- `livability_sentiment`
- `common_positives`
- `common_complaints`
- `review_summary`
- `future_google_place_name`
- `future_google_place_id`
- `future_review_enrichment_status`

---

## 6. What Claude should build today

### Task 1 — Create the seed folder structure
Create:
- `data/seed/properties.json`
- `data/seed/area_profiles.json`
- `data/seed/societies.json`

If needed, also create:
- `data/raw/` for raw captured reference snippets
- `scripts/` or `tools/` for a tiny extraction helper

But keep this lightweight.

### Task 2 — Build a tiny selective extraction helper (optional but useful)
You may create a very small script that helps capture or normalize listing-style data from dm8 reference pages.

This script should:
- be minimal
- operate on a small number of pages or copied snippets
- extract only what is useful
- never attempt site-scale crawling

Even if a script is created, manual correction is expected.

### Task 3 — Populate the seed dataset
Populate the three JSON files with realistic, product-useful data.

This is the main output of the day.

### Task 4 — Add a short data note
Create a small markdown note, for example:
- `docs/day06_data_note.md`

This should explain:
- what source posture was used
- what fields are synthetic vs curated vs semi-curated
- what assumptions were made
- what future enrichment hooks exist

---

## 7. Important product guidance

When shaping the data, always ask:
- will this help the property page feel transparent?
- will this help the results page feel differentiated?
- will this help the shortlist / compare page feel useful?
- does this support one of the transparency widgets?

Do not add fields just because they are easy to scrape.

Prefer product-relevant fields over exhaustiveness.

---

## 8. Constraints

Do not build today:
- broad crawler infrastructure
- Google Places API integration
- Reddit fetching pipeline
- ranking engine
- React page implementation
- Axum API implementation
- real deduplication / normalization pipelines at scale
- image downloading infrastructure beyond a tiny controlled sample if necessary

This is a **seed data** day, not a platform data-ingestion day.

---

## 9. Cleanup expectations

Claude, if older prototype files or assumptions are now getting in the way of the seed-data direction, mention them clearly.

You do not need to rewrite the whole repo today, but if:
- old schemas conflict with the new product schema
- old assumptions are misleading
- the repo needs a cleaner place for `data/seed/`

then make those adjustments and explain them.

---

## 10. Manual verification checklist

By the end of Day 6, verify:

- `data/seed/properties.json` exists and has 15–25 realistic records
- `data/seed/area_profiles.json` exists and has 5 area profiles
- `data/seed/societies.json` exists and supports society/livability style content
- properties reference valid `society_id` and `area`
- image fields are present in a usable way (URLs or placeholders)
- records are plausible enough to make a property page feel real
- a short data note exists describing what was done

---

## 11. Expected success definition

At the end of Day 6, OpenEstates should have a believable seed dataset that can power the first real web UI surfaces.

If Day 6 is successful, Day 7 can move into:
- wiring the first React pages to this seed data
or
- building Axum endpoints around the seed data

The important thing is that the product can now start feeling real.