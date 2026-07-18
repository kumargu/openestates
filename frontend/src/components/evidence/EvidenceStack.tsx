import { useState } from "react";
import type {
  EvidenceMediaStrip,
  EvidenceSection,
  SourceItem,
  PropertyEvidenceResponse,
} from "../../lib/types.ts";
import { constellationForSection, constellationMeta } from "../../lib/evidence.ts";
import {
  ChevronIcon,
  LinkIcon,
  IconForKind,
  IconForLabel,
} from "./EvidenceIcons.tsx";

type StackProps = {
  evidence: PropertyEvidenceResponse | undefined;
  fallbackSections?: EvidenceSection[];
  excludeKinds?: string[];
};

function itemSourceUrl(item: SourceItem): string | undefined {
  return item.source_url ?? item.attributions?.find((a) => a.source_url)?.source_url;
}

function isHttpUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

function httpUrlFrom(value: string): string | null {
  const trimmed = value.trim();
  if (isHttpUrl(trimmed)) return trimmed;
  const match = trimmed.match(/https?:\/\/[^\s)]+/);
  return match ? match[0].replace(/[.,;]+$/, "") : null;
}

function shortLinkLabel(item: SourceItem, index = 0): string {
  const label = item.label.trim();
  if (label.toLowerCase() === "evidence") return index === 0 ? "Open evidence" : `Open evidence ${index + 1}`;
  if (label.toLowerCase().includes("map")) return index === 0 ? "Open map" : `Open map ${index + 1}`;
  if (label.toLowerCase().includes("page")) return index === 0 ? "Open page" : `Open page ${index + 1}`;
  return index === 0 ? "Open source" : `Open source ${index + 1}`;
}

function compactValue(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length <= 160) return trimmed;
  return `${trimmed.slice(0, 159).trimEnd()}...`;
}

/** A single fact row — renders only when it has a real value. */
function FactRow({ item }: { item: SourceItem }) {
  const url = itemSourceUrl(item);
  const values = item.values?.filter(Boolean) ?? [];
  const hasValue = values.length > 0 || (item.value && item.value.trim().length > 0);
  if (!hasValue) return null;
  const valueUrl = item.value ? httpUrlFrom(item.value) : null;

  return (
    <div className="ev-fact">
      <span className="ev-fact__icon"><IconForLabel label={item.label} /></span>
      <div className="ev-fact__body">
        <div className="ev-fact__label">{item.label}</div>
        {values.length > 0 ? (
          <div className="ev-fact__chips">
            {values.slice(0, 8).map((v, index) => (
              httpUrlFrom(v) ? (
                <a
                  key={v}
                  className="ev-fact__chip ev-fact__chip--link"
                  href={httpUrlFrom(v) ?? undefined}
                  target="_blank"
                  rel="noreferrer"
                >
                  {shortLinkLabel(item, index)}
                </a>
              ) : (
                <span key={v} className="ev-fact__chip">{compactValue(v)}</span>
              )
            ))}
          </div>
        ) : valueUrl ? (
          <a
            className="ev-fact__value-link"
            href={valueUrl}
            target="_blank"
            rel="noreferrer"
          >
            {shortLinkLabel(item)}
          </a>
        ) : (
          <div className="ev-fact__value">{compactValue(item.value)}</div>
        )}
      </div>
      <div className="ev-fact__meta">
        <span className="ev-fact__source">{item.source_type}</span>
        {url && (
          <a className="ev-fact__link" href={url} target="_blank" rel="noreferrer" aria-label="Open source">
            <LinkIcon size={13} />
          </a>
        )}
      </div>
    </div>
  );
}

function confidenceTone(pct: number): string {
  if (pct >= 75) return "ev-fold--strong";
  if (pct >= 50) return "ev-fold--moderate";
  return "ev-fold--sparse";
}

function EvidenceMediaStripView({ strip }: { strip: EvidenceMediaStrip }) {
  const frames = strip.frames.filter((frame) => frame.image_url);
  if (frames.length === 0) return null;

  return (
    <div className="ev-media-strip">
      <div className="ev-media-strip__head">
        <span>{strip.caption}</span>
        <b>{strip.coverage_quality}</b>
      </div>
      <div className="ev-media-strip__frames">
        {frames.map((frame) => (
          <a
            key={`${frame.image_url}-${frame.heading}`}
            className="ev-media-strip__frame"
            href={frame.source_url}
            target="_blank"
            rel="noreferrer"
          >
            <img src={frame.image_url} alt={`${strip.title}: ${frame.label}`} loading="lazy" />
            <span>{frame.label}</span>
          </a>
        ))}
      </div>
    </div>
  );
}

function EvidenceFold({
  section,
  defaultOpen,
}: {
  section: EvidenceSection;
  defaultOpen: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const constellation = constellationForSection(section.kind);
  const meta = constellationMeta(constellation);
  const facts = section.items.filter(
    (it) => (it.values?.some(Boolean) ?? false) || (it.value && it.value.trim().length > 0),
  );
  const media = section.media?.filter((strip) => strip.frames.some((frame) => frame.image_url)) ?? [];
  const presentation = section.presentation ?? {
    variant: media.length > 0 ? "media_grid" : "fact_list",
    density: "standard",
    max_preview_items: 4,
  };

  if (facts.length === 0 && media.length === 0) return null;

  return (
    <section className={`ev-fold ${confidenceTone(section.confidence_pct)} ev-fold--${constellation} ev-fold--variant-${presentation.variant} ev-fold--density-${presentation.density} ${open ? "ev-fold--open" : ""}`}>
      <button type="button" className="ev-fold__head" onClick={() => setOpen((v) => !v)} aria-expanded={open}>
        <span className="ev-fold__spine" aria-hidden="true" />
        <span className="ev-fold__icon"><IconForKind kind={section.kind} size={18} /></span>
        <span className="ev-fold__headings">
          <span className="ev-fold__kicker">{meta.label}</span>
          <span className="ev-fold__title">{section.title}</span>
          <span className="ev-fold__read">{section.summary || section.subtitle}</span>
        </span>
        <span className="ev-fold__meta">
          <span className="ev-fold__count">
            {facts.length} facts{media.length > 0 ? ` · ${media.length} media` : ""}
          </span>
          <span className="ev-fold__conf">{section.confidence_pct}%</span>
        </span>
        <span className="ev-fold__chevron"><ChevronIcon size={18} /></span>
      </button>

      <div className="ev-fold__wrap">
        <div className="ev-fold__inner">
          {media.map((strip) => (
            <EvidenceMediaStripView key={`${section.kind}-${strip.kind}`} strip={strip} />
          ))}
          <div className="ev-fold__facts">
            {facts.map((item) => (
              <FactRow key={`${item.entity_id}-${item.label}`} item={item} />
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}

export function EvidenceStack({ evidence, fallbackSections, excludeKinds = [] }: StackProps) {
  const sections = evidence?.sections?.length ? evidence.sections : fallbackSections ?? [];
  const excluded = new Set(excludeKinds);

  const ordered = [...sections]
    .filter((section) => !excluded.has(section.kind))
    .sort((a, b) => a.priority - b.priority);
  // Dynamic: a fold exists only if it carries at least one real fact or media receipt.
  const folds = ordered.filter((s) =>
    s.items.some((it) => (it.values?.some(Boolean) ?? false) || (it.value && it.value.trim().length > 0))
      || (s.media?.some((strip) => strip.frames.some((frame) => frame.image_url)) ?? false),
  );

  if (folds.length === 0) return null;

  return (
    <section className="evidence-stack">
      <div className="property-section-heading">
        <span>Evidence stack</span>
        <h2>What we know, layered by proof</h2>
      </div>

      <div className="evidence-stack__folds">
        {folds.map((section, index) => (
          <EvidenceFold
            key={`${section.kind}-${section.title}`}
            section={section}
            defaultOpen={index === 0}
          />
        ))}
      </div>

    </section>
  );
}
