import { useEffect, useMemo, useState } from "react";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import { usePropertySceneImages } from "../../hooks/usePropertySceneImages.ts";
import { sceneLabelForIndex } from "../../lib/propertyScene.ts";

type SceneChip = { label: string; value: string };

type Props = {
  title: string;
  societyName?: string;
  heroImage?: string | null;
  images?: string[];
  societyId?: string;
  chips: SceneChip[];
};

const KEN_BURNS = [
  "property-scene__layer--pan-right",
  "property-scene__layer--pan-left",
  "property-scene__layer--zoom-in",
  "property-scene__layer--zoom-out",
  "property-scene__layer--drift-up",
];

export function PropertySceneCard({
  title,
  societyName,
  heroImage,
  images,
  societyId,
  chips,
}: Props) {
  const { images: sceneImages, loading, hasImages } = usePropertySceneImages({
    heroImage,
    images,
    societyId,
  });

  const [active, setActive] = useState(0);
  const [reducedMotion, setReducedMotion] = useState(false);
  const safeActive = sceneImages.length > 0 ? active % sceneImages.length : 0;

  useEffect(() => {
    if (reducedMotion || sceneImages.length <= 1) return undefined;
    const timer = window.setInterval(() => {
      setActive((index) => (index + 1) % sceneImages.length);
    }, 6800);
    return () => window.clearInterval(timer);
  }, [reducedMotion, sceneImages.length]);

  useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReducedMotion(mq.matches);
    update();
    mq.addEventListener("change", update);
    return () => mq.removeEventListener("change", update);
  }, []);

  const sceneLabel = useMemo(() => sceneLabelForIndex(safeActive), [safeActive]);
  const motionClass = KEN_BURNS[safeActive % KEN_BURNS.length];
  // Keep mosaic slots stable so auto-rotate only moves the living hero stage.
  const mosaicImages = sceneImages
    .map((src, index) => ({ src, index }))
    .filter(({ index }) => index > 0)
    .slice(0, 4);

  return (
    <div className={`property-scene ${hasImages ? "property-scene--live" : "property-scene--empty"}${mosaicImages.length > 0 ? " property-scene--mosaic" : ""}`}>
      <div className="property-scene__stage" aria-hidden={!hasImages}>
        {hasImages ? (
          sceneImages.map((src, index) => (
            <div
              key={src}
              className={`property-scene__layer ${motionClass} ${index === safeActive ? "is-active" : ""}`}
            >
              <ImageWithFallback
                src={src}
                alt={`${title} — ${sceneLabelForIndex(index)}`}
                className="property-scene__image"
                loading={index === 0 ? "eager" : "lazy"}
              />
            </div>
          ))
        ) : (
          <div className="property-scene__placeholder">
            <span className="property-scene__placeholder-kicker">Project photos</span>
            <strong>{societyName || title}</strong>
            <p>{loading ? "Loading photos…" : "Photos unavailable"}</p>
          </div>
        )}

        <div className="property-scene__vignette" />
        <div className="property-scene__grain" />
        <div className="property-scene__glass">
          <div className="property-scene__glass-top">
            <span className="property-scene__scene-label">{hasImages ? sceneLabel : "Preview"}</span>
            {hasImages && sceneImages.length > 1 && (
              <span className="property-scene__scene-count">
                {safeActive + 1} / {sceneImages.length}
              </span>
            )}
          </div>
          <div className="property-scene__chips">
            {chips.map((chip) => (
              <span key={chip.label} className="property-scene__chip">
                <em>{chip.label}</em> {chip.value}
              </span>
            ))}
          </div>
        </div>
      </div>

      {mosaicImages.length > 0 && (
        <div className="property-scene__mosaic" role="tablist" aria-label="Property scenes">
          {mosaicImages.map(({ src, index }, position) => (
            <button
              key={src}
              type="button"
              role="tab"
              aria-selected={index === safeActive}
              className={`property-scene__mosaic-frame${index === safeActive ? " is-active" : ""}`}
              onClick={() => setActive(index)}
            >
              <ImageWithFallback
                src={src}
                alt={`${title} — ${sceneLabelForIndex(index)}`}
                loading="lazy"
              />
              <span>{sceneLabelForIndex(index)}</span>
              {position === mosaicImages.length - 1 && sceneImages.length > 5 && (
                <b>{sceneImages.length} photos</b>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
