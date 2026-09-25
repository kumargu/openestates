import { useState } from "react";
import type { PropertyStoryModel } from "../../lib/propertyStory.ts";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import { AtlasIcon } from "./AtlasIcon.tsx";

export function PropertyAtlasPhotos({ story }: { story: PropertyStoryModel }) {
  const [index, setIndex] = useState(0);
  const images = story.media.galleryUrls;
  const current = images[index];
  if (!current) return <p>No photos available for this home.</p>;
  const frame = story.media.frames.find((candidate) => candidate.url === current);
  return (
    <div className="property-atlas-photos">
      <ImageWithFallback src={current} alt={`Property view ${index + 1}`} loading="eager" className="property-atlas-photos__image" />
      <div className="property-atlas-photos__controls">
        <button type="button" aria-label="Previous photo" disabled={images.length < 2} onClick={() => setIndex((index + images.length - 1) % images.length)}><AtlasIcon name="previous" /></button>
        <span aria-live="polite">{index + 1} / {images.length}{frame?.lifecycle === "proposed" ? " · Proposed render" : ""}</span>
        <button type="button" aria-label="Next photo" disabled={images.length < 2} onClick={() => setIndex((index + 1) % images.length)}><AtlasIcon name="next" /></button>
      </div>
      <div className="property-atlas-photos__thumbnails" aria-label="Choose a photo">
        {images.map((url, photoIndex) => (
          <button type="button" key={url} aria-label={`Photo ${photoIndex + 1}`} aria-pressed={photoIndex === index} onClick={() => setIndex(photoIndex)}>
            <ImageWithFallback src={url} alt="" loading="lazy" />
          </button>
        ))}
      </div>
      {frame?.sourceUrl && <a href={frame.sourceUrl} target="_blank" rel="noreferrer">Source ↗</a>}
    </div>
  );
}
