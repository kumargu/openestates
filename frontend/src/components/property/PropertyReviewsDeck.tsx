import type {
  DetailSignal,
  PropertyDetailResponse,
} from "../../lib/types.ts";
import type { StoryReviewsModel } from "../../lib/propertyStory.ts";
import type { PropertyProofMatch } from "../../lib/proof-focus.ts";
import { LabelVisualIcon } from "../../lib/LabelVisualIcon.tsx";
import { GoogleReviewsSection } from "./GoogleReviewsSection.tsx";
import "../../styles/property-fact-decks.css";

type Props = {
  model: StoryReviewsModel;
  reviews?: PropertyDetailResponse["external_reviews"] | null;
  signals?: DetailSignal[];
  focusedMatch?: PropertyProofMatch;
};

export function PropertyReviewsDeck({ model, reviews, signals, focusedMatch }: Props) {
  const reviewSignals = (signals ?? [])
    .filter((signal) => signal.label.trim())
    .slice(0, 8);
  const hasReviews = model.state !== "missing";
  const focusedOnly = !hasReviews && reviewSignals.length === 0 && Boolean(focusedMatch);
  if (!hasReviews && reviewSignals.length === 0 && !focusedMatch) return null;
  const heading = focusedOnly
    ? "Matched your search."
    : model.state === "present"
      ? "What residents say."
      : reviewSignals.length > 0
      ? "What stands out."
      : "Google reviews.";
  const kicker = focusedOnly
    ? "Living evidence"
    : model.state === "present"
      ? "Reviews"
      : "Home signals";

  return (
    <section
      id="resident-voice"
      className="property-fact-deck property-reviews-deck"
      aria-labelledby="property-reviews-deck-title"
      tabIndex={-1}
    >
      <header className="property-story-heading">
        <span>{kicker}</span>
        <h2 id="property-reviews-deck-title">{heading}</h2>
      </header>
      {reviewSignals.length > 0 && (
        <div className="property-signal-section" aria-label="Home signals">
          <span className="property-signal-section__label">Themes</span>
          <div className="property-signal-pills">
            {reviewSignals.map((signal) => (
              <span key={signal.key} className="property-signal-pill">
                <LabelVisualIcon id={signal.icon || signal.key} size={22} />
                <strong>{signal.label}</strong>
              </span>
            ))}
          </div>
        </div>
      )}
      {!hasReviews && reviewSignals.length === 0 && focusedMatch && (
        <div className="property-search-match">
          <strong>{focusedMatch.value}</strong>
          {focusedMatch.sourceUrl && (
            <a href={focusedMatch.sourceUrl} target="_blank" rel="noreferrer">
              Source ↗
            </a>
          )}
        </div>
      )}
      {hasReviews && <GoogleReviewsSection reviews={reviews} />}
    </section>
  );
}
