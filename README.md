# OpenEstates v2

A **transparency-first property discovery and matching platform** for Bengaluru real estate.

The product helps buyers find and evaluate properties through context-based search, explainable ranking, and transparent area/society intelligence — not dumb filter forms.

## Product Direction

- Web-first (React + Rust/Axum)
- Context-based search over traditional filters
- Every ranking decision is explainable
- Property pages feel like asset pages, not brochures
- AI is used for intent extraction and explanation — not as the product surface

## Stack

| Layer | Tech |
|---|---|
| Frontend | React |
| Backend API | Rust + Axum |
| Data pipeline | Python |
| Storage (now) | Local JSON files |

## Project Structure

```
frontend/          React web app (Day 8+)
backend/           Rust + Axum API (Day 7+)
engine/            Scoring and ranking logic
pipeline/          Python data collection and enrichment (planned)
agents/            Signal extraction utilities
simulation/        Synthetic market generator and truth model
research/          Reddit market intelligence pipeline
data/
  seed/            Curated seed dataset (properties, areas, societies)
  reddit/          Reddit taxonomy and market signals
  synthetic/       Synthetic market data for engine testing
docs/              Product blueprint and data notes
days/              Daily build specs
```

## Running (Day 6 state)

No web server yet. The seed dataset is ready at `data/seed/`.

To explore the data:
```
cat data/seed/properties.json
cat data/seed/area_profiles.json
cat data/seed/societies.json
```

Python pipeline utilities still run independently:
```
pip install -r requirements.txt
python -m research.reddit_sentiment_researcher  # Reddit market intelligence
```

## Daily Build Log

- **Day 1**: Project skeleton, TUI shell, placeholder modules
- **Day 2**: Synthetic market generator — Property, Seller, Buyer schemas with visible + hidden attributes; seedable; outputs `data/synthetic_market.json`
- **Day 3**: OpenFang integration — signal extractor, change narrator, context graph, graph store, event logging, chat-first TUI
- **Day 4**: Ground truth evaluation + Reddit research — truth compatibility model (`simulation/truth_model.py`) with 12-component scoring, Reddit sentiment researcher producing structured reports and rolling taxonomy
- **Day 5**: v2 reset — deleted TUI/agent/graph/CLI dead code; wrote product blueprint (`docs/openestates_v2_surfaces_and_data.md`) defining 4 core pages, 6 transparency widgets, property + area schema, and data strategy; stack confirmed as Python (pipeline) + Rust/Axum (backend) + React (frontend)
- **Day 6**: Seed dataset — 20 manually curated Bengaluru properties across 5 micro-markets (Whitefield, Sarjapur Road, Bellandur, HSR Layout, North Bengaluru); 5 area profiles with externality signals; 12 society profiles; all fields calibrated to real market ranges
