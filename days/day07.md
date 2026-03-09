Markdown
# days/day07.md

# OpenEstates v2  
## Day 7 – Backend API Layer and First End-to-End Localhost Flow

Before starting today, read:

- `CLAUDE.md`
- `LEARNING.md`
- `docs/openestates_v2_surfaces_and_data.md`
- `docs/day06_data_note.md`
- `data/seed/properties.json`
- `data/seed/area_profiles.json`
- `data/seed/societies.json`

Day 6 produced the first credible seed dataset.  
We now have enough structured data to power the core product surfaces.

Day 7 introduces the **Rust + Axum backend API layer** that serves this seed data to the frontend.

This is the moment OpenEstates transitions from **data files + product design → a real application architecture.**

---

# 1. Goal

The goal of Day 7 is to build the **backend API scaffold** and expose the seed dataset through structured endpoints.

By the end of Day 7 we should have:

- a working **Rust backend using Axum**
- seed dataset loaded into memory at startup
- API endpoints serving properties, areas, and shortlist state
- frontend capable of fetching real data from localhost
- the first **true end-to-end product flow**

The backend will **not** include:

- a database
- ranking logic
- contextual search
- mutation APIs

Day 7 is purely about **structured APIs around the seed dataset.**

---

# 2. Product Reason

Up to Day 6 the system still behaves like a **design artifact**.

Frontend pages cannot safely depend on raw JSON files long-term.

Introducing the backend now achieves three important things.

---

## 2.1 Stabilizes the domain schema

The backend becomes the contract layer between:

- product UI
- seed data
- future ranking engine
- future data pipelines

Frontend pages must rely on **API responses**, not raw files.

This prevents accidental schema drift.

---

## 2.2 Prevents frontend/data coupling

If the frontend imports JSON directly:

- changing seed data breaks UI
- joins become messy
- domain logic spreads everywhere

Instead:


Frontend → API → Seed data


The backend performs joins and returns **UI-ready responses**.

---

## 2.3 Prepares for ranking and context search

Later days will add:

- contextual ranking
- match explanation generation
- filtering
- shortlist persistence
- enrichment pipelines

All of these belong behind the **backend boundary**.

Day 7 establishes that architecture early.

---

# 3. Deliverables

By the end of Day 7 the repository should contain a minimal but structured backend.


backend/
Cargo.toml

src/
main.rs
state.rs
data_loader.rs

models/
  property.rs
  area_profile.rs
  society.rs

routes/
  properties.rs
  areas.rs
  shortlist.rs

The backend should remain **small, explicit, and inspectable.**

No database is required.

---

# 4. API Endpoints

The backend must expose **four endpoints** corresponding to the four core product surfaces.

---

## GET /

Basic health endpoint.

Example response:

```json
{
  "service": "openestates-api",
  "status": "ok"
}

This verifies the backend is running.

GET /api/properties

Returns property cards used by the results page.

Important: this endpoint should return UI-ready card data, not raw property records.

Example response:

JSON
[
  {
    "id": "prop_001",
    "title": "3BHK in Prestige Lakeside Habitat",
    "area": "Whitefield",
    "price": 12500000,
    "price_per_sqft": 8621,
    "bhk": 3,
    "sqft": 1450,
    "society_name": "Prestige Lakeside Habitat",
    "hero_image": "...",
    "transparency_tags": [
      "below_area_median",
      "ready_to_move"
    ]
  }
]

Important fields:

society_name

hero_image

transparency_tags

Avoid exposing raw internal schema fields.

GET /api/properties/{id}

Returns full property detail data.

This endpoint must join information from three sources:

properties.json

societies.json

area_profiles.json

Example response:

JSON
{
  "property": { ... },
  "society": { ... },
  "area": { ... }
}

This endpoint powers the property detail page.

GET /api/areas/{id}

Returns full area profile.

Example response:

JSON
{
  "id": "whitefield",
  "name": "Whitefield",
  "median_price_per_sqft": 9200,
  "trend_direction": "up",
  "metro_access_summary": "...",
  "traffic_summary": "...",
  "waterlogging_summary": "...",
  "livability_summary": "..."
}

This endpoint powers:

homepage area cards

property detail area widgets

GET /api/shortlist

For Day 7 this endpoint is a stub.

Example response:

JSON
{
  "shortlist": ["prop_003", "prop_010"]
}

Persistence will be implemented later.

5. Technical Guidance
5.1 Rust dependencies

Use minimal crates:

axum
tokio
serde
serde_json
tower-http (optional CORS)

Avoid unnecessary frameworks.

5.2 Data loading

Create src/data_loader.rs.

Responsibilities:

read JSON from data/seed/

deserialize using serde

store in memory

Example state:

Rust
pub struct AppState {
    pub properties: Vec<Property>,
    pub areas: Vec<AreaProfile>,
    pub societies: Vec<Society>,
}

Wrap in Arc<AppState> so routes can share safely.

5.3 Models

Create structs:

models/property.rs
models/area_profile.rs
models/society.rs

Derive:

Serialize
Deserialize
Clone
Debug

Keep the domain model explicit and readable.

5.4 Route organization

Routes should live in:

routes/
  properties.rs
  areas.rs
  shortlist.rs

Each file should:

define handler functions

accept shared AppState

return JSON responses

main.rs should only:

initialize state

register routes

start server

5.5 Property join logic

The property detail endpoint should:

find property

lookup society by society_id

lookup area by area

Example flow:

property_id
   ↓
find property
   ↓
society_id → society
area → area_profile
   ↓
compose response

Return a structured object:

{
  property,
  society,
  area
}
5.6 Error handling

Basic structured errors required.

Examples:

404 Property not found
404 Area not found

Return JSON:

JSON
{
  "error": "property_not_found"
}

Do not panic.

6. Local Development Workflow

After implementation the following should work.

Start backend:

Bash
cd backend
cargo run

Server should start at:

http://localhost:4000

Manual verification:

GET http://localhost:4000/
GET http://localhost:4000/api/properties
GET http://localhost:4000/api/properties/{id}
GET http://localhost:4000/api/areas/{id}
GET http://localhost:4000/api/shortlist

All responses should return valid JSON.

7. Frontend Preparation

We are not building full UI today.

However create a minimal frontend API helper.

frontend/src/lib/api.ts

Functions:

TypeScript
getProperties()
getProperty(id)
getArea(id)
getShortlist()

All should fetch from:

http://localhost:4000/api/*

This prepares the frontend for Day 8.

8. Constraints

Do not implement today:

ranking engine

contextual search parsing

filtering

pagination

database

authentication

shortlist persistence

caching

enrichment pipelines

image processing

scraping

Day 7 must remain focused on:

API scaffolding around seed data.

9. Success Criteria

Day 7 is successful if all of the following work:

cargo run starts the backend successfully

seed dataset loads without errors

/api/properties returns property cards

/api/properties/:id returns joined property + society + area data

/api/areas/:id returns area profile

/api/shortlist returns stub data

responses are clean JSON ready for UI usage

frontend can fetch data from backend successfully

If these conditions are met, OpenEstates has crossed an important milestone:

from static prototype → real application backend.

10. What Comes Next

If Day 7 succeeds, the next steps are clear.

Day 8

Frontend page shells and full localhost flow:

homepage

results page

property detail page

shortlist page

Day 9

Results page with:

real property cards

transparency badges

clean layout

Day 10

Full property detail page with transparency widgets.

Day 7 provides the architectural boundary that makes all of this possible.

11. Product Decisions (what changed and why)
Decision: Backend returns UI-ready responses instead of raw dataset objects

Example:

GET /api/properties

returns property card objects instead of the full property schema.

Why:

UI surfaces should receive product-shaped data

prevents frontend logic duplication

allows backend to evolve schema without breaking UI

keeps the domain model controlled in one place

This aligns with the transparency-first product principle because it ensures every UI surface receives exactly the structured data needed to explain results.

Decision: Introduce backend before building full UI

We intentionally introduced the backend before building real frontend pages.

Why:

stabilizes schema contracts early

prevents frontend-data coupling

ensures transparency widgets have consistent inputs

This reduces large refactors later.

End of Day 7.