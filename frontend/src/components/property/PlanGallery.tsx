import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { ProjectPlansView } from "../../lib/types.ts";
import { planGalleryItems } from "../../lib/planGallery.ts";

type PlanGalleryProps = {
  plans?: ProjectPlansView;
  title?: string;
  allowedKinds?: string[];
  maxItems?: number;
  className?: string;
};

export function PlanGallery({
  plans,
  title = "Plans",
  allowedKinds = [],
  maxItems,
  className,
}: PlanGalleryProps) {
  const titleId = useId();
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const items = useMemo(() => {
    const allowed = new Set(allowedKinds);
    const filtered = planGalleryItems(plans).filter((item) => (
      allowed.size === 0 || allowed.has(item.kind)
    ));
    return filtered.slice(0, maxItems ?? filtered.length);
  }, [allowedKinds, maxItems, plans]);
  const active = activeIndex == null ? null : items[activeIndex] ?? null;

  useEffect(() => {
    if (!active) return undefined;
    const previouslyFocused = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    const previousOverflow = document.body.style.overflow;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setActiveIndex(null);
    };
    document.body.style.overflow = "hidden";
    document.addEventListener("keydown", closeOnEscape);
    closeButtonRef.current?.focus();
    return () => {
      document.body.style.overflow = previousOverflow;
      document.removeEventListener("keydown", closeOnEscape);
      previouslyFocused?.focus();
    };
  }, [active]);

  if (items.length === 0) return null;
  return (
    <section className={["plan-gallery", className].filter(Boolean).join(" ")} aria-labelledby={titleId}>
      <h2 id={titleId}>{title}</h2>
      <div className="plan-gallery__grid">
        {items.map((item, index) => (
          <button
            type="button"
            className="plan-gallery__item"
            key={item.id}
            aria-haspopup="dialog"
            onClick={() => setActiveIndex(index)}
          >
            <span className="plan-gallery__image">
              <img src={item.thumbnailUrl} alt="" loading="lazy" />
            </span>
            <strong>{item.label}</strong>
            {item.detail && <span>{item.detail}</span>}
          </button>
        ))}
      </div>
      {active && (
        <div
          className="plan-gallery__backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setActiveIndex(null);
          }}
        >
          <div className="plan-gallery__dialog" role="dialog" aria-modal="true" aria-labelledby={`${titleId}-dialog`}>
            <header>
              <div>
                <h2 id={`${titleId}-dialog`}>{active.label}</h2>
                {active.detail && <p>{active.detail}</p>}
              </div>
              <button ref={closeButtonRef} type="button" onClick={() => setActiveIndex(null)} aria-label="Close plan">
                ×
              </button>
            </header>
            <div className="plan-gallery__dialog-image">
              <img src={active.previewUrl} alt={active.label} />
            </div>
            {items.length > 1 && (
              <div className="plan-gallery__strip" aria-label="Other plans">
                {items.map((item, index) => (
                  <button
                    type="button"
                    className={index === activeIndex ? "is-active" : undefined}
                    key={item.id}
                    aria-pressed={index === activeIndex}
                    onClick={() => setActiveIndex(index)}
                  >
                    <img src={item.thumbnailUrl} alt="" />
                    <span>{item.label}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
