type RailPageControlsProps = {
  page: number;
  pageCount: number;
  onPageChange: (page: number) => void;
  label: string;
};

export function RailPageControls({
  page,
  pageCount,
  onPageChange,
  label,
}: RailPageControlsProps) {
  if (pageCount <= 1) return null;

  return (
    <div className="property-rail-controls" aria-label={label}>
      <button
        type="button"
        onClick={() => onPageChange(Math.max(0, page - 1))}
        disabled={page === 0}
        aria-label="Previous homes"
      >
        <Chevron direction="prev" />
      </button>
      <span>
        {page + 1} / {pageCount}
      </span>
      <button
        type="button"
        onClick={() => onPageChange(Math.min(pageCount - 1, page + 1))}
        disabled={page >= pageCount - 1}
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
