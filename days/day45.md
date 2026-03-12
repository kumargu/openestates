# Day 45: Seller Data Model, Seed Data, and Interest API

## Goal
Lay the foundational data layer for Sprint 2: seller model, seed seller data, interest events, and the API endpoints that serve them.

## Product Reason
Sprint 2 connects buyers and sellers. Before building any UI, the backend must have typed seller entities, interest events, and API contracts. Day 45 creates the data spine that every subsequent Sprint 2 day builds on. Without this foundation, seller dashboards, interest buttons, and profile completeness calculations have nothing to query.

## Deliverables

### 1. Seller data model in Rust (`backend/src/models/seller.rs`)
Define a `Seller` struct with fields aligned to the vision doc's 7-step registration:

```
id: String
name: String
email: Option<String>
phone: Option<String>
property_ids: Vec<String>
has_basic_info: bool
has_property_prompt: bool
property_prompt: Option<String>
has_details: bool
has_pricing: bool
has_documents: bool
has_photos: bool
has_society_info: bool
documents_provided: Vec<String>
verified: bool
created_at: String
updated_at: String
```

Also add a `SellerCard` for list-view responses (id, name, property_ids count, completeness_pct, verified).

Register in `backend/src/models/mod.rs`.

### 2. Interest event model (`backend/src/models/interest.rs`)
Define an `Interest` struct:

```
id: String
property_id: String
buyer_name: Option<String>
buyer_contact: Option<String>
created_at: String
```

Also add `InterestResponse` for the POST response and `InterestCount` for aggregation.

### 3. Seed seller data (`data/sellers/sellers.json`)
Create 10 dummy sellers at varying completeness levels (30%-100%). At least 3 verified. Each links to 1-3 existing property IDs. At least 2 with property_prompt filled.

### 4. Add `seller_id` to Property model
Add `seller_id: Option<String>` to `Property` in `backend/src/models/property.rs` with `#[serde(default)]`. Update 10-15 properties in `data/seed/properties.json` to include `seller_id`. Add to `PropertyCard` as `Option<String>`.

### 5. Load sellers in AppState
- Add `sellers: Vec<Seller>` to AppState
- Load from `data/sellers/sellers.json` at startup

### 6. Seller API endpoints (`backend/src/routes/sellers.rs`)

| Method | Path | Handler | Returns |
|--------|------|---------|---------|
| GET | `/api/sellers` | `list_sellers` | `Vec<SellerCard>` |
| GET | `/api/sellers/{id}` | `get_seller` | Full `Seller` with linked property details |

Completeness percentage computed at response time from `has_*` fields (each = ~14.3%, 7 steps = 100%).

### 7. Interest API endpoint (`backend/src/routes/interests.rs`)

| Method | Path | Handler | Returns |
|--------|------|---------|---------|
| POST | `/api/interests` | `express_interest` | `InterestResponse` with id and status |
| GET | `/api/properties/{id}/interests/count` | `get_interest_count` | `InterestCount` with count |

Interest events persisted to `data/interests/{property_id}.jsonl` (append-only JSONL).

### 8. Frontend type contracts (`frontend/src/lib/types.ts`, `frontend/src/lib/api.ts`)
Add TypeScript types for SellerCard, Seller, InterestRequest, InterestResponse, InterestCount.
Add API functions: getSellers, getSeller, expressInterest, getInterestCount.
Add `seller_id?: string` to PropertyCard and PropertyDetailResponse.

## Technical Guidance
- Follow `.claude/skills/add-api-endpoint.md`
- Reference `backend/src/routes/claims.rs` for file-persisted POST pattern
- Use `#[serde(default)]` for backward compat on seller_id
- Completeness is derived, not stored
- Interest ID: `format!("{}-{}-{}", property_id, timestamp_millis, random_4_digits)`

## Constraints
- No new frontend pages or UI components today
- No authentication/authorization
- No chat/messaging
- `cargo check` and `cargo clippy -- -D warnings` must pass zero warnings
- `npm run build` must pass
- Minimize new crate dependencies

## Success Criteria
1. `cargo check` — zero warnings
2. `cargo clippy -- -D warnings` — zero warnings
3. `npm run build` — passes
4. `curl /api/sellers` returns 10 sellers with completeness percentages
5. `curl /api/sellers/seller-001` returns full seller with property links
6. `curl -X POST /api/interests -d '{"property_id":"prop-w-001"}'` returns 201
7. `curl /api/properties/prop-w-001/interests/count` returns count
8. Properties with seller_id show it in API responses
9. `data/sellers/sellers.json` has 10 sellers at 30%-100% completeness
10. Interest events persisted in `data/interests/` as JSONL
