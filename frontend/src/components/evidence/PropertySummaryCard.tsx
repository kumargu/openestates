import type { PropertySummaryJobResponse } from "../../lib/types.ts";

type Props = {
  summaryJob: PropertySummaryJobResponse | null;
  status: "idle" | "loading" | "ready" | "error";
  onSummarize: () => void;
};

export function PropertySummaryCard({ summaryJob, status, onSummarize }: Props) {
  const isLoading = status === "loading";
  const isReady = status === "ready" && summaryJob?.summaryParagraph;
  const isError = status === "error";

  return (
    <section className="property-evidence-section property-summary-card" aria-label="AI property summary">
      <div className="property-summary-card__header">
        <div className="property-section-heading">
          <span>Summary</span>
          <h2>AI summary</h2>
        </div>
        <button
          className="property-summary-card__button"
          type="button"
          onClick={onSummarize}
          disabled={isLoading}
        >
          {isLoading ? "Preparing" : isReady || isError ? "Retry" : "Summarize"}
        </button>
      </div>

      {status === "idle" && (
        <p className="property-summary-card__empty">
          Get a short evidence-backed read from the current property facts.
        </p>
      )}

      {isLoading && (
        <div className="property-summary-card__skeleton" aria-live="polite">
          <div className="skeleton-bar" />
          <div className="skeleton-bar" />
          <div className="skeleton-bar" />
        </div>
      )}

      {isReady && (
        <>
          <p className="property-summary-card__summary">{summaryJob.summaryParagraph}</p>
          {summaryJob.evidenceRefs.length > 0 && (
            <div className="property-summary-card__receipts" aria-label="Summary receipts">
              {summaryJob.evidenceRefs.slice(0, 6).map((receipt) => (
                <span key={`${receipt.entityId}-${receipt.label}`} className="source-chip">
                  {receipt.label}
                </span>
              ))}
            </div>
          )}
        </>
      )}

      {isError && (
        <p className="property-summary-card__empty">
          Summary is not ready from the current evidence yet.
        </p>
      )}
    </section>
  );
}
