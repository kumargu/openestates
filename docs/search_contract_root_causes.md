# Why the search/data/UI contracts drifted

Root-cause investigation, 21 September 2026. Scope is unchanged from the [verified audit](search_data_ui_audit.md): main at `2f0f4e27` and the separately identified local promoted bundle. This investigation changes documentation only.

## Diagnosis

**The system has strong components but an incomplete shared semantic contract.** Several boundaries check that a record has the right shape or an identity is well formed, without checking that the next layer can consume the same meaning and evidence. Earlier display-oriented representations remain authoritative inputs to newer proof-oriented code.

The result is predictable: data can be valid enough for promotion and a card, insufficient for constrained search, sufficient for a search reason, and insufficient again for detail proof. Those are currently separate decisions rather than projections of one validated decision.

This diagnosis is supported by the failures already reproduced, constructors and validators, consumer implementations, test fixtures, and git history. It does not assume why an individual change was authored.

## 1. Meaning is lost before the API boundary

The source listing carries `area_sqft` **and** `area_type`. `MarketPricing` preserves only square footage and price information. The property projection copies that number into both `carpet_area_sqft` and `super_builtup_sqft`. Search then treats the former as a domain measurement.

Relevant code:

- [Property](../backend/src/models/property.rs#L7): plain scalar price/area fields, with no measurement basis or evidence handle attached to them.
- [MarketPricing](../backend/src/data_loader.rs#L1364) and [parse_listing_pricing](../backend/src/data_loader.rs#L1484): the intermediate projection drops area basis.
- [runtime_numeric_constraint_evidence](../backend/src/search/text.rs#L1473): a projected scalar becomes eligibility evidence with confidence 1.0.
- [validate_property_projection](../backend/src/serving/validator.rs#L684): requires a positive value in `carpet_area_sqft`, described as “positive size data,” without validating that it is carpet area.

The validator uses the same property projection that lost the distinction. It therefore checks the output's completeness without an independent assertion that its semantics match the input.

**Missing contract:** a measurement retains its dimension, basis, unit, scope, and evidence identity through every conversion. Generic listed area cannot silently become carpet area. A UI projection cannot be promoted back into proof simply because it has a nonzero number.

**Primary owner:** the materialized domain/serving model. Fixing labels or filtering individual homes in React would leave the wrong meaning in search and other consumers.

## 2. “Present,” “displayable,” “searchable,” and “provable” have different admission rules

These are legitimate distinct states, but their relationship is not enforced:

| Boundary | What it currently permits/checks |
|---|---|
| Gold → serving fact | Completely absent observation provenance is accepted as `None`; partial provenance is rejected |
| Fact observation validation | Missing observation returns success; existing observation must be internally valid |
| Promotion | Fact/entity/metadata relationships, artifact integrity, configured property requirements, and projected card completeness |
| Capability index | Presence of fact keys or preference labels, including prefix matches |
| Inventory evaluator | Requires a usable observation binding and matching inventory values |

Sources: [builder.rs:1035](../backend/src/serving/builder.rs#L1035), [types.rs:70](../backend/src/serving/types.rs#L70), [eligibility.rs:33](../backend/src/serving/eligibility.rs#L33), [capabilities.rs:18](../backend/src/search/capabilities.rs#L18), [evaluation.rs:25](../backend/src/search/evaluation.rs#L25).

This explains the 16 displayed configurations without inventory bindings. Search is correctly refusing to prove their BHK constraints; the rest of the system has already presented them as normal typed inventory.

Metadata coverage is useful but does not establish usable evidence. `SearchCapabilityIndex` is a presence index; it does not validate an eligible evidence policy, evaluator, and proof projection as one complete capability.

**Missing contract:** every advertised buyer capability has a declared evidence requirement, an evaluator that consumes it, and an explicit proof/presentation outcome. A specific advertised inventory value must have the evidence required by its corresponding constraint. Incomplete societies may still be browsable, but that must be a deliberate state rather than an accidental disagreement.

**Primary owner:** serving validation and capability binding. The correct response is not to weaken the inventory evaluator or fabricate provenance for legacy rows.

## 3. Predicate truth and evidence are not inseparable

The code has a stronger type, `PredicateEvaluation::Satisfied(VerifiedMatch)`, but also a weaker public representation:

```text
BooleanEvaluation {
  state: Satisfied,
  verified_matches: []
}
```

That representation is legal and used by numeric constraints. `match_hard_constraints()` produces evidence, but the caller throws it away and retains only success. Journey projection later binds verified matches only for BHK, budget, and spatial terms.

Sources: [evaluation.rs:289](../backend/src/search/evaluation.rs#L289), [text.rs:3046](../backend/src/search/text.rs#L3046), [journey.rs:837](../backend/src/search/journey.rs#L837).

There is also a lifetime mismatch for category proximity. Evaluation creates an in-memory derived witness. Token issuance accepts that witness's ID. Resolution later has only the immutable bundle and token, and cannot recreate the targetless derivation. The signing operation preserves identity; it does not establish that the identified evidence is retrievable.

Sources: [geo.rs:1035](../backend/src/search/geo.rs#L1035), [journey.rs:868](../backend/src/search/journey.rs#L868), [proof.rs:182](../backend/src/search/proof.rs#L182), [proof.rs:344](../backend/src/search/proof.rs#L344).

**Missing contracts:**

1. A satisfied buyer predicate retains an eligible witness tied to its predicate occurrence and typed constraint. Ranking, explanation, and proof consume that same evaluation.
2. For an unchanged snapshot, every issued proof reference resolves to the same witness without relying on discarded request-local state.
3. Negative predicates retain the evidence establishing the negative decision. This must be designed at the leaf-evaluation level; blindly requiring nonempty evidence on every Boolean node would mishandle empty structural conjunctions and is not a sound design.

**Primary owner:** the evaluator and proof model. Having one compiled plan is a useful foundation, but it is insufficient when execution returns evidence through incompatible paths.

## 4. The API conflates evidence validity, navigation capability, and card visibility

`SearchMatchReason` carries `showOnCard` and a token. It does not tell the client whether that token has a supported detail destination. The resolve response has an optional destination. The frontend picks a token, then treats no destination as unavailable evidence.

These are independent concepts:

- Is the claim proved?
- Should its label appear on the card?
- Can detail focus a specific surface for it?
- Is the original snapshot/evidence still available?

The UI has a real state-handling bug: a successfully resolved receipt with no destination becomes “no longer available.” The API allows that ambiguity to reach the UI. Neither a successful HTTP response nor a TypeScript interface establishes the missing relationship.

Sources: [journey.rs:112](../backend/src/search/journey.rs#L112), [proof.rs:60](../backend/src/search/proof.rs#L60), [types.ts:1318](../frontend/src/lib/types.ts#L1318), [proof-focus.ts:3](../frontend/src/lib/proof-focus.ts#L3), [PropertyPage.tsx:195](../frontend/src/pages/PropertyPage.tsx#L195).

The frontend fetch helpers trust JSON as the requested generic type. The journey projection does check its version and retained-results identity; this is a useful existing guard. Other reduced types, including proof resolution, do not express the full set of mutually exclusive states.

**Missing contract:** explicit, distinguishable outcomes for proved-and-focusable, proved-without-focus, stale, unavailable, and invalid/mismatched requests. Visible-copy priority must not decide navigation capability. Backend and frontend must agree on the transitions, not merely field names.

**Primary owners:** API contract and UI state handling. Generated types or runtime decoding can enforce the shape of this contract, but cannot repair incorrect measurement semantics upstream.

## 5. Presentation can create a second kind of “proof”

`SourceObservation` and `DerivedEvidence` have identity and validation machinery. `SceneReceipt` is separately constructible from claim text, source label, timestamp, and confidence; it has no required reference to either evidence type.

The seed-map fallback can therefore create a syntactically valid `SceneReceipt` while bypassing the evidence index and promoted bundle. Its records look like normal buyer evidence. The UI is largely rendering what the backend declared; this is not principally a map-component bug.

Sources: [SceneReceipt](../backend/src/surfaces.rs#L202), [merge_surface_context_polygons](../backend/src/surfaces.rs#L527), [routes/surfaces.rs:222](../backend/src/routes/surfaces.rs#L222).

**Missing contract:** every buyer-fact receipt is a projection of eligible durable evidence. A basemap decoration may have visual/source metadata, but must not become an asserted property relationship or proof receipt through a presentation fallback. Scene features and their receipts must remain attributable to the declared bundle.

**Primary owner:** backend presentation boundary. This does not require removing maps or OSM visuals; it requires separating visual context from asserted evidence.

## 6. Migrations and tests close individual paths, not all consumers

Two verified consumer migrations are incomplete:

- Search events moved to a lake-writing worker, but Area Tracker still reads the old in-memory graph log.
- The journey envelope and signed proof transport replaced the public search contract, but the Python benchmark still reads top-level results and client-authored proof focus.

The benchmark's 18 self-tests passed during this investigation, while the previously captured 32-result real response is interpreted as zero results. Its tests model the retired response shape.

The browser fixture supplies a successful proof with a destination. A separate unit test deliberately requires hidden budget reasons to retain a navigation token. Each assertion passes locally, but no required check joins that navigation decision to a real destination-less inventory proof.

Sources: [benchmark tests](../tests/test_search_quality_benchmark.py#L90), [browser mock](../frontend/e2e/search/journey.spec.ts#L37), [proof navigation test](../frontend/tests/listing-price-and-proof-focus.test.ts#L98).

Controlled search fixtures directly construct `LoadedServingBundle` and add well-formed observations. These are valuable for proving runtime semantics independently of data coverage. They cannot establish that current materializers and existing promoted records supply equivalent evidence.

There **is** a materializer-to-API test. It asserts source fields and a simple BHK search result count, but does not establish area-basis correctness or open every generated proof. The problem is therefore not “no integration tests.” Their assertions do not cover the cross-layer guarantees at issue.

Sources: [controlled fixture construction](../backend/tests/search_conversational_semantics_contract.rs#L3392), [fixture observation](../backend/tests/search_conversational_semantics_contract.rs#L4009), [vertical contract](../backend/tests/project_enrichment_vertical_contract.rs#L38).

The checked-in [CI workflow](../.github/workflows/ci.yml) runs three named Rust search contracts and fixture browser journeys. It does not run `project_enrichment_vertical_contract`; the pinned Rust live test is ignored by default, and live browser cases skip without `SEARCH_LIVE_API`. This describes the checked-in workflow, not any uninspected external branch-protection configuration.

**Missing contract:** an API/data-contract migration is complete only when all active producers, consumers, and validating runners consume the new contract, and the superseded paths are removed. A required vertical gate must exercise real serialization and materialization for the supported predicate/evidence families.

## How this accumulated

Git history supports a progressive, incomplete tightening of an older model:

| Commit/date | Observed architectural change |
|---|---|
| `418f523d`, 26 July | Existing generic-size → carpet-field projection is already present |
| `3e15f5a3`, 31 July | Search runtime snapshot/log-worker work introduced |
| `820c6b95`, 6 September | Serving observation identity preserved, but absence remains allowed |
| `0461a73c`, 6 September | Inventory predicates fail closed; numeric success still drops witnesses |
| `6aa772a2`, 13 September | Seed lake footprints gain scene relations/receipt records |
| `18603071`, 21 September | Journey API and UI migrate; benchmark consumer stays on old shape |
| `f41e7475`, 21 September | Exact receipt handoff tightened for the tested paths |

The core pattern is **partial migration of semantic authority**: new evidence rules were added, while older constructors, projections, and consumers retained permission to produce or interpret buyer meaning. Missing values and fallbacks allowed both approaches to continue compiling and serving.

## Contracts to settle before implementation

| Required invariant | Authoritative boundary | Proof of completion |
|---|---|---|
| Measurement meaning survives projection | Materializer + typed serving view | Distinct carpet/built-up/mixed inputs remain distinct through search and detail |
| Advertised inventory and its searchable predicates share evidence | Promotion + inventory binding | Every advertised typed value either has its eligible witness or an explicit incomplete state |
| A predicate decision retains its witness | Evaluator | Numeric, inventory, identity, spatial, and negative decisions preserve predicate IDs, constraints, and evidence |
| Issuance implies resolvability on the same snapshot | Proof service | Every issued reference resolves; request-local derivations cannot escape without a resolution strategy |
| Proof validity is independent of UI focus | API + UI state model | Valid receipts without a destination never become expired/unavailable states |
| Presentation cannot invent buyer evidence | Scene/detail projection | Every buyer-fact receipt traces to the declared bundle; decoration cannot manufacture receipts |
| Capability means a complete supported path | Config/bundle binding | Eligible evidence policy, evaluator, proof projection, and explicit presentation outcome are bound together |
| Contract migration covers all consumers | CI/release validation | Materializer, API, benchmark, browser, and event consumers use the same contract version |

These are contract-level design decisions, not a proposal to introduce another parser, evaluator, generic framework, or registry. Existing immutable snapshots, compiled plans, evidence identities, frozen query bank, and backend-owned ordering should be retained.

Before a remediation implementation, the useful design artifact is one agreed domain/evaluation/proof contract and a small matrix of existing vertical scenarios that proves these invariants. Adding eight isolated patches would leave the same architectural permissions in place.

## Limits and work performed

- Reused the prior current-bundle reproductions and passing baseline; did not rebuild or promote data.
- Read types, constructors, validators, evaluator/proof paths, frontend consumers, fixtures, CI, and relevant commit history.
- Ran the benchmark's 18 self-tests to verify the validation blind spot; all passed. Log: [/tmp/openestates-audit-20260921/benchmark-self-tests.log](/tmp/openestates-audit-20260921/benchmark-self-tests.log).
- No production code, configuration, tests, expectations, or source data changed. No UI patch or API migration attempted.
