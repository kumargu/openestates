import type {
  DetailSignal,
  PropertyDetailResponse,
} from "../../lib/types.ts";
import type { StoryReviewsModel } from "../../lib/propertyStory.ts";
import { LabelVisualIcon } from "../../lib/LabelVisualIcon.tsx";
import { GoogleReviewsSection } from "./GoogleReviewsSection.tsx";
import "../../styles/property-fact-decks.css";

type Props = {
  model: StoryReviewsModel;
  reviews?: PropertyDetailResponse["external_reviews"] | null;
  signals?: DetailSignal[];
};

export function PropertyReviewsDeck({ model, reviews, signals }: Props) {
  const reviewSignals = (signals ?? [])
    .filter((signal) => signal.label.trim())
    .slice(0, 8);
  const hasReviews = model.state !== "missing";
  if (!hasReviews && reviewSignals.length === 0) return null;
  const heading = model.state === "present"
    ? "What residents say."
    : reviewSignals.length > 0
      ? "What stands out."
      : "Google reviews.";

  return (
    <section
      id="resident-voice"
      className="property-fact-deck property-reviews-deck"
      aria-labelledby="property-reviews-deck-title"
    >
      <header className="property-story-heading">
        <span>{model.state === "present" ? "Reviews" : "Home signals"}</span>
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
      {hasReviews && <GoogleReviewsSection reviews={reviews} />}
    </section>
  );
}
