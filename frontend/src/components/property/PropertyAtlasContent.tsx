import { useState } from "react";
import { Link } from "react-router-dom";
import type { PropertyDetailResponse, ProofFocus } from "../../lib/types.ts";
import type { PropertyStoryModel } from "../../lib/propertyStory.ts";
import { humanizeFactText, visibleEvidenceSections } from "../../lib/evidence.ts";
import { hrefWithSearchSpan, searchSpanReferenceForTarget } from "../../lib/navigationContext.ts";
import { useSearchSpan } from "../workspace/SearchSpanContext.ts";
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

/** Existing served facts remain available below the canvas, without another drawer. */
export function PropertyAtlasFacts({ data, story, focus }: {
  data: PropertyDetailResponse;
  story: PropertyStoryModel;
  focus?: ProofFocus;
}) {
  const searchContext = useSearchSpan();
  const sections = visibleEvidenceSections(data.evidence?.sections);
  const records = [...new Map(story.recordCards.map(record => [record.href, record])).values()];
  if (!sections.length && !records.length) return null;
  return (
    <section className="property-atlas-facts" aria-label="Property information">
      {sections.map(section => {
        const focused = focus?.destinationKind === "section" && section.items.some(item =>
          item.key === focus.factKey && (!focus.entityId || item.entity_id === focus.entityId));
        return <details key={section.kind} id={`property-topic-${section.kind}`} tabIndex={-1}
          open={focused || undefined} data-proof-focused={focused || undefined}>
          <summary>{section.title}</summary>
          <dl>
            {section.items.filter(item => item.value?.trim() || item.values?.length).map((item, index) => (
              <div key={`${item.entity_id}:${item.key ?? item.label}:${index}`}>
                <dt>{item.label}</dt>
                <dd>{humanizeFactText(item.values?.length ? item.values.join(" · ") : item.value)}
                  {item.source_url && <a href={item.source_url} target="_blank" rel="noreferrer">Source ↗</a>}
                </dd>
              </div>
            ))}
          </dl>
          {section.community_pulse?.paragraph && <p>{section.community_pulse.paragraph}</p>}
          {section.media?.map((strip, index) => (
            <div className="property-atlas-facts__media" key={`${strip.kind}:${index}`}>
              {strip.frames.map((frame, frameIndex) => <figure key={`${frame.image_url}:${frameIndex}`}>
                <ImageWithFallback src={frame.image_url} alt={frame.label} loading="lazy" />
                {frame.source_url && <figcaption><a href={frame.source_url} target="_blank" rel="noreferrer">Source ↗</a></figcaption>}
              </figure>)}
            </div>
          ))}
        </details>;
      })}
      {records.length > 0 && <nav aria-label="Property reports">
        {records.map(record => <Link key={record.href} to={hrefWithSearchSpan(record.href, searchSpanReferenceForTarget(searchContext, data.property.id))}>
          {record.label}<span aria-hidden="true"> ↗</span>
        </Link>)}
      </nav>}
    </section>
  );
}
