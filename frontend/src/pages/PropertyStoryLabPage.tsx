import { useMemo, useState, type ReactNode } from "react";
import { Helmet } from "react-helmet-async";
import { AroundThisHomePlate } from "../components/evidence/AroundThisHomePlate.tsx";
import { NotebookCommentAnchor } from "../components/notebook/NotebookCommentAnchor.tsx";
import { PropertyArrivalFilm } from "../components/property/PropertyArrivalFilm.tsx";
import { PropertyReraTeaser } from "../components/property/PropertyReraTeaser.tsx";
import { PropertyReviewsDeck } from "../components/property/PropertyReviewsDeck.tsx";
import {
  PropertySceneCard,
  PropertySceneFacts,
  PropertySceneIdentity,
  type StoryPlaybackSpeed,
  type StoryScenePlayback,
} from "../components/property/PropertySceneCard.tsx";
import { PropertyShortCompare } from "../components/property/PropertyShortCompare.tsx";
import { SaveHeartButton } from "../components/SaveHeartButton.tsx";
import { PUBLIC_BRAND_NAME } from "../lib/brand.ts";
import {
  nextStoryFrameIndex,
  projectPropertyStory,
  type StoryMotionTheme,
} from "../lib/propertyStory.ts";
import { hasAroundThisHomePlate } from "../lib/nearbyPlateProjection.ts";
import {
  storyLabDetailFixture,
  storyLabMediaFixture,
  type StoryLabCoverage,
  type StoryLabImageCount,
  type StoryLabLifecycle,
  type StoryLabPropertyFixture,
  type StoryLabProvenance,
  type StoryLabReraState,
  type StoryLabReviewState,
} from "../lib/propertyStoryFixtures.ts";
import "../styles/story-lab.css";

type LabViewport = "desktop" | "tablet" | "mobile";
type LabMotionTheme = "auto" | StoryMotionTheme;
type LabDeck =
  | "page"
  | "hero"
  | "map"
  | "arrival"
  | "reviews"
  | "record"
  | "compare";

export function PropertyStoryLabPage() {
  const [propertyId, setPropertyId] =
    useState<StoryLabPropertyFixture>("fixture-prestige-lakeside-3bhk");
  const [deck, setDeck] = useState<LabDeck>("page");
  const [viewport, setViewport] = useState<LabViewport>("desktop");
  const [coverage, setCoverage] = useState<StoryLabCoverage>("rich");
  const [imageCount, setImageCount] = useState<StoryLabImageCount>("many");
  const [lifecycle, setLifecycle] =
    useState<StoryLabLifecycle>("under-construction");
  const [provenance, setProvenance] =
    useState<StoryLabProvenance>("mixed");
  const [reviews, setReviews] = useState<StoryLabReviewState>("present");
  const [rera, setRera] = useState<StoryLabReraState>("complete");
  const [theme, setTheme] = useState<LabMotionTheme>("auto");
  const [playing, setPlaying] = useState(true);
  const [activeIndex, setActiveIndex] = useState(0);
  const [speed, setSpeed] = useState<StoryPlaybackSpeed>(1);
  const [reducedMotion, setReducedMotion] = useState(false);
  const [visibility, setVisibility] =
    useState<StoryScenePlayback["visibility"]>("visible");

  const detail = useMemo(
    () =>
      storyLabDetailFixture({
        propertyId,
        coverage,
        lifecycle,
        reviews,
        rera,
      }),
    [coverage, lifecycle, propertyId, rera, reviews],
  );
  const media = useMemo(
    () => storyLabMediaFixture({ count: imageCount, provenance }),
    [imageCount, provenance],
  );
  const story = useMemo(
    () =>
      projectPropertyStory(detail, {
        media,
        mapAvailable: hasAroundThisHomePlate(detail.map_context ?? null),
        motionTheme: theme === "auto" ? undefined : theme,
      }),
    [detail, media, theme],
  );
  const frameCount = deck === "hero" || deck === "page"
    ? story.media.frames.length
    : deck === "arrival"
      ? story.arrival.frames.length
      : 0;
  const mapAvailable = story.map.available;
  const reviewsAvailable = story.reviews.state !== "missing";
  const recordAvailable = story.recordCards.length > 0;
  const compareAvailable =
    story.comparisons.length === 3 && Boolean(story.compareHref);

  function selectDeck(nextDeck: LabDeck) {
    setDeck(nextDeck);
    setActiveIndex(0);
  }

  const actions = (
    <>
      <SaveHeartButton
        propertyId={story.identity.propertyId}
        className="property-action-link property-action-save"
        label="Save"
      />
      <NotebookCommentAnchor
        propertyId={story.identity.propertyId}
        labels={[]}
        detail={story.identity.title}
        source="Property Story Lab"
        label="Note"
      />
    </>
  );

  return (
    <div className="story-lab">
      <Helmet>
        <title>Property Story Lab — {PUBLIC_BRAND_NAME}</title>
      </Helmet>

      <header className="story-lab__header">
        <div>
          <span>Internal preview</span>
          <h1>Property Story Lab</h1>
          <p>Production projection and decks, isolated for review.</p>
        </div>
        <div className="story-lab__status" aria-live="polite">
          <strong>{story.coverage.level}</strong>
          <span>
            {story.coverage.availableDecks}/{story.coverage.totalDecks} decks
          </span>
          <span>{story.motionTheme}</span>
        </div>
      </header>

      <div className="story-lab__workspace">
        <aside className="story-lab__controls" aria-label="Story Lab controls">
          <LabSelect
            label="Property fixture"
            value={propertyId}
            onChange={(value) => {
              setPropertyId(value as StoryLabPropertyFixture);
              setActiveIndex(0);
            }}
          >
            <option value="fixture-prestige-lakeside-3bhk">
              Prestige Lakeside
            </option>
            <option value="fixture-sobha-royal-pavilion-4bhk">
              Sobha Royal Pavilion
            </option>
            <option value="fixture-vaswani-starlight-3bhk">
              Vaswani Starlight
            </option>
          </LabSelect>
          <LabSelect
            label="Viewport"
            value={viewport}
            onChange={(value) => setViewport(value as LabViewport)}
          >
            <option value="desktop">Desktop</option>
            <option value="tablet">Tablet</option>
            <option value="mobile">Mobile</option>
          </LabSelect>
          <LabSelect
            label="Coverage"
            value={coverage}
            onChange={(value) => {
              setCoverage(value as StoryLabCoverage);
              setActiveIndex(0);
            }}
          >
            <option value="rich">Rich</option>
            <option value="partial">Partial</option>
            <option value="sparse">Sparse</option>
          </LabSelect>
          <LabSelect
            label="Images"
            value={imageCount}
            onChange={(value) => setImageCount(value as StoryLabImageCount)}
          >
            <option value="many">Many</option>
            <option value="single">One</option>
            <option value="none">None</option>
          </LabSelect>
          <LabSelect
            label="Lifecycle"
            value={lifecycle}
            onChange={(value) => setLifecycle(value as StoryLabLifecycle)}
          >
            <option value="ready">Ready</option>
            <option value="under-construction">Under construction</option>
          </LabSelect>
          <LabSelect
            label="Provenance"
            value={provenance}
            onChange={(value) => setProvenance(value as StoryLabProvenance)}
          >
            <option value="mixed">Current + render</option>
            <option value="current">Current</option>
            <option value="render">Render</option>
          </LabSelect>
          <LabSelect
            label="Reviews"
            value={reviews}
            onChange={(value) => setReviews(value as StoryLabReviewState)}
          >
            <option value="present">Present</option>
            <option value="unresolved">Unresolved</option>
            <option value="empty">Empty</option>
          </LabSelect>
          <LabSelect
            label="RERA"
            value={rera}
            onChange={(value) => setRera(value as StoryLabReraState)}
          >
            <option value="complete">Complete</option>
            <option value="partial">Partial</option>
            <option value="missing">Missing</option>
          </LabSelect>
          <LabSelect
            label="Motion theme"
            value={theme}
            onChange={(value) => setTheme(value as LabMotionTheme)}
          >
            <option value="auto">Auto</option>
            <option value="quiet-pan">Quiet pan</option>
            <option value="architectural-drift">Architectural drift</option>
            <option value="slow-push">Slow push</option>
            <option value="editorial-cut">Editorial cut</option>
            <option value="still">Still</option>
          </LabSelect>
          <LabSelect
            label="Speed"
            value={String(speed)}
            onChange={(value) => setSpeed(Number(value) as StoryPlaybackSpeed)}
          >
            <option value="0.5">0.5×</option>
            <option value="1">1×</option>
            <option value="2">2×</option>
          </LabSelect>
          <LabSelect
            label="Visibility"
            value={visibility ?? "auto"}
            onChange={(value) =>
              setVisibility(value as StoryScenePlayback["visibility"])
            }
          >
            <option value="visible">Visible</option>
            <option value="hidden">Document hidden</option>
            <option value="offscreen">Offscreen</option>
            <option value="auto">Browser state</option>
          </LabSelect>

          <label className="story-lab__check">
            <input
              type="checkbox"
              checked={reducedMotion}
              onChange={(event) => setReducedMotion(event.target.checked)}
            />
            Reduced motion
          </label>

          <div className="story-lab__transport">
            <button type="button" onClick={() => setPlaying(!playing)}>
              {playing ? "Pause" : "Play"}
            </button>
            <button
              type="button"
              disabled={frameCount <= 1}
              onClick={() =>
                setActiveIndex(
                  nextStoryFrameIndex(activeIndex, frameCount),
                )
              }
            >
              Step
            </button>
          </div>
        </aside>

        <section className="story-lab__preview">
          <nav className="story-lab__decks" aria-label="Story deck">
            <button
              type="button"
              className={deck === "page" ? "is-active" : ""}
              aria-pressed={deck === "page"}
              onClick={() => selectDeck("page")}
            >
              Full page
            </button>
            <button
              type="button"
              className={deck === "hero" ? "is-active" : ""}
              aria-pressed={deck === "hero"}
              onClick={() => selectDeck("hero")}
            >
              Hero
            </button>
            <button
              type="button"
              className={deck === "map" ? "is-active" : ""}
              aria-pressed={deck === "map"}
              onClick={() => selectDeck("map")}
            >
              Map
              {!mapAvailable && <span>omitted</span>}
            </button>
            <button
              type="button"
              className={deck === "arrival" ? "is-active" : ""}
              aria-pressed={deck === "arrival"}
              onClick={() => selectDeck("arrival")}
            >
              Arrival
              {story.arrival.frames.length === 0 && <span>omitted</span>}
            </button>
            <button
              type="button"
              className={deck === "reviews" ? "is-active" : ""}
              aria-pressed={deck === "reviews"}
              onClick={() => selectDeck("reviews")}
            >
              Reviews
              {!reviewsAvailable && <span>omitted</span>}
            </button>
            <button
              type="button"
              className={deck === "record" ? "is-active" : ""}
              aria-pressed={deck === "record"}
              onClick={() => selectDeck("record")}
            >
              RERA
              {!recordAvailable && <span>omitted</span>}
            </button>
            <button
              type="button"
              className={deck === "compare" ? "is-active" : ""}
              aria-pressed={deck === "compare"}
              onClick={() => selectDeck("compare")}
            >
              Compare
              {!compareAvailable && <span>omitted</span>}
            </button>
          </nav>
          <div
            className={`story-lab__viewport story-lab__viewport--${viewport}`}
          >
            {deck === "page" && (
              <div
                key={`page:${propertyId}:${imageCount}:${provenance}`}
                className="property-story-page story-lab__production-shell"
              >
                <div className="property-scene property-scene--identity-only">
                  <PropertySceneIdentity
                    story={story}
                    actions={actions}
                    showFacts={false}
                  />
                </div>
                <PropertySceneFacts story={story} pageScoped />
                <PropertySceneCard
                  story={story}
                  showIdentity={false}
                  cinematicMotion={false}
                  playback={{
                    activeIndex,
                    playing,
                    speed,
                    reducedMotion,
                    visibility,
                    onActiveIndexChange: setActiveIndex,
                    onPlayingChange: setPlaying,
                  }}
                />
                <main className="property-clean-flow">
                  {mapAvailable && detail.map_context && (
                    <section
                      id="around-this-home"
                      className="property-map-section"
                    >
                      <AroundThisHomePlate
                        propertyId={story.identity.propertyId}
                        context={detail.map_context}
                      />
                    </section>
                  )}
                  <PropertyArrivalFilm
                    propertyId={story.identity.propertyId}
                    title={story.identity.title}
                    frames={story.arrival.frames}
                    cinematicMotion={false}
                    playback={{
                      playing,
                      speed,
                      reducedMotion,
                      visibility,
                      onPlayingChange: setPlaying,
                    }}
                  />
                  <PropertyReviewsDeck
                    model={story.reviews}
                    reviews={detail.external_reviews}
                  />
                  <PropertyReraTeaser cards={story.recordCards} />
                  <PropertyShortCompare
                    homes={story.comparisons}
                    compareHref={story.compareHref}
                  />
                </main>
              </div>
            )}
            {deck === "hero" && (
              <PropertySceneCard
                key={`${propertyId}:${imageCount}:${provenance}`}
                story={story}
                actions={actions}
                cinematicMotion={false}
                playback={{
                  activeIndex,
                  playing,
                  speed,
                  reducedMotion,
                  visibility,
                  onActiveIndexChange: setActiveIndex,
                  onPlayingChange: setPlaying,
                }}
              />
            )}
            {deck === "map" && mapAvailable && detail.map_context && (
              <AroundThisHomePlate
                propertyId={story.identity.propertyId}
                context={detail.map_context}
              />
            )}
            {deck === "arrival" && story.arrival.frames.length > 0 && (
              <PropertyArrivalFilm
                propertyId={story.identity.propertyId}
                title={story.identity.title}
                frames={story.arrival.frames}
                cinematicMotion={false}
                playback={{
                  activeIndex,
                  playing,
                  speed,
                  reducedMotion,
                  visibility,
                  onActiveIndexChange: setActiveIndex,
                  onPlayingChange: setPlaying,
                }}
              />
            )}
            {deck === "reviews" && reviewsAvailable && (
              <PropertyReviewsDeck
                model={story.reviews}
                reviews={detail.external_reviews}
              />
            )}
            {deck === "record" && recordAvailable && (
              <PropertyReraTeaser cards={story.recordCards} />
            )}
            {deck === "compare" && compareAvailable && (
              <PropertyShortCompare
                homes={story.comparisons}
                compareHref={story.compareHref}
              />
            )}
            {((deck === "map" && !mapAvailable)
              || (deck === "arrival" && story.arrival.frames.length === 0)
              || (deck === "reviews" && !reviewsAvailable)
              || (deck === "record" && !recordAvailable)
              || (deck === "compare" && !compareAvailable)) && (
              <div className="story-lab__omitted">
                <strong>Deck omitted</strong>
                <span>This fixture has no usable {deck} evidence.</span>
              </div>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}

function LabSelect({
  label,
  value,
  onChange,
  children,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  children: ReactNode;
}) {
  return (
    <label className="story-lab__field">
      <span>{label}</span>
      <select
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        {children}
      </select>
    </label>
  );
}
