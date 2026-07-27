import { useEffect, useMemo, useRef, useState } from "react";
import type { EvidenceSection } from "../../lib/types.ts";
import { visibleEvidenceSections } from "../../lib/evidence.ts";

/* eslint-disable react-refresh/only-export-components */

type Props = {
  sections: EvidenceSection[];
};

type TrailFrame = {
  image_url: string;
  source_url: string;
  label: string;
  capture_date?: string;
};

function approachRoadSection(sections: EvidenceSection[]): EvidenceSection | undefined {
  return visibleEvidenceSections(sections).find((section) => section.kind === "approach_road");
}

function trailFrames(section: EvidenceSection): TrailFrame[] {
  return section.media
    ?.flatMap((strip) =>
      strip.frames
        .filter((frame) => frame.image_url)
        .map((frame) => ({
          image_url: frame.image_url,
          source_url: frame.source_url,
          label: frame.label,
          capture_date: frame.capture_date,
        })),
    ) ?? [];
}

function frameKey(frame: TrailFrame): string {
  return `${frame.image_url}::${frame.label}`;
}

export function hasApproachRoadTrail(sections: EvidenceSection[]): boolean {
  const section = approachRoadSection(sections);
  if (!section) return false;
  return trailFrames(section).length > 0;
}

export function ApproachRoadTrail({ sections }: Props) {
  const section = approachRoadSection(sections);
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const closeButtonRef = useRef<HTMLButtonElement>(null);

  const frames = useMemo(
    () => (section ? trailFrames(section).slice(0, 6) : []),
    [section],
  );

  useEffect(() => {
    if (!open) return undefined;

    const previousOverflow = document.body.style.overflow;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };

    document.body.style.overflow = "hidden";
    document.addEventListener("keydown", closeOnEscape);
    closeButtonRef.current?.focus();

    return () => {
      document.body.style.overflow = previousOverflow;
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  if (!section || frames.length === 0) return null;

  const safeActive = activeIndex % frames.length;
  const hero = frames[safeActive];
  const hasStrip = frames.length > 1;
  const viewLabel = `${frames.length} ${frames.length === 1 ? "view" : "views"}`;
  const tilePreview = frames[0];

  function openTrail() {
    setActiveIndex(0);
    setOpen(true);
  }

  function promoteFrame(nextIndex: number) {
    setActiveIndex(nextIndex);
  }

  return (
    <section className="area-trail" aria-labelledby="area-trail-title">
      <button
        type="button"
        className="detail-action-tile area-trail__tile"
        aria-haspopup="dialog"
        onClick={openTrail}
      >
        <span className="area-trail__preview">
          <img src={tilePreview.image_url} alt="" loading="lazy" />
        </span>
        <span className="area-trail__copy">
          <span className="area-trail__kicker">Approach road</span>
          <strong id="area-trail-title">Gate-side approach</strong>
          <span>{tilePreview.label} · {viewLabel}</span>
        </span>
        <span className="area-trail__open" aria-hidden="true">
          View trail
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="m9 18 6-6-6-6" />
          </svg>
        </span>
      </button>

      {open && (
        <div
          className="area-trail__backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setOpen(false);
          }}
        >
          <div
            className="area-trail__dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="area-trail-dialog-title"
          >
            <div className="area-trail__dialog-head">
              <div>
                <span>Approach road</span>
                <h2 id="area-trail-dialog-title">Gate-side approach</h2>
                <p>
                  {hero.label}
                  {viewLabel ? ` · ${viewLabel}` : ""}
                </p>
              </div>
              <button
                ref={closeButtonRef}
                type="button"
                className="area-trail__close"
                aria-label="Close approach road trail"
                onClick={() => setOpen(false)}
              >
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 6 6 18M6 6l12 12" />
                </svg>
              </button>
            </div>

            <div className="area-trail__hero" aria-live="polite">
              <img src={hero.image_url} alt={`Approach road: ${hero.label}`} />
              <span>{hero.label}</span>
            </div>

            {hasStrip && (
              <div className="area-trail__strip" aria-label="Additional approach road views">
                {frames.map((frame, index) => {
                  if (index === safeActive) return null;
                  return (
                    <button
                      key={frameKey(frame)}
                      type="button"
                      className="area-trail__thumb"
                      onClick={() => promoteFrame(index)}
                      aria-label={`Show ${frame.label} in the main panel`}
                    >
                      <img src={frame.image_url} alt="" loading="lazy" />
                      <span>{frame.label}</span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
