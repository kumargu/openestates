# Day 5 Cleanup Note

**Date:** Day 5
**Reason:** OpenEstates v2 reset — product pivoted from terminal-first AI prototype to web-first transparency platform.

---

## What Was Deleted

| Path | Why |
|---|---|
| `app/` (main.py, tui.py) | Textual TUI — dead surface in v2 |
| `agents/coach.py` | AI coach was the v1 product surface, not v2 |
| `agents/change_narrator.py` | TUI-specific narrative output, no v2 equivalent |
| `agents/openfang_client.py` | OpenFang integration not needed for v2 pipeline |
| `graph/context_graph.py` | Built for TUI session state; v2 uses structured context objects instead |
| `graph/graph_store.py` | File persistence for TUI context graph — same reason |
| `simulation/conversation_simulator.py` | Simulated TUI conversations, not applicable to web product |
| `src/` (click CLI + egg-info) | Installed CLI entry point — v2 has a React frontend |
| `data/contexts/` | Buyer context JSON files from TUI sessions |
| `data/events.jsonl` | TUI event log |

---

## What Was Kept and Why

| Path | Why |
|---|---|
| `agents/schemas.py` | SignalUpdate schema (value, confidence, weight, provenance) — reusable in Python data pipeline |
| `agents/signal_extractor.py` | Signal extraction logic — reusable for AI-assisted enrichment in pipeline |
| `simulation/market_generator.py` | Generates synthetic buyer/property sets — useful for engine testing |
| `simulation/truth_model.py` | 12-component compatibility scoring model — will evolve into Rust ranking engine |
| `engine/scoring.py` | Scoring function stubs — reference for ranking architecture |
| `engine/match_engine.py` | Match engine stub — reference |
| `research/reddit_client.py` | Reddit API client — active use in market intelligence pipeline |
| `research/reddit_sentiment_researcher.py` | Area signal extraction from Reddit — directly useful |
| `data/reddit/taxonomy.json` | Collected decision drivers, area prices, phrase counts — real signal data |
| `data/reddit_cache/` | Cached Reddit API responses |
| `data/synthetic_market*.json` | Synthetic property data for engine testing |
| `data/truth_model_weights.json` | Evidence-based scoring weights |

---

## What Replaces the Deleted Work

| Deleted | Replaced by |
|---|---|
| TUI (`app/`) | React web frontend (`frontend/`) — Day 8+ |
| Click CLI (`src/`) | Rust + Axum backend (`backend/`) — Day 7+ |
| Context graph (`graph/`) | Structured user context objects served by backend |
| OpenFang client | Direct Claude API calls in Python pipeline, or stub |
| TUI event log | Structured interaction events from web frontend (later) |

---

## Net Result

The codebase is now aligned with v2. No legacy TUI assumptions remain in active code paths. The Python modules that survive (`agents/`, `simulation/`, `research/`, `engine/`) will serve the data pipeline and ranking engine, not a terminal interface.
