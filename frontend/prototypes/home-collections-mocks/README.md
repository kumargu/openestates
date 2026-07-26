# Collections UI mocks

Interactive comparison of **Curated by intent** landing section directions.

## Run

```bash
cd frontend/prototypes/home-collections-mocks
npm install
npm run dev
```

Open **http://127.0.0.1:5174** and use the top pills to switch mocks.

## Directions

| Mock | Reference | Idea |
|------|-----------|------|
| **A · Current** | (production) | Four dense columns — baseline to beat |
| **B · Cosmos clusters** | [Cosmos](https://cosmos.so) | One collection active, warm canvas, asymmetric image grid, minimal chrome |
| **C · Horizontal rail** | Airbnb / Zillow | Swipeable cards, one proof line each |
| **D · Intent list** | Cora / minimal SaaS | Expandable intent rows, no wall of shelves |
| **E · Editorial hero** | Magazine / Cosmos hero | One featured home per shelf, text aside |

## Recommendation (for discussion)

- **Homepage default:** B or E — image-first, one intent at a time
- **Mobile:** C or D — less parsing, thumb-friendly
- **Avoid:** showing all 4–5 shelves at once with 3 text listings each (current)

Pick a winner before implementing in `HomePage.tsx`.
