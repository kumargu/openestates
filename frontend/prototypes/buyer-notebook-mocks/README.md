# Buyer notebook mocks

Interactive journey mocks for the connected **Notebook + Compare** product direction (issue #14).

## Run

```bash
cd frontend/prototypes/buyer-notebook-mocks
npm install
npm run dev
```

Open **http://localhost:5173/**

## Locked rules

1. **Property / Plan** — pin structured UI atoms only (hover notebook icon). No handwritten composers.
2. **RERA-style long text** — select text → floating **Remember** → tagged notebook note + short summary.
3. **Handwritten** — only on the Notebook page (`+ New`).
4. **Compare** — second page; selection from notebook only; rows join on **tags**.
5. **Map basemap pins** — deferred. Future: save from selected marker card (Google Maps / AllTrails), not icons on every marker.

## Connectivity

Discover → Property → Plan → Notebook → Compare share one store. Sidebar homes open Property. Compare home headers open Property. Cross-links on each page.

## Cousins we stole from

- AllTrails — heart on the selected thing
- Notion / Readwise — selection → Remember
- Google Maps — save from place card (map phase later)

## Try

1. Property → hover-pin Schools
2. Open RERA → select complaint text → Remember
3. Plan → pin down-payment gap
4. Notebook → add one handwritten note
5. Select two homes → Compare (tag rows) → click a home name back to Property
