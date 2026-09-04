type RailPageControlsProps = {
  canPrevious: boolean;
  canNext: boolean;
  rangeStart: number;
  rangeEnd: number;
  total: number;
  onPrevious: () => void;
  onNext: () => void;
  label: string;
};

export function RailPageControls({
  canPrevious,
  canNext,
  rangeStart,
  rangeEnd,
  total,
  onPrevious,
  onNext,
  label,
}: RailPageControlsProps) {
  if (!canPrevious && !canNext) return null;

  return (
    <div className="property-rail-controls" aria-label={label}>
      <button
        type="button"
        onClick={onPrevious}
        disabled={!canPrevious}
        aria-label="Previous homes"
      >
        <Chevron direction="prev" />
      </button>
      <span aria-live="polite">
        {rangeStart === rangeEnd ? rangeStart : `${rangeStart}\u2013${rangeEnd}`} of {total}
      </span>
      <button
        type="button"
        onClick={onNext}
        disabled={!canNext}
        aria-label="Next homes"
      >
        <Chevron direction="next" />
      </button>
    </div>
  );
}

function Chevron({ direction }: { direction: "prev" | "next" }) {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      aria-hidden="true"
    >
      {direction === "prev" ? (
        <path d="m15 18-6-6 6-6" />
      ) : (
        <path d="m9 18 6-6-6-6" />
      )}
    </svg>
  );
}
