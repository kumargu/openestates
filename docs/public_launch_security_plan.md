# Public Launch Security Plan

Status: implemented baseline for an anonymous, single-process Rust launch.

## Goal

Keep OpenEstates useful without login while preventing one client from
exhausting the API. Prefer small, maintained Rust crates and configuration over
paid edge products or a custom security framework.

This baseline is intentionally narrow:

- no SEO work;
- no paid Vercel WAF, bot-management, or observability dependency;
- no user accounts or login wall;
- no claim that application middleware can stop a bandwidth-level DDoS attack.

If the frontend remains on Vercel, its automatic plan-level DDoS protection is
a free outer layer. The application must remain safe without optional Vercel
features.

## Residual risks

1. In-process rate, storage, and single-flight state is not shared across
   replicas; launch with one API process.
2. Unknown-route and CORS floods still need transport-level connection limits.
3. Public catalog responses can be copied even though request rates are bounded.
4. Streamed media needs reverse-proxy slow-client protection.

## Rust baseline

Use the existing Axum/Tower stack:

- `tower-governor` 0.8.x for in-process token-bucket rate limiting;
- `tower-http` 0.6.x for CORS, timeouts, and response headers;
- explicit non-queueing admission guards for bounded in-flight work;
- Axum route composition so read, write, and admin endpoints receive different
  policies.

All security code is intentionally grouped for audit and modification:

```text
backend/src/security/
  mod.rs          # small public surface
  admin_auth.rs   # fail-closed admin token verification
  client_ip.rs    # peer IP and trusted-proxy rules
  guards.rs       # request/query rejection guards
  execution.rs    # HTTP/compute/internal execution-lane handles
  media_stream.rs # permits held for the full streamed-body lifetime
  config.rs       # typed, startup-validated TOML policy loader
  retention.rs    # rebuildable cache and bounded log retention
  interest_storage.rs # bounded public-submission storage
  admin_run.rs    # single-flight asset-process reservation
  policy.rs       # every rate, timeout, body, CORS, header, and concurrency rule
```

Operational values live in `app/config/security/runtime.toml`. Rust owns the
generic mechanics and validates the complete file at startup; tuning does not
require adding policy constants or branches to handlers.

Do not trust `X-Forwarded-For` by default. Use the socket peer address unless
the socket peer matches an exact IP in `OPENESTATES_TRUSTED_PROXY_IPS`. The
proxy must overwrite the header or append the address it observed as the final
value, and the Rust origin must reject direct public traffic. Otherwise an
attacker can forge a new address on every request and bypass limits. CIDR-based
proxy fleets need a deliberate follow-up rather than a broad trust switch.

## Initial policies

These values are conservative starting points and should be adjusted after a
local load test:

| Route class | Burst | Refill | Other bounds |
| --- | ---: | ---: | --- |
| Public reads | 120 requests | 2 requests/second/client | 3 second timeout, 16 concurrent |
| Search | 16 requests | 1 request/second/client | 3 second timeout, 8 admitted, 2 CPU jobs, 1.5 second bounded compute wait |
| Batch POST reads | 30 requests | 1 request/second/client | 64 KiB body |
| Interest submission | 5 requests | 1 request/30 seconds/client | 16 KiB body, 1 KiB records, 32 MiB total storage |
| Admin | 20 requests | 1 request/second/client | 64 KiB body, explicit token, one asset run |

Apply a 64-request global admission ceiling as a final backstop. Full-catalog
builds have a separate 2-request lane and are serialized once per bundle
version. Benchmark the intended machine before changing these TOML values.

The API process owns three execution lanes: customer HTTP coordination,
bounded customer CPU work, and internal/background work. Search ranking and
catalog construction never run on HTTP coordination workers. Limiter cleanup,
search-event handling, reload hydration, and child-process management run on
the internal runtime.

## Admin fail-closed rule

Production must never invent an admin credential. When `ADMIN_TOKEN` is
missing, admin requests return unavailable/unauthorized and no process-spawning
operation runs. Local development may use an explicit token in `.env.local`,
but there is no source-code fallback.

Admin routes should eventually move to a separate listener or private network.
That is not required for the first Rust baseline, but a header token should not
be treated as the final perimeter.

## Scraping expectations

Public pages and public JSON can always be copied by a determined client. The
baseline makes bulk extraction slower and protects availability:

- rate-limit by client;
- cache repeated reads;
- return only product API views, never lake files or internal indexes;
- cap batch request sizes and item counts;
- keep media content-addressed and cacheable.

The current `GET /api/properties` response still exposes the full public
inventory in one request. Rate limiting protects availability but does not make
that inventory hard to copy. Pagination or response caps are a later product
decision if full-catalog extraction becomes a real problem.

Media streams hold a dedicated permit until their body is dropped. Connection,
header, and streaming idle timeouts still belong at the transport/hosting
layer.

Do not block broad user-agent strings such as `bot`, `curl`, or `python`. They
are easy to forge and cause false positives. Add a block only after observed
abuse identifies a narrow, testable signal.

## Verification

- Normal API and buyer smoke tests still pass.
- Missing `ADMIN_TOKEN` cannot authorize any admin endpoint.
- Repeated requests receive 429 and `Retry-After` without crashing the server.
- Oversized POST bodies receive 413.
- Excess route work receives 503 instead of queueing indefinitely. Search CPU
  waits only inside its configured bounded window.
- Admin reload hydrates on the internal runtime and publishes its immutable
  snapshot last while compatibility writers are held.
- The limiter periodically evicts stale client keys. Unique active-client
  cardinality remains bounded only by incoming traffic and needs load testing.
- `cargo check`, focused security/search contracts, and the 50-endpoint smoke
  suite pass.
- A local eight-query cache-miss burst keeps health below 1 ms; all eight
  searches drain through two compute slots in roughly 250 ms on the test host.

## Deployment checklist

1. Set `OPENESTATES_ALLOWED_ORIGINS` to the comma-separated production frontend
   origins whenever the browser calls a different API origin. The safe default
   allows local Vite only.
2. Set a non-empty `ADMIN_TOKEN` only when production admin operations are
   required; leaving it unset deliberately disables every admin request.
3. Leave `OPENESTATES_TRUSTED_PROXY_IPS` empty unless the origin is private, the
   listed proxy peer IPs are stable, and that proxy strips or overwrites inbound
   forwarding headers.

## Later, only when metrics justify it

- Put rate counters in Redis or another shared store when multiple Rust
  replicas make per-process limits ineffective.
- Add an anonymous signed session identifier if carrier-grade NAT causes IP
  limits to affect unrelated buyers.
- Add a managed challenge only to the abused endpoint.
- Add structured request/429/latency logging before production traffic; keep
  secrets and buyer contact values out of logs.
- Pay for edge WAF/bot tooling only when attack volume or operational cost is
  greater than the service cost.

## Definition of done

- Anonymous browsing remains intact.
- Admin access fails closed.
- Public reads, batch reads, and writes have separate Rust limits.
- Public request bodies, duration, and concurrent work are bounded.
- Stale in-process rate-limit keys are periodically evicted.
- Optional hosting protection is helpful but not required for application
  correctness.
