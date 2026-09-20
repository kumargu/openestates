import { useState } from "react";
import { Link } from "react-router-dom";
import type { PropertyDetailResponse, ProofFocus } from "../../lib/types.ts";
import type { PropertyStoryModel } from "../../lib/propertyStory.ts";
import { humanizeFactText, visibleEvidenceSections } from "../../lib/evidence.ts";
import { hrefWithSearchSpan, searchSpanReferenceForTarget } from "../../lib/navigationContext.ts";
import { useSearchSpan } from "../workspace/SearchSpanContext.ts";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import { AtlasIcon } from "./AtlasIcon.tsx";

type EvidenceSection = NonNullable<PropertyDetailResponse["evidence"]>["sections"][number];

function compactMarketItems(section: EvidenceSection) {
  const seenSources = new Set<string>();
  const limit = section.presentation?.max_preview_items ?? 6;
  return section.items
    .filter((item) => item.value?.trim() || item.values?.length)
    .filter((item) => {
      const sourceIdentity = item.source_url?.trim();
      if (!sourceIdentity) return true;
      if (seenSources.has(sourceIdentity)) return false;
      seenSources.add(sourceIdentity);
      return true;
    })
    .slice(0, limit);
}

function marketValue(item: ReturnType<typeof compactMarketItems>[number]) {
  const value = humanizeFactText(item.values?.length ? item.values.join(" · ") : item.value).trim();
  const separator = value.indexOf(":");
  return separator > 0 && separator < 64 ? value.slice(separator + 1).trim() : value;
}

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

/** The lower property chapter owns market context; other evidence stays on its primary surface. */
export function PropertyAtlasFacts({ data, story, focus }: {
  data: PropertyDetailResponse;
  story: PropertyStoryModel;
  focus?: ProofFocus;
}) {
  const searchContext = useSearchSpan();
  const sections = visibleEvidenceSections(data.evidence?.sections);
  const market = sections.find((section) => section.kind === "market");
  const items = market ? compactMarketItems(market) : [];
  const records = [...new Map(story.recordCards.map(record => [record.href, record])).values()];
  if (!market && !records.length) return null;
  const focused = Boolean(market && focus?.destinationKind === "section" && market.items.some(item =>
    item.key === focus.factKey && (!focus.entityId || item.entity_id === focus.entityId)));
  return (
    <section className="property-atlas-facts" aria-label="Property information">
      <header>
        <h2>{market?.title ?? "Market trail"}</h2>
        {records.length > 0 && <nav aria-label="Property reports">
          {records.map(record => <Link key={record.href} to={hrefWithSearchSpan(record.href, searchSpanReferenceForTarget(searchContext, data.property.id))}>
            {record.label}<span aria-hidden="true"> ↗</span>
          </Link>)}
        </nav>}
      </header>
      {market && items.length > 0 ? (
        <dl id="property-topic-market" tabIndex={-1} data-proof-focused={focused || undefined}>
          {items.map((item, index) => (
            <div key={`${item.entity_id}:${item.key ?? item.label}:${index}`}>
              <dt>{item.label}</dt>
              <dd>{marketValue(item)}
                {item.source_url && <a href={item.source_url} target="_blank" rel="noreferrer" aria-label={`Open source for ${item.label}`}>↗</a>}
              </dd>
            </div>
          ))}
        </dl>
      ) : null}
    </section>
  );
}
