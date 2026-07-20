import type { EvidenceSection } from "../../lib/types.ts";
import { visibleEvidenceSections } from "../../lib/evidence.ts";

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

export function hasApproachRoadTrail(sections: EvidenceSection[]): boolean {
  const section = approachRoadSection(sections);
  if (!section) return false;
  return trailFrames(section).length > 0;
}

export function ApproachRoadTrail({ sections }: Props) {
  const section = approachRoadSection(sections);
  if (!section) return null;

  const frames = trailFrames(section);
  if (frames.length === 0) return null;

  const hero = frames[0];

  return (
    <section className="area-trail area-trail--visual-only" aria-labelledby="area-trail-title">
      <div className="area-trail__head">
        <div>
          <span>Approach road</span>
          <h2 id="area-trail-title">Gate-side approach</h2>
        </div>
        <div className="area-trail__meta">
          <span>{frames.length} Street View frames</span>
        </div>
      </div>

      <a className="area-trail__hero" href={hero.source_url} target="_blank" rel="noreferrer">
        <img src={hero.image_url} alt={`Approach road: ${hero.label}`} loading="eager" />
        <span>{hero.label}</span>
      </a>

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
