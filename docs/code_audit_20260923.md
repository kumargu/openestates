# Code audit — 23 September 2026

Reviewed `refactor/domain-evidence-contract` at `19d55baa`, followed by the sales-flow removal, cleanup and evidence-admission fix described below. This checkpoint records those follow-up changes for PR #145.

The architecture is substantially simpler than the base branch, but the PR is not ready to call complete. The important remaining work is evidence correctness and migration completion. Adding another parser, evaluator, or generalized framework would make that work harder.

## Scope and method

Scanned the tracked backend, frontend, pipeline and configuration tree (405 files before this cleanup), active HTTP and frontend routes, transaction-related identifiers, runtime state ownership, source materialization, search evaluation, proof handoff, and compatibility paths. Reviewed the PR's source changes separately from generated contracts and captured fixtures. This is a broad architecture audit with focused behavioral verification, not a claim of exhaustive line-by-line or live-browser verification.

Before further cleanup, the full selected baseline passed: 655 Rust library tests and 56 integration contracts; 306 frontend tests; frontend lint/build; 105 Python collector/audit/benchmark tests. Logs and the repeatable diagnostic probes are retained under `/tmp/openestates-code-audit-20260923/`.

## Removed

- Buyer-contact collection and interest-count endpoints, request/response models, file storage, counters, locks, dedicated middleware and configuration.
- Property-page schema.org `Offer` metadata. Source price observations remain discovery evidence.
- The unused `/api/shortlist` endpoint, which returned two hardcoded IDs. The actual frontend shortlist remains in its existing local notebook state.
- Duplicate mutable property, society and search-index copies in `AppState`. The sitemap now reads the same immutable serving snapshot as search and property APIs; startup/reload no longer clone and synchronize those copies.
- An unused discovery-config field and repeated startup messages.
- The superseded `pipeline/smoke_test_api.py`, which still tested retired area endpoints and the placeholder shortlist. The current HTTP journey smoke test remains `tests/api_smoke.py`.

No active bidding, seller-posting, payment or negotiation flow was found. Seller attribution, seller-documentation risk and historical transaction evidence describe external facts, not transaction execution, and remain intact. Added the transaction-flow exclusion to the branch's engineering non-goals.

## Findings and current status

| Priority | Finding | Evidence and smallest direction |
| --- | --- | --- |
| Fixed in this checkpoint | Inventory admission accepted the shape of JSON as its evidence class. | Configured listing fact class, source allowlist, confidence threshold/caps, subject-bound observation and individual-listing validation now gate both selection and inventory capability evidence. Unrelated JSON and ineligible listing facts cannot prove inventory. |
| Fixed in this checkpoint | Identity admission accepted weak and contradictory relationships. | Source/confidence and endpoint admission now precede membership evaluation. Conflicts yield Unknown within the configured relationship scope; OSM boundary and market-locality memberships remain independent. Validation reuses the same evaluator. |
| P1 | Selecting a single listing before the query loses valid alternatives. | `InventoryOption::from_serving_observation` ends with `candidates.into_iter().next()`. A ₹1cr/1,000 sqft listing matches “3BHK under 1.5 crore” alone; adding a ₹3cr/2,000 sqft listing makes that match disappear. Evaluate constraints on each admitted listing, retain its witness, then group results. Do not combine price and size across listings. |
| P1 | The live migration is unfinished. | The corrected historical audit admits 31/149 browse entries as inventory, rejects 52 inconsistent fact rows, and reports an old policy version. Recollect, rebuild, validate and repin. Carpet matching remains deliberately deferred. The PR body's earlier zero-issue/pinned-live claims are stale. |
| P2 | Proof navigation can suppress the usable receipt. | `PropertyPage.tsx` still renders the generic receipt only when `!proofFocus`; a configured destination does not ensure usable map geometry. The earlier reproduction remains applicable. Decide suppression from actual rendered proof availability. |
| P2 | Comparison recovery retains the obsolete snapshot. | `WorkspacePage.tsx` changes `retryKey` but sends the same `snapshotIdentity`. Handle snapshot changes explicitly and recover through the search journey. |
| P2 | Search still has parallel semantic paths. | `search/text.rs` reconstructs preferences with `legacy_display_preference_signal` and matches rendered reason strings. Its confidence function also hardcodes source scores (RERA 1.0, seller 0.6, other 0.5) and weights. Consolidate onto structured evaluations/config under a behavior-preserving benchmark; do not add another wrapper. |
| P2 | Remote validation is failing. | At the reviewed remote head, Backend Check passes; search audit, frontend, materialized-browser and Vercel checks fail. The collector test imports Pydantic without a CI dependency-install step. Frontend jobs stop during `npm ci`; 17 private-registry lockfile URLs remain, although the npm log alone does not establish causality. Vercel's failure cause was not investigated in this audit. |

The first three findings were reproduced after the structural cleanup, before the admission fix. These are controlled counterexamples, not evidence that every corresponding defect occurs in the current live corpus. The first two now have passing permanent regression coverage. Alternative-listing selection remains unfixed; its diagnostic probe describes the defective behavior and stays outside the permanent test suite.

The branch also defers Area Tracker, while the current session's supplied product instructions describe it as first-class. That product-scope conflict remains unresolved by this technical cleanup.

## Is the implementation overdone?

Mostly the opposite: relative to `origin/main`, the reviewed backend source was approximately 4,000 added lines versus 14,000 removed lines. Most frontend additions are generated Rust-to-JSON-schema/TypeScript contracts, not independently maintained implementations. Typed evidence, immutable snapshots and the separation between domain context and UI projection have clear responsibilities and should stay.

The excess was the disconnected stub endpoint and duplicate runtime state, now removed. The more important remaining complexity is overlapping semantics: display-string preference recovery, inconsistent admission rules and a representative-listing model that collapses alternatives too early. Resolve these in their existing owners. Large files alone are not a reason to introduce more layers; several search files include substantial embedded tests.

The production-search hardcoding gate reports zero findings. The broader advisory scan reports 328 possible findings, including source adapters, structural mappings and documentation strings; this is neither 328 confirmed bugs nor a clean repository-wide policy result. The explicit source-score branch above demonstrates why the narrow gate is insufficient on its own.

## Verification and limits

- Baseline: 655 library tests and 56 integration contracts passed, excluding pinned-live cases.
- After removing duplicate state and the stub: all-target Rust compilation, warning-free Clippy and 27 materializer/API contracts passed. The existing fixture now checks retired routes return 404 and the sitemap follows a replaced snapshot. The sitemap test protects the structural contract; it is not a claim of a reproduced production incident.
- Frontend: 306 tests, lint, production build and generated-contract drift check passed. No visible UI was redesigned during this cleanup; browser journeys were not rerun.
- Python: 105 collector/audit/benchmark tests passed. Production-search hardcoding gate passed. Formatting and diff checks passed.
- The separate live-data security test cannot start because this worktree has no promoted dev catalog bundle. Its remaining assertions were retained. Old pinned-live promotion remains blocked by the corrected inventory validation.

Admission checkpoint: 717 selected Rust tests pass (656 library, 61 integration), excluding pinned-live cases; all-target Clippy, three Python audit tests, the production-search hardcoding gate and diff checks pass. A read-only check of the immutable bundle preserves all 107 identity relationships, including the five societies with both boundary and market-locality membership. No real bundle, pointer or wire format changed. See [the migration checkpoint](domain_evidence_migration.md#evidence-admission-checkpoint) for research and verification artifacts.

Remaining sequence: per-listing constraint evaluation and grouping; source recollection and bundle promotion; proof/snapshot recovery; remote CI and PR-claim reconciliation. Keep each semantic change tied to a controlled counterexample and a materializer-to-API contract.
