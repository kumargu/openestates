# Search Evaluation Report — v1

Generated: 2026-07-19 17:54 UTC

## 1. Summary

- **Queries run:** 20 (errors: 0)
- **Automated checks:** 96/120 passed (80%)
- **Avg results per query:** 33.1
- **Also-consider triggered:** 0 total results across all queries

### Category pass rates

| Category | Queries | Checks passed |
|----------|---------|--------------|
| intent_clarity | 3 | 15/18 (83%) |
| soft_preference | 4 | 19/24 (79%) |
| negative_preference | 3 | 12/18 (67%) |
| archetype | 4 | 17/24 (71%) |
| contradiction | 2 | 11/12 (92%) |
| dual_intent | 1 | 4/6 (67%) |
| edge_case | 3 | 18/18 (100%) |

## 2. Per-Query Results

### Q01: 3 BHK Whitefield around 2.5 cr family friendly

- **Intent parsed:** area=Whitefield, bhk=3, budget=None, archetype=None
- **Results:** 11 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 3 BHK in Godrej Splendour | Whitefield | 0.49 | Partial match | 0 | False |
| 2 | 3 BHK in Prestige Park Grove | Whitefield | 0.48 | Partial match | 0 | False |
| 3 | 3 BHK in Prestige Raintree Park | Whitefield | 0.48 | Partial match | 0 | False |

**Failed checks:**
- `archetype_detection`: expected family, got None

### Q02: quiet apartment near metro whitefield

- **Intent parsed:** area=Whitefield, bhk=None, budget=None, archetype=None
- **Results:** 29 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 3 BHK in Sumadhura Capitol Residences | Whitefield | 0.52 | Good match | 0 | False |
| 2 | 4 BHK in Sumadhura Capitol Residences | Whitefield | 0.52 | Good match | 0 | False |
| 3 | 1 BHK in Godrej Splendour | Whitefield | 0.49 | Partial match | 0 | False |

**Failed checks:**
- `positive_preferences`: missing canonical keys: ['quiet', 'commute'] (got: [])

### Q03: good society for family in east bangalore

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 57 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 2 BHK in Abhee Celestial City | Sarjapur Road | 0.09 | Weak match | 0 | False |
| 2 | 3 BHK in Abhee Celestial City | Sarjapur Road | 0.09 | Weak match | 0 | False |
| 3 | 1 BHK in Brigade Woods | Whitefield | 0.09 | Weak match | 0 | False |

**Failed checks:**
- `archetype_detection`: expected family, got None

### Q04: something calmer for my parents, less chaos, more breathing room

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 5 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 1 BHK in Bhartiya City Nikoo Homes Phase 4 |  | 0.03 | Weak match | 0 | False |
| 2 | 2 BHK in Bhartiya City Nikoo Homes Phase 4 |  | 0.03 | Weak match | 0 | False |
| 3 | 3 BHK in Bhartiya City Nikoo Homes Phase 4 |  | 0.03 | Weak match | 0 | False |

**Failed checks:**
- `positive_preferences`: missing canonical keys: ['quiet', 'open_space'] (got: [])

### Q05: good family project but not fake luxury and not too dense

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 27 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 3 BHK in Prestige Glenbrook | Marathahalli | 0.07 | Weak match | 0 | False |
| 2 | 4 BHK in Prestige Glenbrook | Marathahalli | 0.07 | Weak match | 0 | False |
| 3 | 1 BHK in Prestige Lakeside Habitat | Whitefield | 0.07 | Weak match | 0 | False |

**Failed checks:**
- `archetype_detection`: expected family, got None
- `negative_preferences`: missing negative keys: ['premium', 'open_space'] (got: [])

### Q06: society that feels easier to live in, not just impressive on paper

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 21 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 1 BHK in Brigade Cornerstone Utopia |  | 0.27 | Partial match | 0 | False |
| 2 | 2 BHK in Brigade Cornerstone Utopia |  | 0.27 | Partial match | 0 | False |
| 3 | 3 BHK in Brigade Cornerstone Utopia |  | 0.27 | Partial match | 0 | False |

**Failed checks:**
- `positive_preferences`: missing canonical keys: ['livability'] (got: [])

### Q07: peaceful less crowded with greenery, okay to be slightly far from city

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 11 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 2 BHK in Abhee Celestial City | Sarjapur Road | 0.59 | Good match | 0 | False |
| 2 | 3 BHK in Abhee Celestial City | Sarjapur Road | 0.59 | Good match | 0 | False |
| 3 | 1 BHK in Bhartiya City Nikoo Homes Phase 4 |  | 0.24 | Weak match | 0 | False |

**Failed checks:**
- `positive_preferences`: missing canonical keys: ['quiet', 'open_space', 'greenery'] (got: [])

### Q08: avoid water issues, no tanker dependency

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 13 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 3 BHK in Prestige Waterford | Pattandur Agrahara | 0.26 | Partial match | 0 | False |
| 2 | 4 BHK in Prestige Waterford | Pattandur Agrahara | 0.26 | Partial match | 0 | False |
| 3 | 4 BHK in Prestige Somerville | Marathahalli—Whitefield Road | 0.04 | Weak match | 0 | False |

**Failed checks:**
- `negative_preferences`: missing negative keys: ['water_issues'] (got: [])
- `concern_surfacing`: no concerns found in top 5 results (expected some)

### Q09: don't want maintenance headaches or shady builder

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 61 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 3 BHK in Shriram Esquire | Koramangala | 0.13 | Weak match | 0 | False |
| 2 | 4 BHK in Shriram Esquire | Koramangala | 0.13 | Weak match | 0 | False |
| 3 | 2 BHK in Abhee Celestial City | Sarjapur Road | 0.09 | Weak match | 0 | False |

**Failed checks:**
- `negative_preferences`: missing negative keys: ['good_maintenance', 'builder_trust'] (got: [])
- `concern_surfacing`: no concerns found in top 5 results (expected some)

### Q10: not too packed, avoid highway noise and construction dust

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 14 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 3 BHK in Prestige Glenbrook | Marathahalli | 0.07 | Weak match | 0 | False |
| 2 | 4 BHK in Prestige Glenbrook | Marathahalli | 0.07 | Weak match | 0 | False |
| 3 | 2 BHK in Prestige Raintree Park | Whitefield | 0.07 | Weak match | 0 | False |

**Failed checks:**
- `negative_preferences`: missing negative keys: ['open_space', 'quiet'] (got: [])
- `concern_surfacing`: no concerns found in top 5 results (expected some)

### Q11: best investment opportunity in whitefield under 1.5 cr

- **Intent parsed:** area=Whitefield, bhk=None, budget=15000000, archetype=None
- **Results:** 34 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 1 BHK in Brigade Woods | Whitefield | 0.23 | Weak match | 0 | False |
| 2 | 1 BHK in Godrej Splendour | Whitefield | 0.23 | Weak match | 0 | False |
| 3 | 2 BHK in Godrej Splendour | Whitefield | 0.23 | Weak match | 0 | False |

**Failed checks:**
- `archetype_detection`: expected investor, got None

### Q12: safe legal paperwork, reliable builder, no risk

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 54 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 2 BHK in Abhee Celestial City | Sarjapur Road | 0.13 | Weak match | 0 | False |
| 2 | 3 BHK in Abhee Celestial City | Sarjapur Road | 0.13 | Weak match | 0 | False |
| 3 | 2 BHK in Casagrand Flamingo | HSR Layout | 0.13 | Weak match | 0 | False |

**Failed checks:**
- `archetype_detection`: expected riskAverse, got None
- `positive_preferences`: missing canonical keys: ['legal_safety', 'builder_trust'] (got: [])

### Q13: affordable 2BHK for young couple, good commute

- **Intent parsed:** area=None, bhk=2, budget=None, archetype=None
- **Results:** 27 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 2 BHK in Abhee Celestial City | Sarjapur Road | 0.1 | Weak match | 0 | False |
| 2 | 2 BHK in Casagrand Flamingo | HSR Layout | 0.07 | Weak match | 0 | False |
| 3 | 2 BHK in Godrej Park Retreat | Sarjapur Road | 0.07 | Weak match | 0 | False |

**Failed checks:**
- `archetype_detection`: expected valueBuyer, got None
- `positive_preferences`: missing canonical keys: ['commute', 'value_for_money'] (got: [])

### Q14: premium 4BHK, builder reputation matters, willing to pay more

- **Intent parsed:** area=None, bhk=4, budget=None, archetype=None
- **Results:** 41 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 4 BHK in Prestige Lavender Fields | Varthur | 0.1 | Weak match | 0 | False |
| 2 | 4 BHK in Prestige Glenbrook | Marathahalli | 0.07 | Weak match | 0 | False |
| 3 | 4 BHK in Prestige Lakeside Habitat | Whitefield | 0.07 | Weak match | 0 | False |

**Failed checks:**
- `archetype_detection`: expected luxuryBuyer, got None
- `positive_preferences`: missing canonical keys: ['premium'] (got: [])

### Q15: cheap but also premium quality

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 28 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 3 BHK in Prestige Glenbrook | Marathahalli | 0.07 | Weak match | 0 | False |
| 2 | 4 BHK in Prestige Glenbrook | Marathahalli | 0.07 | Weak match | 0 | False |
| 3 | 1 BHK in Prestige Lakeside Habitat | Whitefield | 0.07 | Weak match | 0 | False |

### Q16: near whitefield but avoid traffic

- **Intent parsed:** area=Whitefield, bhk=None, budget=None, archetype=None
- **Results:** 35 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 1 BHK in Brigade Woods | Whitefield | 0.13 | Weak match | 0 | False |
| 2 | 2 BHK in Brigade Woods | Whitefield | 0.13 | Weak match | 0 | False |
| 3 | 3 BHK in Brigade Woods | Whitefield | 0.13 | Weak match | 0 | False |

**Failed checks:**
- `negative_preferences`: missing negative keys: ['commute'] (got: [])

### Q17: good for family AND good investment

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 3 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 3 BHK in Casagrand Flamingo | HSR Layout | 0.04 | Weak match | 0 | False |
| 2 | 4 BHK in Casagrand Flamingo | HSR Layout | 0.03 | Weak match | 0 | False |
| 3 | 2 BHK in Casagrand Flamingo | HSR Layout | 0.03 | Weak match | 0 | False |

**Failed checks:**
- `archetype_detection`: expected family, got None
- `positive_preferences`: missing canonical keys: ['family_friendly', 'investment'] (got: [])

### Q18: apartments

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 9 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 1 BHK in Brigade Altair Apartments |  | 0.33 | Partial match | 0 | False |
| 2 | 2 BHK in Brigade Altair Apartments |  | 0.33 | Partial match | 0 | False |
| 3 | 3 BHK in Brigade Altair Apartments |  | 0.33 | Partial match | 0 | False |

### Q19: tell me what's actually good in bangalore

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 146 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 2 BHK in Prestige Somerville | Marathahalli—Whitefield Road | 0.5 | Good match | 0 | False |
| 2 | 3 BHK in Prestige Somerville | Marathahalli—Whitefield Road | 0.5 | Good match | 0 | False |
| 3 | 4 BHK in Prestige Somerville | Marathahalli—Whitefield Road | 0.5 | Good match | 0 | False |

### Q20: something like prestige lakeside but cheaper

- **Intent parsed:** area=None, bhk=None, budget=None, archetype=None
- **Results:** 37 primary, 0 also-consider

**Top 3:**

| # | Title | Area | Score | Label | Concerns | Has explanation |
|---|-------|------|-------|-------|----------|----------------|
| 1 | 3 BHK in Prestige Lakeside Habitat | Whitefield | 0.87 | Strong match | 0 | False |
| 2 | 1 BHK in Prestige Lakeside Habitat | Whitefield | 0.87 | Strong match | 0 | False |
| 3 | 2 BHK in Prestige Lakeside Habitat | Whitefield | 0.87 | Strong match | 0 | False |

## 3. Pattern Analysis

_Fill in manually after reviewing query-by-query results above._

### What works well
- TBD

### What fails
- TBD

### Structural issues
- TBD

## 4. Priority Fixes for Days 37-38

_Ranked list of issues to address, derived from failing checks and manual evaluation._

1. TBD
2. TBD
3. TBD
