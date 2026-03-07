# OpenEstates Prototype

An experimental prototype for a **context-driven real estate matching engine**.

This system is not a full product. It is a simulation environment for testing whether contextual signals — buyer flexibility, seller urgency, negotiation style, emotional constraints — can produce better matches than traditional filter-based search.

## Goal

Replace static filter-based matching (price, location, BHK) with a matching engine that understands:

- buyer preferences and flexibility
- seller urgency and constraints
- signals extracted from conversations
- evolving user context over time

## Status

Experimental. Under active development. Do not use in production.

## Running the App

Install dependencies:

```
pip install -r requirements.txt
```

Launch the terminal interface:

```
python app/main.py
```

## Project Structure

```
openestates/
├── app/                   # TUI entry point
├── engine/                # Matching engine and scoring
├── graph/                 # Context graph modeling
├── agents/                # AI coach and signal extractor
├── simulation/            # Synthetic market and conversation simulation
├── data/                  # Sample market data (JSON)
├── days/                  # Daily build task specifications
├── requirements.txt
└── README.md
```

## Daily Build Log

- **Day 1**: Project skeleton, TUI shell, placeholder modules
- **Day 2**: Synthetic market generator — Property, Seller, Buyer schemas with visible + hidden attributes; seedable; outputs `data/synthetic_market.json`; TUI wired
- **Day 3**: OpenFang integration — signal extractor (live + stub), change narrator, context graph with apply/reinforce/weaken, graph store with file persistence, event logging, chat-first TUI with `/buyer`, `/extract`, `/context` commands
