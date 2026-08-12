import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { ProjectPlansView } from "../../lib/types.ts";

type PlanGalleryItem = {
  id: string;
  kind: string;
  label: string;
  detail?: string;
  previewUrl: string;
  thumbnailUrl: string;
};

type PlanGalleryProps = {
  plans?: ProjectPlansView;
  title?: string;
  allowedKinds?: string[];
  maxItems?: number;
  className?: string;
};

function usablePreviewUrl(value?: string): string | null {
  if (!value) return null;
  if (value.startsWith("/media/")) return value;
  try {
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol) ? url.toString() : null;
  } catch {
    return null;
  }
}

function galleryItems(plans?: ProjectPlansView): PlanGalleryItem[] {
  if (!plans) return [];
  const items: PlanGalleryItem[] = [];
  const siteUrl = usablePreviewUrl(plans.site_overview?.preview_url);
  if (plans.site_overview && siteUrl) {
    items.push({
      id: plans.site_overview.artifact_id,
      kind: "site_plan",
      label: plans.site_overview.label,
      previewUrl: siteUrl,
      thumbnailUrl: usablePreviewUrl(plans.site_overview.thumbnail_url) ?? siteUrl,
    });
  }
  for (const plan of plans.floor_plans) {
    const previewUrl = usablePreviewUrl(plan.preview_url);
    if (!previewUrl) continue;
    const detail = [
      plan.carpet_area_sqft
        ? `${plan.carpet_area_sqft.toLocaleString("en-IN")} sq ft carpet`
        : null,
      plan.sale_area_sqft
        ? `${plan.sale_area_sqft.toLocaleString("en-IN")} sq ft sale area`
        : null,
    ].filter(Boolean).join(" · ");
    items.push({
      id: plan.artifact_id,
      kind: "floor_plan",
      label: plan.title,
      detail: detail || undefined,
      previewUrl,
      thumbnailUrl: usablePreviewUrl(plan.thumbnail_url) ?? previewUrl,
    });
  }
  for (const plan of plans.filed_plan_previews ?? []) {
    const previewUrl = usablePreviewUrl(plan.preview_url);
    if (!previewUrl) continue;
    items.push({
      id: plan.artifact_id,
      kind: plan.kind,
      label: plan.label,
      previewUrl,
      thumbnailUrl: usablePreviewUrl(plan.thumbnail_url) ?? previewUrl,
    });
  }
  return items;
}

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
    const filtered = galleryItems(plans).filter((item) => (
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
