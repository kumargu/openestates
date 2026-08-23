import type {
  ExternalReviewCard,
  PropertyDetailResponse,
} from "../../lib/types.ts";
import { formatGoogleRating } from "../../lib/reviewFormatting.ts";

type Props = {
  reviews?: PropertyDetailResponse["external_reviews"] | null;
};

function hasKnownNumber(
  value: number | null | undefined,
): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function formatReviewCount(
  value: number | null | undefined,
): string | null {
  if (!hasKnownNumber(value)) return null;
  return `${value.toLocaleString("en-IN")} Google ${
    value === 1 ? "review" : "reviews"
  }`;
}

function reviewSpaceCost(review: ExternalReviewCard): number {
  const words = review.text.trim().split(/\s+/).filter(Boolean).length;
  if (words <= 32) return 1;
  if (words <= 70) return 1.8;
  return 2.4;
}

function fitReviewCards(
  reviewCards: ExternalReviewCard[],
  budget = 22,
): ExternalReviewCard[] {
  const selected: ExternalReviewCard[] = [];
  let used = 0;
  for (const review of reviewCards) {
    const cost = reviewSpaceCost(review);
    if (selected.length >= 8 && used + cost > budget) break;
    if (selected.length >= 12) break;
    selected.push(review);
    used += cost;
  }
  return selected;
}

function reviewDateLabel(value?: string): string | undefined {
  const label = value?.trim();
  if (!label) return undefined;
  if (!/^\d{4}-\d{2}-\d{2}T/.test(label)) return label;
  const parsed = new Date(label);
  if (Number.isNaN(parsed.getTime())) return label;
  return new Intl.DateTimeFormat("en-IN", {
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  }).format(parsed);
}

function reviewText(value: string): string {
  return value.replace(/\*\*([^*]+)\*\*/g, "$1").trim();
}

export function GoogleReviewsSection({ reviews }: Props) {
  const googleUrl = reviews?.google_reviews_url;
  const rating = formatGoogleRating(reviews?.google_rating);
  const reviewCount = formatReviewCount(reviews?.google_review_count);
  const reviewCards = fitReviewCards(reviews?.reviews ?? []);
  const reviewButtonLabel = reviewCount
    ? "Show all reviews"
    : "Open Google reviews";

  if (!googleUrl && reviewCards.length === 0 && !rating) return null;

  return (
    <section
      className="property-google-reviews"
      aria-label="Google reviews"
    >
      <div className="property-google-reviews__summary">
        <strong>
          {rating ? `★ ${rating}` : "Google reviews"}
          {reviewCount ? ` · ${reviewCount}` : ""}
        </strong>
      </div>

      {reviewCards.length > 0 && (
        <div className="property-review-grid">
          {reviewCards.map((review) => {
            const dateLabel = reviewDateLabel(review.date_label);
            return (
              <article key={review.id} className="property-review-card">
                {(review.rating || dateLabel) && (
                <p className="property-review-card__meta">
                  {review.rating && (
                    <span>{"★".repeat(Math.round(review.rating))}</span>
                  )}
                  {review.rating && dateLabel && " · "}
                  {dateLabel}
                </p>
                )}
                <p>{reviewText(review.text)}</p>
              </article>
            );
          })}
        </div>
      )}

      {googleUrl && (
        <a
          className="property-review-more"
          href={googleUrl}
          target="_blank"
          rel="noreferrer"
        >
          {reviewButtonLabel}
        </a>
      )}
    </section>
  );
}
