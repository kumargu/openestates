# 80feet production launch runbook

Status: repository preparation in progress; no Vercel, DNS, plan, domain, or
deployment mutation has been performed.

## Architecture boundary

- Vercel serves only the Vite build: HTML, hashed JS/CSS, and the small
  allowlisted assets emitted by `frontend/vite.config.ts`.
- Browsers call the Mac Mini API and media origin directly over HTTPS.
- Rust, Python, the serving bundle, `data/lake`, property media, and admin work
  stay on the Mac Mini.
- A frontend rollback never changes the serving bundle or lake.
- The customer-facing product name is `80feet`. Repository names, package and
  binary identifiers, `OPENESTATES_*` environment variables, and existing
  `openestates:*` browser-storage keys remain unchanged for compatibility.

## Read-only preflight — 2026-08-25

Vercel team `team_Se2UnBr7giyAl5AXQ4dses1k` is on Hobby.

| Project | ID | State |
|---|---|---|
| `frontend` | `prj_e4iau5k4WLw7CK9rAOSmF1A1UnW0` | Vite, Node 24.x, no Git link, 15 old manual production deployments |
| `openestates` | `prj_3iuFXRsicGlshSut7QbPbWqJM1x6` | Empty, no deployments, no domains |

The latest old `frontend` deployment is READY at commit
`3128ead873fc398645c118bcf7745b60104d5faa`. Retain it as a rollback
candidate until the new domain, API, media, outage, and rollback checks pass.

The promoted serving bundle is not yet launch-ready. On 2026-08-25,
`cargo test --test recommendation_live_audit -- --nocapture` reported 88.4%
society-coordinate coverage against the 90% trust threshold. Enrich and promote
the missing coordinates, then rerun the audit. Do not lower the threshold for
this deployment.

Public registry and DNS checks show that `80feet.app` was registered through
Name.com on 2026-08-25, expires on 2027-08-25, and delegates to
`ns1.vercel-dns.com` and `ns2.vercel-dns.com`. It is not attached to either
Vercel project and HTTPS is not working yet. The owner must confirm that the
domain is in their Name.com account and that auto-renew is enabled.

## Decisions requiring owner approval

1. **Plan:** a commercial/public product beta should use Pro before launch.
   Hobby is acceptable only if the owner records that this is genuinely
   non-commercial validation. Do not upgrade without approval.
2. **Canonical web origin:** choose exactly one. Recommended: serve
   `www.80feet.app` and redirect `80feet.app` once. If apex is selected instead,
   use it consistently for `VITE_SITE_URL`, `OPENESTATES_SITE_URL`, canonical
   tags, CORS, and smoke tests.
3. **Mac ingress:** select a managed outbound tunnel with stable HTTPS, real
   client-IP forwarding, restart support, and bounded logs. Do not expose port
   4000. If `api.80feet.app` cannot be secured without changing DNS providers or
   buying a plan, use the provider's stable HTTPS hostname for the first beta
   and update both `VITE_API_BASE` and the CSP before building.
4. **Preview API:** previews have no API access by default. Approve either a
   separate read-only preview API or a narrow, tested origin allowlist before
   enabling preview data. Never allow arbitrary `*.vercel.app` origins.
5. **External mutation:** Git linking, project rename, environment changes,
   domain attachment, DNS changes, plan changes, deployments, promotions, and
   rollback tests all require explicit approval.

## Repository build contract

Production builds fail unless both public origins are absolute HTTPS origins:

```bash
cd frontend
VITE_API_BASE=https://api.80feet.app \
VITE_SITE_URL=https://www.80feet.app \
npm run build
npm run verify:dist
```

`VITE_*` values are public browser configuration, never secrets. The build
normalizes a trailing slash and rejects credentials, paths, query strings, and
fragments. `/api/*` and `/media/*` payload URLs resolve against the API origin;
frontend-owned assets and absolute/data/blob URLs remain unchanged.

The build allowlists `favicon.svg`, `public/landing`, and `public/story-lab`.
It does not copy `public/societies`, property media, lake data, backend files,
or pipeline files. On 2026-08-25 this reduced `dist` from 51,077,085 bytes to
3,174,605 bytes. Accepted baseline:

- JS: 1,612,557 bytes
- CSS: 347,353 bytes
- largest JS chunk (MapLibre): 1,068,496 bytes
- total output: 3,174,605 bytes

The checked-in budget allows roughly 10–13% growth. Raise it only with measured
PR evidence; do not hide the existing MapLibre chunk by inventing a lower cap.

## Vercel project settings after approval

Reuse `frontend`; do not delete `openestates` yet.

- Optional project rename: `80feet-web`
- Git repository: `kumargu/openestates`
- Production branch: `main`
- Root Directory: `frontend`
- Framework Preset: Vite
- Node: 22.x (also pinned in `frontend/package.json`)
- Install: `npm ci`
- Build: `npm run build`
- Output: `dist`
- Production environment: `VITE_API_BASE`, `VITE_SITE_URL`
- Preview environment: neither value until the preview API policy is approved

Use Vercel's GitHub integration for preview and production deployments. Do not
add a second GitHub Action that calls `vercel deploy`. Require both `Frontend
Build` and Vercel's deployment check in branch protection.

The CSP currently allows only `api.80feet.app` and OpenFreeMap. A provider
hostname fallback must be added explicitly before its build is promoted.

## Mac Mini service contract

Build the release binary, then run it as a non-admin user under launchd. The
launchd job should set only this pointer directly:

```text
OPENESTATES_ENV_FILE=/absolute/private/path/openestates-api.env
```

The referenced file must be access-controlled and contain at least:

```dotenv
OPENESTATES_API_ADDR=127.0.0.1:4000
OPENESTATES_ALLOWED_ORIGINS=https://www.80feet.app
OPENESTATES_SITE_URL=https://www.80feet.app
```

Add existing lake, serving-bundle, admin-auth, and security settings without
copying them into launchd or documentation. The API now defaults to loopback,
and an explicit environment-file path fails startup if it is relative or
unreadable.

The launchd job must use an explicit working directory, `RunAtLoad`,
`KeepAlive` on failure, and separate stdout/stderr paths. Configure macOS
`newsyslog` or the chosen supervisor to rotate and cap both logs. Create the
tunnel's launchd job only after the provider is chosen; configure it to reach
`http://127.0.0.1:4000`, never a LAN/public bind.

Mac launch prerequisites:

- disable system sleep while allowing display sleep;
- enable restart after power failure;
- verify network recovery after reboot;
- trust forwarded client IPs only from the selected local tunnel/proxy;
- keep admin routes unavailable to anonymous browser flows.

## Health, smoke, and cutover

Before domain attachment:

1. `cargo clippy -- -D warnings`, the Rust unit suite,
   `recommendation_live_audit`, and `./tests/smoke_test.sh` pass. The live audit
   is currently blocked by the coordinate-coverage gap recorded above.
2. Frontend lint, tests, build, and `verify:dist` pass on Node 22.
3. API health works through the approved HTTPS ingress.
4. Exact allowed origins receive CORS; an unrelated origin receives no grant.
5. One content-addressed `/media/...` response has the correct MIME type, ETag,
   and `public, max-age=31536000, immutable`.

After an approved preview is READY, verify `/`, a query on `/`, one real
`/property/:id`, `/property/:id/rera`, `/workspace`, and
`/workspace/buy-vs-rent/:id`. Refresh each deep link. Check API and media URLs
in the browser network panel: neither may target localhost or a Vercel
Function.

Stop the Mac API deliberately. The static shell must stay available, API pages
must stop waiting after the bounded timeout, and Retry must recover after the
API returns. Confirm `robots.txt`, sitemap XML, canonical tags, OG metadata,
and JSON-LD contain no `openestates.in` URL.

## Rollback and emergency disable

- Record the Git SHA and Vercel deployment ID after every promotion.
- Roll back by reassigning the production alias to the last verified Vercel
  deployment. Do not rebuild and do not touch the Mac serving bundle.
- Restore by reassigning the alias to the current verified deployment.
- For an API incident, stop/disable the tunnel first; keep the static frontend
  online with its unavailable state.
- Do not delete deployments, projects, domains, or data as incident response.

API compatibility order is additive backend change, frontend promotion, then
old-field removal only after the old frontend is no longer a rollback candidate.

## Cost and availability checks

Expected Vercel usage is static Edge Requests and Fast Data Transfer only:
zero Functions, ISR, Image Optimization, and Storage. Check usage after launch,
after 7 days, after 30 days, and monthly. Track Mac `/media` egress separately.
Alert on frontend and `/api/health` only after multiple consecutive failures so
a Mac/network restart does not create noise.
