import assert from "node:assert/strict";
import test from "node:test";
import {
  nextStoryFrameIndex,
  primaryStoryFactKeys,
  projectPropertyStory,
  selectStoryMotionTheme,
  shouldAutoAdvanceStory,
  stableStoryHash,
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
    ["hero", "decision"],
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
  assert.equal(shouldAutoAdvanceStory({ ...base, playing: true }), true);
  assert.equal(nextStoryFrameIndex(3, 4), 0);
  assert.equal(nextStoryFrameIndex(0, 1), 0);
});
