import { useState } from "react";
import type { EvidenceSection, SourceItem } from "../../lib/types.ts";
import {
  constellationForSection,
  constellationMeta,
  evidenceHeatClass,
  summarizeEvidence,
} from "../../lib/evidence.ts";

export type EvidenceZoom = "compact" | "expanded" | "board";

type Props = {
  section: EvidenceSection;
  zoom?: EvidenceZoom;
  defaultOpen?: boolean;
};

function itemSourceUrl(item: SourceItem): string | undefined {
  return item.source_url ?? item.attributions?.find((a) => a.source_url)?.source_url;
}

function previewValue(value: string, max = 72): string {
  if (value.length <= max) return value;
  return `${value.slice(0, max - 1).trimEnd()}…`;
}

export function EvidenceSectionCard({
  section,
  zoom = "board",
  defaultOpen = false,
}: Props) {
  const [open, setOpen] = useState(defaultOpen);
  const constellation = constellationForSection(section.kind);
  const meta = constellationMeta(constellation);
  const heat = summarizeEvidence({
    property_id: "",
    entity_refs: {
      property_entity_id: "",
      society_entity_id: "",
      area_entity_id: "",
    },
    sections: [section],
  });

  const visibleItems = zoom === "compact"
    ? section.items.slice(0, 1)
    : zoom === "expanded"
      ? section.items.slice(0, section.presentation?.max_preview_items ?? 4)
      : section.items;
  const mediaFrames = section.media
    ?.flatMap((strip) =>
      strip.frames
        .filter((frame) => frame.image_url)
        .map((frame) => ({
          ...frame,
          stripTitle: strip.title,
          stripCaption: strip.caption,
        })),
    ) ?? [];
  const mediaCount = mediaFrames.length;
  const visibleMediaFrames = mediaFrames.slice(0, zoom === "compact" ? 3 : 6);

  const heatClass = heat ? evidenceHeatClass(heat.heat) : "evidence-heat--sparse";

  return (
    <article
      className={`evidence-section-card evidence-section-card--${zoom} evidence-section-card--${constellation} ${heatClass}`}
    >
      <header className="evidence-section-card__header">
        <div className="evidence-section-card__titles">
          <span className="evidence-section-card__constellation">{meta.label}</span>
          <h3 className="evidence-section-card__title">{section.title}</h3>
          {section.subtitle && (
            <p className="evidence-section-card__subtitle">{section.subtitle}</p>
          )}
        </div>
        <div className="evidence-section-card__meta">
          <span className="evidence-section-card__confidence">
            {section.confidence_pct}% conf
          </span>
          <span className="evidence-section-card__count">
            {section.items.length} facts{mediaCount > 0 ? ` · ${mediaCount} views` : ""}
          </span>
        </div>
      </header>

      {section.summary && (
        <p className="evidence-section-card__summary">{section.summary}</p>
      )}

      {visibleMediaFrames.length > 0 && (
        <div className="evidence-section-card__media-strip" aria-label={`${section.title} visual receipts`}>
          {visibleMediaFrames.map((frame) => (
            <a
              key={`${frame.image_url}-${frame.heading}`}
              className="evidence-section-card__media-frame"
              href={frame.source_url}
              target="_blank"
              rel="noreferrer"
            >
              <img src={frame.image_url} alt={`${frame.stripTitle}: ${frame.label}`} loading="lazy" />
              <span>{frame.label}</span>
            </a>
          ))}
        </div>
      )}

      {visibleItems.length > 0 && (
        <ul className="evidence-section-card__items">
          {visibleItems.map((item) => (
            <li key={`${item.entity_id}-${item.label}`} className="evidence-section-card__item">
              <div className="evidence-section-card__item-head">
                <span className="evidence-section-card__item-label">{item.label}</span>
                <span className="evidence-section-card__item-source">{item.source_type}</span>
              </div>
              <div className="evidence-section-card__item-value">
                {item.values?.length
                  ? item.values.slice(0, zoom === "compact" ? 2 : 6).map((v) => (
                      <span key={v} className="evidence-section-card__chip">{v}</span>
                    ))
                  : previewValue(item.value, zoom === "board" ? 140 : 88)}
              </div>
              {itemSourceUrl(item) && zoom !== "compact" && (
                <a
                  className="evidence-section-card__source-link"
                  href={itemSourceUrl(item)}
                  target="_blank"
                  rel="noreferrer"
                >
                  Open source
                </a>
              )}
            </li>
          ))}
        </ul>
      )}

      {visibleItems.length === 0 && mediaCount > visibleMediaFrames.length && (
        <p className="evidence-section-card__summary">
          {mediaCount - visibleMediaFrames.length} more visual receipts available in the detail evidence stack.
        </p>
      )}

      {zoom === "board" && section.items.length > visibleItems.length && (
        <button
          type="button"
          className="evidence-section-card__toggle"
          onClick={() => setOpen((v) => !v)}
        >
          {open ? "Show fewer facts" : `Show all ${section.items.length} facts`}
        </button>
      )}

      {zoom === "board" && open && section.items.length > visibleItems.length && (
        <ul className="evidence-section-card__items evidence-section-card__items--more">
          {section.items.slice(visibleItems.length).map((item) => (
            <li key={`more-${item.entity_id}-${item.label}`} className="evidence-section-card__item">
              <div className="evidence-section-card__item-head">
                <span className="evidence-section-card__item-label">{item.label}</span>
                <span className="evidence-section-card__item-source">{item.source_type}</span>
              </div>
              <div className="evidence-section-card__item-value">{item.value}</div>
            </li>
          ))}
        </ul>
      )}
    </article>
  );
}
