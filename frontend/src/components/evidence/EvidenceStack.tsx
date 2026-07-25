import { useMemo, useState } from "react";
import type {
  EvidenceMediaStrip,
  EvidenceSection,
  SourceItem,
  PropertyEvidenceResponse,
} from "../../lib/types.ts";
import { canShowBuyerSource, constellationMeta, displaySourceType, humanizeFactText, sectionConstellation, sectionTileCount, sectionTileSignal } from "../../lib/evidence.ts";
import {
  LinkIcon,
  IconForKind,
  IconForLabel,
} from "./EvidenceIcons.tsx";
import { CommunityPulseCard } from "./CommunityPulseCard.tsx";

type StackProps = {
  evidence: PropertyEvidenceResponse | undefined;
  excludeKinds?: string[];
};

function sectionKey(section: EvidenceSection): string {
  return `${section.kind}-${section.title}`;
}

function canShowItemSource(item: SourceItem): boolean {
  const sourceType = item.source_type.trim().toLowerCase();
  if (sourceType.includes("rera")) return true;
  if (!sourceType.includes("google")) return false;
  const key = item.key?.toLowerCase() ?? "";
  const label = item.label.toLowerCase();
  const relationship = item.relationship?.toLowerCase() ?? "";
  return key.includes("review")
    || label.includes("review")
    || relationship.includes("review");
}

function itemSourceUrl(item: SourceItem): string | undefined {
  if (!canShowItemSource(item)) return undefined;
  if (item.source_url) {
    return item.source_url;
  }
  return item.attributions?.find((attribution) =>
    canShowBuyerSource(attribution.source_type) && attribution.source_url)?.source_url;
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
  if (label.toLowerCase() === "evidence") return index === 0 ? "Evidence" : `Evidence ${index + 1}`;
  if (label.toLowerCase().includes("map")) return index === 0 ? "Map" : `Map ${index + 1}`;
  if (label.toLowerCase().includes("page")) return index === 0 ? "Page" : `Page ${index + 1}`;
  return index === 0 ? "Source" : `Source ${index + 1}`;
}

function compactValue(value: string): string {
  const trimmed = humanizeFactText(value.trim());
  if (trimmed.length <= 160) return trimmed;
  return `${trimmed.slice(0, 159).trimEnd()}...`;
}

function FactRow({ item }: { item: SourceItem }) {
  const url = itemSourceUrl(item);
  const sourceLabel = canShowItemSource(item) ? displaySourceType(item.source_type) : null;
  const values = item.values
    ?.filter((value) => Boolean(value) && (canShowItemSource(item) || !httpUrlFrom(value)))
    ?? [];
  const hasValue = values.length > 0 || (item.value && item.value.trim().length > 0);
  if (!hasValue) return null;
  const valueUrl = item.value ? httpUrlFrom(item.value) : null;
  const canLinkValue = canShowItemSource(item);

  return (
    <div className="ev-fact">
      <span className="ev-fact__icon"><IconForLabel label={item.label} /></span>
      <div className="ev-fact__body">
        <div className="ev-fact__label">{item.label}</div>
        {values.length > 0 ? (
          <div className="ev-fact__chips">
            {values.slice(0, 8).map((v, index) => (
              httpUrlFrom(v) && canLinkValue ? (
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
        ) : valueUrl && canLinkValue ? (
          <a
            className="ev-fact__value-link"
            href={valueUrl}
            target="_blank"
            rel="noreferrer"
          >
            {shortLinkLabel(item)}
          </a>
        ) : valueUrl ? (
          <div className="ev-fact__value">Available</div>
        ) : (
          <div className="ev-fact__value">{compactValue(item.value)}</div>
        )}
      </div>
      <div className="ev-fact__meta">
        {sourceLabel && (
          <span className="ev-fact__source">{sourceLabel}</span>
        )}
        {url && (
          <a className="ev-fact__link" href={url} target="_blank" rel="noreferrer" aria-label="Source">
            <LinkIcon size={13} />
          </a>
        )}
      </div>
    </div>
  );
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
          <div
            key={`${frame.image_url}-${frame.heading}`}
            className="ev-media-strip__frame"
          >
            <img src={frame.image_url} alt={`${strip.title}: ${frame.label}`} loading="lazy" />
            <span>{frame.label}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function FactGridBody({ facts }: { facts: SourceItem[] }) {
  return (
    <div className="ev-fold__facts ev-fold__facts--grid">
      {facts.map((item) => (
        <FactRow key={`${item.entity_id}-${item.label}`} item={item} />
      ))}
    </div>
  );
}

function FactListBody({ facts }: { facts: SourceItem[] }) {
  return (
    <div className="ev-fold__facts">
      {facts.map((item) => (
        <FactRow key={`${item.entity_id}-${item.label}`} item={item} />
      ))}
    </div>
  );
}

function useSectionContent(section: EvidenceSection) {
  const constellation = sectionConstellation(section);
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
  const variant = presentation.variant;
  const FactBody = variant === "fact_grid" || variant === "risk_grid"
    ? FactGridBody
    : FactListBody;

  return { constellation, meta, facts, media, presentation, variant, FactBody };
}


function EvidenceFold({
  section,
  open,
  onToggle,
}: {
  section: EvidenceSection;
  open: boolean;
  onToggle: () => void;
}) {
  const { constellation, meta, facts, media, presentation, variant, FactBody } = useSectionContent(section);
  const signal = sectionTileSignal(section);
  const count = sectionTileCount(section);
  const panelId = `evidence-${section.kind}`;

  return (
    <div
      className={`ev-fold ev-fold--${constellation} ev-fold--variant-${variant} ev-fold--density-${presentation.density}${open ? " ev-fold--open" : ""}`}
    >
      <span className="ev-fold__spine" aria-hidden="true" />
      <button
        type="button"
        className="ev-fold__head"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={onToggle}
      >
        <span className="ev-fold__icon"><IconForKind kind={section.kind} size={18} /></span>
        <span className="ev-fold__headings">
          <span className="ev-fold__kicker">{meta.label}</span>
          <span className="ev-fold__title">{section.title}</span>
          <span className="ev-fold__read">{signal}</span>
        </span>
        <span className="ev-fold__meta">
          {count != null && <span>{count}</span>}
          <span className="ev-fold__chevron" aria-hidden="true">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="m6 9 6 6 6-6" />
            </svg>
          </span>
        </span>
      </button>

      <div id={panelId} className="ev-fold__wrap">
        <div className="ev-fold__inner">
          {(section.summary || section.subtitle) && (
            <p className="ev-fold__lead">
              {section.community_pulse ? section.subtitle : (section.summary || section.subtitle)}
            </p>
          )}
          {variant === "story" && section.community_pulse ? (
            <CommunityPulseCard pulse={section.community_pulse} />
          ) : (
            <>
              {section.community_pulse && (
                <CommunityPulseCard pulse={section.community_pulse} />
              )}
              {media.length > 0 && media.map((strip) => (
                <EvidenceMediaStripView key={`${section.kind}-${strip.kind}`} strip={strip} />
              ))}
              {facts.length > 0 && <FactBody facts={facts} />}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function toggleKey(keys: Set<string>, key: string): Set<string> {
  const next = new Set(keys);
  if (next.has(key)) {
    next.delete(key);
  } else {
    next.add(key);
  }
  return next;
}

function hasRenderableContent(section: EvidenceSection): boolean {
  const facts = section.items.filter(
    (it) => (it.values?.some(Boolean) ?? false) || (it.value && it.value.trim().length > 0),
  );
  const media = section.media?.filter((strip) => strip.frames.some((frame) => frame.image_url)) ?? [];
  return facts.length > 0 || media.length > 0 || section.community_pulse != null;
}

export function EvidenceStack({ evidence, excludeKinds = [] }: StackProps) {
  const excluded = new Set(excludeKinds);

  const folds = useMemo(() => {
    const sections = evidence?.sections ?? [];
    return [...sections]
      .filter((section) => !excluded.has(section.kind))
      .sort((a, b) => a.priority - b.priority)
      .filter(hasRenderableContent);
  }, [evidence?.sections, excludeKinds]);

  const [openKeys, setOpenKeys] = useState<Set<string>>(() => new Set());

  if (folds.length === 0) return null;

  return (
    <section className="evidence-stack">
      <div className="property-section-heading">
        <span>Sources</span>
        <h2>Property context</h2>
      </div>

      <div className="evidence-stack__rows" aria-label="Property context sources">
        {folds.map((section) => {
          const key = sectionKey(section);
          return (
            <EvidenceFold
              key={key}
              section={section}
              open={openKeys.has(key)}
              onToggle={() => setOpenKeys((current) => toggleKey(current, key))}
            />
          );
        })}
      </div>
    </section>
  );
}
