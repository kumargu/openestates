# Day 36: Lightweight Seller Claim + Sitemap

## Goal
Add "Is this your property? Claim it" demand-capture on property pages + dynamic sitemap for SEO.

## Product Reason
Cheapest possible supply-side signal capture. No auth, no registration — just interest. Sitemap ensures every property/society page is discoverable by Google.

## Deliverables

### 1. Backend: POST /api/claims
- New `backend/src/routes/claims.rs`
- Request: { property_id, name, phone?, email? } → validate → store to `data/claims/{property_id}.json`
- Returns 201 with { status: "claim_received" }
- Wire into main.rs router

### 2. Frontend: Claim section on PropertyPage
- Subtle card at bottom of main column: "Is this your property? Claim it"
- Inline expand (not modal) → form: name, phone, email → submit → success message
- Add `postJson` helper + `submitClaim` to api.ts
- CSS: `.claim-input` styles in index.css

### 3. Backend: GET /api/sitemap.xml
- Generate XML from in-memory properties + societies
- Content-Type: application/xml

### 4. robots.txt
- `frontend/public/robots.txt` with sitemap reference

## Constraints
- No auth, no rate limiting, no modal
- File-based storage in data/claims/
- No new Rust dependencies

## Success Criteria
1. POST /api/claims returns 201, creates file
2. PropertyPage shows claim card, form works
3. GET /api/sitemap.xml returns valid XML with all URLs
4. robots.txt accessible
5. cargo check + npm run build pass
6. Claim form responsive on mobile
