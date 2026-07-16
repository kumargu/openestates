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

  return (
    <div className={`property-scene ${hasImages ? "property-scene--live" : "property-scene--empty"}`}>
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
            <span className="property-scene__placeholder-kicker">Gathering visuals</span>
            <strong>{societyName || title}</strong>
            <p>{loading ? "Searching project photos…" : "No verified photos yet — enrichment queued."}</p>
          </div>
        )}

        <div className="property-scene__vignette" />
        <div className="property-scene__grain" />
      </div>

      {hasImages && sceneImages.length > 1 && (
        <div className="property-scene__reel" role="tablist" aria-label="Property scenes">
          {sceneImages.map((src, index) => (
            <button
              key={src}
              type="button"
              role="tab"
              aria-selected={index === safeActive}
              className={`property-scene__frame ${index === safeActive ? "is-active" : ""}`}
              onClick={() => setActive(index)}
            >
              <img src={src} alt="" />
              <span>{sceneLabelForIndex(index)}</span>
            </button>
          ))}
        </div>
      )}

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
  );
}
