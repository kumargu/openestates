import assert from "node:assert/strict";
import test from "node:test";
import {
  nextStoryFrameIndex,
  primaryStoryFactKeys,
  projectPropertyStory,
  selectStoryMotionTheme,
  shouldAutoAdvanceStory,
  stableStoryHash,
  wrappedFilmstripOffset,
  type StoryMotionTheme,
} from "../src/lib/propertyStory.ts";
import {
  storyLabDetailFixture,
  storyLabMediaFixture,
} from "../src/lib/propertyStoryFixtures.ts";

function richDetail() {
  return storyLabDetailFixture({
    propertyId: "fixture-prestige-lakeside-3bhk",
    coverage: "rich",
    lifecycle: "under-construction",
    reviews: "present",
    rera: "complete",
  });
}

test("property story projection is deterministic", () => {
  const detail = richDetail();
  const media = storyLabMediaFixture({ count: "many", provenance: "mixed" });
  assert.deepEqual(
    projectPropertyStory(detail, { media }),
    projectPropertyStory(detail, { media }),
  );
});

test("Story Lab projection matrix stays deterministic and bounded", () => {
  const propertyIds = [
    "fixture-prestige-lakeside-3bhk",
    "fixture-sobha-royal-pavilion-4bhk",
    "fixture-vaswani-starlight-3bhk",
  ] as const;
  const coverages = ["rich", "partial", "sparse"] as const;
  const lifecycles = ["ready", "under-construction"] as const;
  const reviewStates = ["present", "unresolved", "empty"] as const;
  const reraStates = ["complete", "partial", "missing"] as const;
  const mediaCases = [
    { count: "none", provenance: "current" },
    { count: "single", provenance: "current" },
    { count: "many", provenance: "current" },
    { count: "many", provenance: "mixed" },
    { count: "many", provenance: "render" },
  ] as const;
  let projections = 0;

  for (const propertyId of propertyIds) {
    for (const coverage of coverages) {
      for (const lifecycle of lifecycles) {
        for (const reviews of reviewStates) {
          for (const rera of reraStates) {
            const detail = storyLabDetailFixture({
              propertyId,
              coverage,
              lifecycle,
              reviews,
              rera,
            });
            for (const mediaCase of mediaCases) {
              const media = storyLabMediaFixture(mediaCase);
              const first = projectPropertyStory(detail, { media });
              const second = projectPropertyStory(detail, { media });
              const factKeys = primaryStoryFactKeys(first);

              assert.deepEqual(first, second);
              assert.equal(new Set(factKeys).size, factKeys.length);
              assert.ok(first.media.frames.length <= 7);
              assert.ok(first.arrival.frames.length <= 6);
              assert.ok(first.coverage.availableDecks <= first.coverage.totalDecks);
              if (mediaCase.count === "none") {
                assert.equal(first.media.frames.length, 0);
              }
              projections += 1;
            }
          }
        }
      }
    }
  }

  assert.equal(projections, 810);
});

test("sparse properties compact unavailable decks", () => {
  const detail = storyLabDetailFixture({
    propertyId: "fixture-vaswani-starlight-3bhk",
    coverage: "sparse",
    lifecycle: "ready",
    reviews: "empty",
    rera: "missing",
  });
  const story = projectPropertyStory(detail, { media: [] });
  assert.equal(story.coverage.level, "sparse");
  assert.deepEqual(
    story.decks.map((deck) => deck.kind),
    ["hero"],
  );
  assert.equal(story.media.frames.length, 0);
});

test("primary buyer facts belong to one deck", () => {
  const story = projectPropertyStory(richDetail(), {
    media: storyLabMediaFixture({ count: "many", provenance: "mixed" }),
  });
  assert.equal(story.identity.title, "Prestige Lakeside Habitat");
  assert.equal(story.identity.facts[1]?.value, "3 BHK");
  const factKeys = primaryStoryFactKeys(story);
  assert.equal(new Set(factKeys).size, factKeys.length);
});

test("identity preserves exact crore prices and area measurement type", () => {
  const detail = richDetail();
  detail.property.price = 125_000_000;
  detail.property.carpet_area_sqft = 1_543;
  const story = projectPropertyStory(detail);
  assert.equal(
    story.identity.facts.find((fact) => fact.key === "price")?.value,
    "₹12.5 Cr",
  );
  assert.equal(
    story.identity.facts.find((fact) => fact.key === "size")?.value,
    "1,543 sqft carpet",
  );
});

test("listing identity is retained when stripping BHK would duplicate locality", () => {
  const detail = richDetail();
  detail.society = null;
  detail.property.title = "3 BHK in Whitefield";
  detail.property.area = "Whitefield";
  detail.property.city = "Bengaluru";
  const story = projectPropertyStory(detail);
  assert.equal(story.identity.title, "3 BHK in Whitefield");
  assert.equal(story.identity.location, "Whitefield, Bengaluru");
});

test("media provenance survives projection without inferring unknown images", () => {
  const detail = richDetail();
  detail.property.hero_image = "/media/unlabelled.webp";
  const unlabelled = projectPropertyStory(detail);
  assert.equal(unlabelled.media.frames[0]?.lifecycle, "unknown");
  assert.equal(unlabelled.media.frames[0]?.sourceType, "unknown");

  const mixed = projectPropertyStory(detail, {
    media: storyLabMediaFixture({ count: "many", provenance: "mixed" }),
  });
  assert.deepEqual(
    mixed.media.frames.map((frame) => frame.lifecycle),
    ["current", "current", "proposed", "proposed"],
  );
});

test("arrival projection keeps only bounded approach-road evidence", () => {
  const detail = richDetail();
  const approach = detail.evidence?.sections
    .find((section) => section.kind === "approach_road");
  assert.ok(approach?.media?.[0]);
  const strip = approach.media[0];
  strip.frames[0].source_url = "javascript:alert(1)";
  strip.frames.push(
    ...[5, 6, 7].map((index) => ({
      ...strip.frames[0],
      label: `Approach ${index}`,
      image_url: `/story-lab/approach-${index}.webp`,
      source_url: `https://www.google.com/maps/frame/${index}`,
    })),
  );
  detail.evidence?.sections.push({
    ...approach,
    kind: "building_gallery",
    media: [{
      ...strip,
      frames: [{
        ...strip.frames[0],
        label: "Unrelated gallery frame",
        image_url: "/story-lab/unrelated.webp",
      }],
    }],
  });

  const story = projectPropertyStory(detail);
  assert.equal(story.arrival.frames.length, 6);
  assert.equal(story.arrival.frames[0]?.label, "Main road");
  assert.equal(story.arrival.frames[0]?.distanceFromGateM, 180);
  assert.equal(story.arrival.frames[0]?.lifecycle, "current");
  assert.equal(story.arrival.frames[0]?.sourceType, "Google Street View");
  assert.equal(story.arrival.frames[0]?.sourceUrl, undefined);
  assert.equal(
    story.arrival.frames.some((frame) => frame.label === "Unrelated gallery frame"),
    false,
  );
  assert.deepEqual(
    story.decks.slice(0, 3).map((deck) => deck.kind),
    ["hero", "map", "arrival"],
  );
});

test("official record teaser projects only known registration and document facts", () => {
  const detail = richDetail();
  detail.rera_report_ref = {
    registration_ids: ["internal:rera-record"],
    href: `/property/${detail.property.id}/rera`,
    availability: "available",
  };
  detail.decision_check_summary = {
    tileLabel: "RERA",
    tone: "positive",
    registrationNumberCompact: "PRM/KA/.../004371",
    primaryCount: 2,
    totalCount: 2,
    primaryLabels: [
      {
        key: "parking_per_home_available",
        label: "1 parking/home",
        severity: "positive",
        scope: "project",
        visualId: "visit",
        valueText: "1",
        priority: 48,
        confidence: 1,
        groupId: "project_facts",
        placement: "more",
      },
    ],
    groups: [{
      id: "documents",
      title: "Documents",
      labels: [{
        key: "sanction_plan_available",
        label: "Sanction plan available",
        severity: "positive",
        scope: "project",
        visualId: "layout",
        valueText: "1",
        priority: 28,
        confidence: 1,
        groupId: "documents",
        placement: "audit",
      }],
    }],
  };
  const cards = projectPropertyStory(detail).recordCards;
  assert.equal(cards.length, 2);
  assert.equal(cards[0]?.availability, "available");
  assert.deepEqual(cards[0]?.facts, [
    {
      key: "registration",
      label: "Number",
      value: "PRM/KA/.../004371",
    },
  ]);
  assert.deepEqual(cards[1]?.facts, [
    {
      key: "sanction_plan_available",
      label: "Sanction plan available",
      value: "1",
    },
  ]);
  assert.equal(
    cards.flatMap((card) => card.facts).some(
      (fact) => fact.label === "1 parking/home",
    ),
    false,
  );
});

test("partial official records and unresolved reviews stay compact without invention", () => {
  const detail = richDetail();
  detail.rera_report_ref = {
    registration_ids: [],
    href: `/property/${detail.property.id}/rera`,
    availability: "partial",
  };
  delete detail.decision_check_summary;
  detail.external_reviews = {
    google_reviews_url: "https://www.google.com/maps",
  };
  const story = projectPropertyStory(detail);
  assert.deepEqual(story.recordCards, []);
  assert.equal(story.reviews.state, "unresolved");
  assert.equal(
    story.decks.some((deck) => deck.kind === "record"),
    false,
  );
  assert.equal(
    story.decks.some((deck) => deck.kind === "reviews"),
    true,
  );
});

test("short compare projects current home plus two distinct peers and handoff", () => {
  const detail = richDetail();
  const base = detail.similar_properties[0];
  assert.ok(base);
  const peerA = {
    ...base,
    id: "peer-a",
    society_name: "Peer A",
    kg_entity_refs: undefined,
  };
  const peerB = {
    ...base,
    id: "peer-b",
    society_name: "Peer B",
    kg_entity_refs: undefined,
  };
  const peerC = {
    ...base,
    id: "peer-c",
    society_name: "Peer C",
    kg_entity_refs: undefined,
  };
  detail.similar_properties = [peerA];
  const story = projectPropertyStory(detail, {
    comparisonProperties: [peerB, peerC],
  });
  assert.deepEqual(
    story.comparisons.map((home) => home.id),
    [detail.property.id, "peer-b", "peer-c"],
  );
  assert.deepEqual(
    story.comparisons.map((home) => home.isCurrent),
    [true, false, false],
  );
  assert.equal(
    story.compareHref,
    `/workspace/compare?ids=${encodeURIComponent(
      `${detail.property.id},peer-b,peer-c`,
    )}&focus=${encodeURIComponent(detail.property.id)}`,
  );

  detail.similar_properties = [peerA];
  const sparseCompare = projectPropertyStory(detail);
  assert.deepEqual(sparseCompare.comparisons, []);
  assert.equal(
    sparseCompare.decks.some((deck) => deck.kind === "compare"),
    false,
  );
});

test("motion selection is stable and cannot change facts or deck order", () => {
  const detail = richDetail();
  const media = storyLabMediaFixture({ count: "many", provenance: "current" });
  const automatic = projectPropertyStory(detail, { media });
  const themes: StoryMotionTheme[] = [
    "quiet-pan",
    "architectural-drift",
    "slow-push",
    "editorial-cut",
    "still",
  ];

  assert.equal(
    stableStoryHash(detail.property.id),
    stableStoryHash(detail.property.id),
  );
  for (const motionTheme of themes) {
    const themed = projectPropertyStory(detail, { media, motionTheme });
    assert.deepEqual(themed.identity, automatic.identity);
    assert.deepEqual(
      themed.decks.map((deck) => deck.kind),
      automatic.decks.map((deck) => deck.kind),
    );
  }
});

test("production gallery projection deterministically demonstrates distinct themes", () => {
  const propertyIds = [
    "fixture-prestige-lakeside-3bhk",
    "fixture-sobha-royal-pavilion-4bhk",
    "fixture-vaswani-starlight-3bhk",
  ] as const;
  const themes = propertyIds.map((propertyId) => {
    const detail = storyLabDetailFixture({
      propertyId,
      coverage: "rich",
      lifecycle: "ready",
      reviews: "present",
      rera: "complete",
    });
    detail.property.hero_image = `/media/${propertyId}/hero.webp`;
    detail.property.images = Array.from(
      { length: 6 },
      (_, index) => `/media/${propertyId}/gallery-${index + 1}.webp`,
    );
    return projectPropertyStory(detail).motionTheme;
  });

  assert.deepEqual(themes, [
    "slow-push",
    "architectural-drift",
    "editorial-cut",
  ]);
  assert.equal(new Set(themes).size, propertyIds.length);
});

test("single-image and reduced-motion scenes never auto-advance", () => {
  const singleFrame = storyLabMediaFixture({
    count: "single",
    provenance: "current",
  });
  assert.equal(
    selectStoryMotionTheme({
      frames: singleFrame.map((frame, index) => ({
        id: frame.id ?? String(index),
        url: frame.url,
        role: frame.role ?? "unknown",
        sourceType: frame.sourceType ?? "unknown",
        lifecycle: frame.lifecycle ?? "unknown",
      })),
      motionSeed: 42,
    }),
    "still",
  );
  assert.equal(
    shouldAutoAdvanceStory({
      playing: true,
      frameCount: 1,
      reducedMotion: false,
      isVisible: true,
      documentVisible: true,
    }),
    false,
  );
  assert.equal(
    shouldAutoAdvanceStory({
      playing: true,
      frameCount: 4,
      reducedMotion: true,
      isVisible: true,
      documentVisible: true,
    }),
    false,
  );
});

test("pause, offscreen, hidden, and step behavior are bounded", () => {
  const base = {
    frameCount: 4,
    reducedMotion: false,
    isVisible: true,
    documentVisible: true,
  };
  assert.equal(shouldAutoAdvanceStory({ ...base, playing: false }), false);
  assert.equal(
    shouldAutoAdvanceStory({ ...base, playing: true, isVisible: false }),
    false,
  );
  assert.equal(
    shouldAutoAdvanceStory({ ...base, playing: true, documentVisible: false }),
    false,
  );
  assert.equal(
    shouldAutoAdvanceStory({ ...base, playing: true, durationMs: 0 }),
    false,
  );
  assert.equal(shouldAutoAdvanceStory({ ...base, playing: true }), true);
  assert.equal(nextStoryFrameIndex(3, 4), 0);
  assert.equal(nextStoryFrameIndex(0, 1), 0);
});

test("filmstrip offsets wrap adjacent frames without edge jumps", () => {
  assert.equal(wrappedFilmstripOffset(0, 0, 7), 0);
  assert.equal(wrappedFilmstripOffset(6, 0, 7), -1);
  assert.equal(wrappedFilmstripOffset(0, 6, 7), 1);
  assert.equal(wrappedFilmstripOffset(4, 0, 7), -3);
  assert.equal(wrappedFilmstripOffset(0, 0, 1), 0);
});
