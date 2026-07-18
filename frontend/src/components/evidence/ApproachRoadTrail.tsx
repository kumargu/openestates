import type { EvidenceSection, SourceItem } from "../../lib/types.ts";
import { visibleEvidenceSections } from "../../lib/evidence.ts";

type Props = {
  sections: EvidenceSection[];
};

type TrailFrame = {
  image_url: string;
  source_url: string;
  label: string;
  capture_date?: string;
  stripCaption?: string;
};

function itemUrl(item: SourceItem | undefined): string | undefined {
  return item?.source_url ?? item?.attributions?.find((a) => a.source_url)?.source_url;
}

function approachRoadSection(sections: EvidenceSection[]): EvidenceSection | undefined {
  return visibleEvidenceSections(sections).find((section) => section.kind === "approach_road");
}

function roadSignalItem(section: EvidenceSection): SourceItem | undefined {
  return section.items.find((item) => item.key === "approach_road_condition")
    ?? section.items.find((item) => item.label.toLowerCase().includes("review"));
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
          stripCaption: strip.caption,
        })),
    ) ?? [];
}

function compactSnippet(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length <= 96) return trimmed;
  return `${trimmed.slice(0, 95).trimEnd()}...`;
}

function reviewRead(item: SourceItem | undefined, frameCount: number): string {
  const snippets = item?.values?.filter(Boolean) ?? [];
  if (snippets.length >= 2) {
    return `Google reviews mention ${snippets.length} road signals near the approach. Use the frames to inspect whether that matches the last-lane reality.`;
  }
  if (snippets.length === 1) {
    return "A Google review calls out the approach road. The frames give the visual receipt before a site visit.";
  }
  if (frameCount > 0) {
    return "Street View frames cover the gate-side approach so you can inspect road width, surface, and turns before visiting.";
  }
  return "";
}

export function hasApproachRoadTrail(sections: EvidenceSection[]): boolean {
  const section = approachRoadSection(sections);
  if (!section) return false;
  return trailFrames(section).length > 0 || section.items.some((item) => item.values?.length || item.value);
}

export function ApproachRoadTrail({ sections }: Props) {
  const section = approachRoadSection(sections);
  if (!section) return null;

  const frames = trailFrames(section);
  const signal = roadSignalItem(section);
  const snippets = signal?.values?.filter(Boolean) ?? [];
  const read = reviewRead(signal, frames.length);
  if (frames.length === 0 && !read) return null;

  const hero = frames[0];
  const sourceUrl = hero?.source_url ?? itemUrl(signal);

  return (
    <section className="area-trail" aria-labelledby="area-trail-title">
      <div className="area-trail__head">
        <div>
          <span>Area trail</span>
          <h2 id="area-trail-title">Approach road, with receipts</h2>
        </div>
        <div className="area-trail__meta">
          {frames.length > 0 && <span>{frames.length} views</span>}
          {signal && <span>{signal.source_type}</span>}
        </div>
      </div>

      <div className="area-trail__body">
        {hero && (
          <a className="area-trail__hero" href={hero.source_url} target="_blank" rel="noreferrer">
            <img src={hero.image_url} alt={`Approach road: ${hero.label}`} loading="lazy" />
            <span>{hero.label}</span>
          </a>
        )}

        <div className="area-trail__read">
          <span className="area-trail__kicker">Review read</span>
          <p>{read}</p>
          {snippets.length > 0 && (
            <div className="area-trail__snippets">
              {snippets.slice(0, 3).map((snippet) => (
                <span key={snippet}>{compactSnippet(snippet)}</span>
              ))}
            </div>
          )}
          {sourceUrl && (
            <a className="area-trail__source" href={sourceUrl} target="_blank" rel="noreferrer">
              Open receipt
            </a>
          )}
        </div>
      </div>

      {frames.length > 1 && (
        <div className="area-trail__strip" aria-label="Additional approach road views">
          {frames.slice(1, 6).map((frame) => (
            <a
              key={`${frame.image_url}-${frame.label}`}
              className="area-trail__thumb"
              href={frame.source_url}
              target="_blank"
              rel="noreferrer"
            >
              <img src={frame.image_url} alt={`Approach road: ${frame.label}`} loading="lazy" />
              <span>{frame.label}</span>
            </a>
          ))}
        </div>
      )}
    </section>
  );
}
