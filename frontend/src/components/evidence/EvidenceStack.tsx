import { useMemo, useState } from "react";
import type {
  EvidenceMediaStrip,
  EvidenceSection,
  SourceItem,
  PropertyEvidenceResponse,
} from "../../lib/types.ts";
import { bandMeta, constellationMeta, displaySourceType, groupSectionsByBand, humanizeFactText, sectionConstellation, sectionTileCount, sectionTileSignal } from "../../lib/evidence.ts";
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
  const trimmed = humanizeFactText(value.trim());
  if (trimmed.length <= 160) return trimmed;
  return `${trimmed.slice(0, 159).trimEnd()}...`;
}

function FactRow({ item }: { item: SourceItem }) {
  const url = itemSourceUrl(item);
  const sourceLabel = displaySourceType(item.source_type);
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
        {sourceLabel && (
          <span className="ev-fact__source">{sourceLabel}</span>
        )}
        {url && (
          <a className="ev-fact__link" href={url} target="_blank" rel="noreferrer" aria-label="Open source">
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

function EvidenceTile({
  section,
  active,
  onSelect,
}: {
  section: EvidenceSection;
  active: boolean;
  onSelect: () => void;
}) {
  const { constellation, meta } = useSectionContent(section);
  const signal = sectionTileSignal(section);
  const count = sectionTileCount(section);

  return (
    <button
      type="button"
      className={`ev-tile ev-tile--${constellation} ${active ? "ev-tile--active" : ""}`}
      onClick={onSelect}
      aria-pressed={active}
    >
      <span className="ev-tile__spine" aria-hidden="true" />
      <span className="ev-tile__icon"><IconForKind kind={section.kind} size={18} /></span>
      <span className="ev-tile__headings">
        <span className="ev-tile__kicker">{meta.label}</span>
        <span className="ev-tile__title">{section.title}</span>
        <span className="ev-tile__signal">{signal}</span>
      </span>
      {count != null && (
        <span className="ev-tile__count" aria-label={`${count} items`}>{count}</span>
      )}
    </button>
  );
}

function EvidenceDetailPanel({ section }: { section: EvidenceSection }) {
  const { constellation, meta, facts, media, presentation, variant, FactBody } = useSectionContent(section);

  return (
    <div
      id={`evidence-${section.kind}`}
      className={`ev-detail ev-detail--${constellation} ev-fold--variant-${variant} ev-fold--density-${presentation.density}`}
    >
      <div className="ev-detail__head">
        <span className="ev-detail__kicker">{meta.label}</span>
        <h3 className="ev-detail__title">{section.title}</h3>
        {(section.summary || section.subtitle) && (
          <p className="ev-detail__lead">
            {section.community_pulse ? section.subtitle : (section.summary || section.subtitle)}
          </p>
        )}
      </div>

      <div className="ev-detail__body">
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
  );
}

function defaultOpenKey(folds: EvidenceSection[]): string | null {
  if (folds.length === 0) return null;
  const rera = folds.find((section) => section.kind === "rera");
  if (rera) return sectionKey(rera);
  return sectionKey(folds[0]);
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

  const bands = useMemo(() => groupSectionsByBand(folds), [folds]);
  const [openKey, setOpenKey] = useState<string | null>(() => defaultOpenKey(folds));
  const openSection = folds.find((section) => sectionKey(section) === openKey) ?? null;

  if (folds.length === 0) return null;

  return (
    <section className="evidence-stack">
      <div className="property-section-heading">
        <span>Sources</span>
        <h2>Property context</h2>
      </div>

      <div className="evidence-stack__bands">
        {bands.map((band) => (
          <div key={band.id} className="evidence-stack__band">
            <h3 className="evidence-stack__band-label">{bandMeta(band.id).label}</h3>
            <div className="evidence-stack__tiles" role="tablist" aria-label={bandMeta(band.id).label}>
              {band.sections.map((section) => {
                const key = sectionKey(section);
                const active = openKey === key;
                return (
                  <EvidenceTile
                    key={key}
                    section={section}
                    active={active}
                    onSelect={() => setOpenKey(active ? null : key)}
                  />
                );
              })}
            </div>
          </div>
        ))}
      </div>

      {openSection && (
        <div className="evidence-stack__detail" role="tabpanel">
          <EvidenceDetailPanel section={openSection} />
        </div>
      )}
    </section>
  );
}
