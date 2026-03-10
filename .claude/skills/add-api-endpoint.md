# Skill: Add a New Rust API Endpoint

## When to use
When you need to add a new API endpoint to the Rust+Axum backend.

## Prerequisites
- Backend compiles: `cd backend && cargo check`
- Understand the data the endpoint will serve (check `data/seed/` and `data/intelligence/`)
- Read the existing routes in `backend/src/routes/` for patterns

## Architecture context

The backend serves structured JSON APIs. Key files:
- `backend/src/main.rs` — Router setup, server bootstrap
- `backend/src/state.rs` — `AppState` struct (holds all in-memory data)
- `backend/src/data_loader.rs` — Loads seed JSON into AppState at startup
- `backend/src/models/` — Serde structs for domain entities
- `backend/src/routes/` — Route handler functions

All handlers receive `State(state): State<Arc<AppState>>` to access data.

## Steps

### 1. Define the response model

Create or extend a model in `backend/src/models/`. Every API response type must derive `Serialize`:

```rust
// In backend/src/models/{entity}.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct MyNewResponse {
    pub id: String,
    pub name: String,
    // ... fields that match what the frontend needs
}
```

If the model also needs to be deserialized from JSON (for data loading), add `Deserialize`.

Register the model in `backend/src/models/mod.rs`:
```rust
pub use {entity}::MyNewResponse;
```

### 2. Add data to AppState (if needed)

If the endpoint needs new data that is not already in AppState:

**a)** Add the field to `backend/src/state.rs`:
```rust
pub struct AppState {
    pub properties: Vec<Property>,
    pub areas: Vec<AreaProfile>,
    pub societies: Vec<Society>,
    pub my_new_data: Vec<MyNewEntity>,  // Add here
}
```

**b)** Load the data in `backend/src/data_loader.rs`:
```rust
pub fn load_seed_data(data_dir: &Path) -> AppState {
    // ... existing loads ...
    let my_new_data: Vec<MyNewEntity> = load_json(data_dir.join("my_new_data.json"));

    AppState {
        properties,
        areas,
        societies,
        my_new_data,
    }
}
```

### 3. Write the route handler

Create a new file `backend/src/routes/{resource}.rs` or add to an existing one:

```rust
use std::sync::Arc;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use crate::state::AppState;

// For query parameters:
#[derive(Deserialize)]
pub struct MyQueryParams {
    pub q: Option<String>,
    pub limit: Option<usize>,
}

// Error response (reuse from properties if possible):
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// GET /api/my-resource — description of what this returns.
pub async fn list_my_resource(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MyQueryParams>,
) -> Json<Vec<MyNewResponse>> {
    let items = state.my_new_data.iter()
        .map(|item| MyNewResponse { /* ... */ })
        .collect();
    Json(items)
}

/// GET /api/my-resource/{id} — returns a single item.
pub async fn get_my_resource(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<MyNewResponse>, (StatusCode, Json<ErrorResponse>)> {
    let item = state.my_new_data.iter()
        .find(|i| i.id == id)
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { error: "not_found".to_string() }),
        ))?;
    Ok(Json(MyNewResponse { /* ... */ }))
}
```

### 4. Register the module

In `backend/src/routes/mod.rs`, add:
```rust
pub mod my_resource;
```

### 5. Register the route

In `backend/src/main.rs`, add the route to the Router:

```rust
let app = Router::new()
    // ... existing routes ...
    .route("/api/my-resource", get(routes::my_resource::list_my_resource))
    .route("/api/my-resource/{id}", get(routes::my_resource::get_my_resource))
    // ...
```

Also add the route to the println block so it shows in the startup log.

### 6. Add the TypeScript type

In `frontend/src/lib/types.ts`, add the response type:

```typescript
export type MyNewResponse = {
  id: string;
  name: string;
  // ... match the Rust Serialize output exactly
};
```

### 7. Add the API function

In `frontend/src/lib/api.ts`, add:

```typescript
import type { MyNewResponse } from "./types.ts";

export function getMyResource(): Promise<MyNewResponse[]> {
  return fetchJson("/api/my-resource");
}

export function getMyResourceById(id: string): Promise<MyNewResponse> {
  return fetchJson(`/api/my-resource/${encodeURIComponent(id)}`);
}
```

### 8. Verify

```bash
# Build backend
cd backend && cargo check

# Run backend
cargo run

# Test endpoint
curl http://localhost:4000/api/my-resource | python3 -m json.tool
curl http://localhost:4000/api/my-resource/some-id | python3 -m json.tool
```

## Patterns to follow

- **List endpoints** return `Json<Vec<T>>` (never error, return empty array if no data)
- **Detail endpoints** return `Result<Json<T>, (StatusCode, Json<ErrorResponse>)>`
- **Use PropertyCard pattern**: list endpoints return lightweight summaries, detail endpoints return full data with joins
- **Join related data in the handler**: look up society by `society_id`, area by `area_id`, etc.
- **Keep handlers simple**: no business logic in routes, just data lookup and mapping

## Checklist

- [ ] Response model defined in `backend/src/models/`
- [ ] Model registered in `backend/src/models/mod.rs`
- [ ] AppState extended if new data source needed
- [ ] data_loader updated if new data source needed
- [ ] Route handler written in `backend/src/routes/`
- [ ] Route module registered in `backend/src/routes/mod.rs`
- [ ] Route registered in `backend/src/main.rs` Router
- [ ] Route printed in startup log
- [ ] TypeScript type added to `frontend/src/lib/types.ts`
- [ ] API function added to `frontend/src/lib/api.ts`
- [ ] `cargo check` passes
- [ ] Endpoint tested with curl
