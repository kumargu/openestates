import { useEffect, useRef } from "react";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import { wrapPhotoIndex } from "../../lib/propertyScene.ts";

type Props = {
  title: string;
  images: string[];
  index: number;
  onIndexChange: (index: number) => void;
  onClose: () => void;
};

export function PropertyPhotoWalker({
  title,
  images,
  index,
  onIndexChange,
  onClose,
}: Props) {
  const closeRef = useRef<HTMLButtonElement>(null);
  const activeThumbRef = useRef<HTMLButtonElement>(null);
  const total = images.length;
  const safeIndex = wrapPhotoIndex(index, total);
  const current = images[safeIndex];
  const canWalk = total > 1;

  useEffect(() => {
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    closeRef.current?.focus();
    return () => {
      document.body.style.overflow = previousOverflow;
    };
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
        return;
      }
      if (!canWalk) return;
      if (event.key === "ArrowRight") {
        event.preventDefault();
        onIndexChange(wrapPhotoIndex(safeIndex + 1, total));
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        onIndexChange(wrapPhotoIndex(safeIndex - 1, total));
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [canWalk, onClose, onIndexChange, safeIndex, total]);

  useEffect(() => {
    activeThumbRef.current?.scrollIntoView({
      block: "nearest",
      inline: "center",
    });
  }, [safeIndex]);

  if (!current) return null;

  return (
    <div
      className="property-photo-walker"
      role="dialog"
      aria-modal="true"
      aria-label={`${title} photos`}
    >
      <div className="property-photo-walker__bar">
        <p className="property-photo-walker__count" aria-live="polite">
          {safeIndex + 1} / {total}
        </p>
        <button
          ref={closeRef}
          type="button"
          className="property-photo-walker__close"
          onClick={onClose}
          aria-label="Close photos"
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
        </button>
      </div>

      <div className="property-photo-walker__stage">
        {canWalk && (
          <button
            type="button"
            className="property-photo-walker__nav property-photo-walker__nav--prev"
            onClick={() => onIndexChange(wrapPhotoIndex(safeIndex - 1, total))}
            aria-label="Previous photo"
          >
            ‹
          </button>
        )}
        <ImageWithFallback
          src={current}
          alt={`${title}, photo ${safeIndex + 1} of ${total}`}
          className="property-photo-walker__image"
          loading="eager"
          fetchPriority="high"
        />
        {canWalk && (
          <button
            type="button"
            className="property-photo-walker__nav property-photo-walker__nav--next"
            onClick={() => onIndexChange(wrapPhotoIndex(safeIndex + 1, total))}
            aria-label="Next photo"
          >
            ›
          </button>
        )}
      </div>

      {canWalk && (
        <div className="property-photo-walker__strip" role="tablist" aria-label="Photos">
          {images.map((src, position) => {
            const active = position === safeIndex;
            return (
              <button
                key={src}
                ref={active ? activeThumbRef : undefined}
                type="button"
                role="tab"
                aria-selected={active}
                aria-label={`Photo ${position + 1}`}
                className={`property-photo-walker__thumb${active ? " is-active" : ""}`}
                onClick={() => onIndexChange(position)}
              >
                <ImageWithFallback
                  src={src}
                  alt=""
                  loading="lazy"
                  fetchPriority="low"
                />
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
