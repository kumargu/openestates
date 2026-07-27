import { useMemo, useState } from "react";
import type {
  EvidenceMediaStrip,
  EvidenceSection,
  SourceItem,
  PropertyEvidenceResponse,
  ReraInfo,
} from "../../lib/types.ts";
import { canShowBuyerSource, canShowSourceProvenance, displaySourceType, humanizeFactText, sectionConstellation, sectionTileCount, sectionTileSignal } from "../../lib/evidence.ts";
import { reraFactCount, reraFactGroups } from "../../lib/reraProjectFacts.ts";
import {
  LinkIcon,
  IconForKind,
  IconForLabel,
} from "./EvidenceIcons.tsx";
import { CommunityPulseCard } from "./CommunityPulseCard.tsx";
import { ReraProjectFacts } from "./ReraProjectFacts.tsx";

type StackProps = {
  evidence: PropertyEvidenceResponse | undefined;
  rera?: ReraInfo | null;
  googleReviews?: {
    google_rating?: number;
    google_review_count?: number;
    google_reviews_url?: string;
  };
  excludeKinds?: string[];
};

function sectionKey(section: EvidenceSection): string {
  return `${section.kind}-${section.title}`;
}

function structuredReraSection(): EvidenceSection {
  return {
    kind: "rera",
    title: "RERA",
    summary: "",
    subtitle: "Official project registration and delivery record.",
    scope: "society",
    relationship: "project registration",
    priority: 10,
    constellation: "trust",
    source_types: ["Rera"],
    entity_ids: [],
    presentation: {
      variant: "timeline",
      density: "compact",
      max_preview_items: 4,
    },
    items: [],
    missing: [],
  };
}

function canShowItemSource(item: SourceItem): boolean {
  return canShowBuyerSource(item.source_type, item.source_display);
}

function itemSourceUrl(item: SourceItem): string | undefined {
  if (!canShowSourceProvenance(item.source_type, item.source_display)) return undefined;
  if (item.source_url) {
    return item.source_url;
  }
  return item.attributions?.find((attribution) =>
    canShowSourceProvenance(attribution.source_type, attribution.source_display) && attribution.source_url)?.source_url;
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

function itemText(item: SourceItem): string {
  const values = item.values?.filter(Boolean) ?? [];
  return values.length > 0 ? values.join(" ") : item.value;
}

function itemToken(item: SourceItem): string {
  return `${item.key ?? ""} ${item.label} ${itemText(item)}`.toLowerCase();
}

function hasAnyToken(item: SourceItem, tokens: string[]): boolean {
  const haystack = itemToken(item);
  return tokens.some((token) => haystack.includes(token));
}

function itemValue(item: SourceItem): string | null {
  const values = item.values?.filter(Boolean) ?? [];
  if (values.length > 0) return compactValue(values[0]);
  if (item.value?.trim()) return compactValue(item.value);
  return null;
}

function firstFact(facts: SourceItem[], tokens: string[]): SourceItem | null {
  return facts.find((item) => hasAnyToken(item, tokens)) ?? null;
}

function factValue(facts: SourceItem[], tokens: string[]): string | null {
  const item = firstFact(facts, tokens);
  return item ? itemValue(item) : null;
}

function formatStoryNumber(value: string | null, suffix: string): string | null {
  if (!value) return null;
  const match = value.match(/-?\d+(?:\.\d+)?/);
  return match ? `${Number(match[0]).toLocaleString("en-IN")}${suffix}` : value;
}

function uniqueNonEmpty(values: Array<string | null | undefined>): string[] {
  return [...new Set(values.filter((value): value is string => Boolean(value?.trim())))];
}

function FactRow({ item }: { item: SourceItem }) {
  const url = itemSourceUrl(item);
  const sourceLabel = canShowItemSource(item)
    ? displaySourceType(item.source_type, item.source_display)
    : null;
  const values = item.values
    ?.filter((value) => Boolean(value) && (canShowItemSource(item) || !httpUrlFrom(value)))
    ?? [];
  const hasValue = values.length > 0 || (item.value && item.value.trim().length > 0);
  if (!hasValue) return null;
  const valueUrl = item.value ? httpUrlFrom(item.value) : null;
  const canLinkValue = canShowSourceProvenance(item.source_type, item.source_display);

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

function WaterResilienceStory({ facts }: { facts: SourceItem[] }) {
  const groundwater = factValue(facts, ["groundwater"]);
  const waterDistance = formatStoryNumber(
    factValue(facts, ["water_body_distance", "water body distance", "nearest water body"]),
    " m",
  );
  const resilience = uniqueNonEmpty([
    firstFact(facts, ["borewell_existing_count", "existing borewell", "borewell"])
      ? "Borewell exists"
      : null,
    firstFact(facts, ["rainwater_harvesting_present", "rainwater harvesting"])
      ? "Rainwater harvesting"
      : null,
    formatStoryNumber(
      factValue(facts, ["rainwater_harvesting_area", "harvesting area", "recharge area"]),
      " sqm",
    ),
    formatStoryNumber(factValue(facts, ["stp_capacity_kld", "stp capacity"]), " KLD STP"),
  ]);

  return (
    <div className="ev-water-story">
      {groundwater && (
        <div className="ev-water-story__rail" aria-label={`${groundwater} groundwater potential`}>
          <span />
          <b>{groundwater}</b>
        </div>
      )}
      <div className="ev-water-story__legend">
        {waterDistance && <span>Water body {waterDistance}</span>}
        {resilience.map((value) => <span key={value}>{value}</span>)}
      </div>
    </div>
  );
}

function useSectionContent(section: EvidenceSection) {
  const constellation = sectionConstellation(section);
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

  return { constellation, facts, media, presentation, variant, FactBody };
}

function closedSectionSignal(
  section: EvidenceSection,
  facts: SourceItem[],
  rera?: ReraInfo | null,
  googleReviews?: StackProps["googleReviews"],
): string {
  if (section.community_pulse) {
    const sentiment = humanizeFactText(section.community_pulse.sentiment_band);
    if (googleReviews?.google_rating != null && googleReviews.google_rating > 0) {
      const reviewCount = googleReviews.google_review_count;
      return [
        `Google ${googleReviews.google_rating.toFixed(1)}`,
        reviewCount != null && reviewCount > 0
          ? `${reviewCount.toLocaleString("en-IN")} reviews`
          : null,
        sentiment,
      ].filter(Boolean).join(" · ");
    }
    const positiveCount = section.community_pulse.positives.length;
    return positiveCount > 0
      ? `${sentiment} · ${positiveCount} positive theme${positiveCount === 1 ? "" : "s"}`
      : sentiment;
  }
  if (section.kind === "rera") {
    if (rera) {
      const groups = reraFactGroups(rera);
      const status = groups
        .find((group) => group.id === "registration")
        ?.rows.find((row) => row.label === "Status")?.value;
      const target = groups
        .find((group) => group.id === "schedule")
        ?.rows.find((row) => row.label === "Current target")?.value;
      return uniqueNonEmpty([
        status,
        target ? `Target ${target}` : null,
      ]).join(" · ");
    }
    const compact = (value: string | null) =>
      value?.replace(/^[^:]{1,36}:\s*/, "").trim() || null;
    const status = compact(factValue(facts, [
      "rera_status",
      "registration status",
      "project status",
      "approved",
    ]));
    const completion = compact(factValue(facts, [
      "project_actual_completion_date",
      "project_revised_completion_date",
      "rera_completion_date",
      "completion date",
    ]));
    const parts = uniqueNonEmpty([
      status,
      completion ? `Target ${completion}` : null,
    ]);
    if (parts.length > 0) return parts.join(" · ");
    return `${facts.length} RERA project fact${facts.length === 1 ? "" : "s"}`;
  }
  return sectionTileSignal(section);
}

function EvidenceFold({
  section,
  rera,
  googleReviews,
  open,
  onToggle,
}: {
  section: EvidenceSection;
  rera?: ReraInfo | null;
  googleReviews?: StackProps["googleReviews"];
  open: boolean;
  onToggle: () => void;
}) {
  const { constellation, facts, media, presentation, variant, FactBody } = useSectionContent(section);
  const signal = closedSectionSignal(section, facts, rera, googleReviews);
  const count = section.kind === "rera" && rera ? reraFactCount(rera) : sectionTileCount(section);
  const panelId = `evidence-${section.kind}`;

  return (
    <div
      className={`detail-action-tile ev-fold ev-fold--${constellation} ev-fold--variant-${variant} ev-fold--density-${presentation.density}${open ? " ev-fold--open" : ""}`}
    >
      <button
        type="button"
        className="ev-fold__head"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={onToggle}
      >
        <span className="ev-fold__icon"><IconForKind kind={section.kind} size={18} /></span>
        <span className="ev-fold__headings">
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
          {section.kind !== "rera" && (section.summary || section.subtitle) && (
            <p className="ev-fold__lead">
              {section.community_pulse ? section.subtitle : (section.summary || section.subtitle)}
            </p>
          )}
          {variant === "story" && section.community_pulse ? (
            <CommunityPulseCard pulse={section.community_pulse} />
          ) : section.kind === "rera" && rera ? (
            <ReraProjectFacts rera={rera} />
          ) : section.kind === "water_context" ? (
            <WaterResilienceStory facts={facts} />
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

export function EvidenceStack({
  evidence,
  rera,
  googleReviews,
  excludeKinds = [],
}: StackProps) {
  const folds = useMemo(() => {
    const excluded = new Set(excludeKinds);
    const sections = [...(evidence?.sections ?? [])];
    const structuredReraFacts = rera ? reraFactCount(rera) : 0;
    if (structuredReraFacts > 0 && !sections.some((section) => section.kind === "rera")) {
      sections.push(structuredReraSection());
    }
    return sections
      .filter((section) => !excluded.has(section.kind))
      .sort((a, b) => a.priority - b.priority)
      .filter((section) =>
        (section.kind === "rera" && structuredReraFacts > 0)
        || hasRenderableContent(section));
  }, [evidence?.sections, excludeKinds, rera]);

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
              rera={rera}
              googleReviews={googleReviews}
              open={openKeys.has(key)}
              onToggle={() => setOpenKeys((current) => toggleKey(current, key))}
            />
          );
        })}
      </div>
    </section>
  );
}
