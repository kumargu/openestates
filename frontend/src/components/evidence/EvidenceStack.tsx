import { useState } from "react";
import type { EvidenceSection, SourceItem, PropertyEvidenceResponse } from "../../lib/types.ts";
import { constellationForSection, constellationMeta } from "../../lib/evidence.ts";
import {
  ChevronIcon,
  GapIcon,
  LinkIcon,
  IconForKind,
  IconForLabel,
} from "./EvidenceIcons.tsx";

type StackProps = {
  evidence: PropertyEvidenceResponse | undefined;
  fallbackSections?: EvidenceSection[];
};

function itemSourceUrl(item: SourceItem): string | undefined {
  return item.source_url ?? item.attributions?.find((a) => a.source_url)?.source_url;
}

/** A single fact row — renders only when it has a real value. */
function FactRow({ item }: { item: SourceItem }) {
  const url = itemSourceUrl(item);
  const values = item.values?.filter(Boolean) ?? [];
  const hasValue = values.length > 0 || (item.value && item.value.trim().length > 0);
  if (!hasValue) return null;

  return (
    <div className="ev-fact">
      <span className="ev-fact__icon"><IconForLabel label={item.label} /></span>
      <div className="ev-fact__body">
        <div className="ev-fact__label">{item.label}</div>
        {values.length > 0 ? (
          <div className="ev-fact__chips">
            {values.slice(0, 8).map((v) => (
              <span key={v} className="ev-fact__chip">{v}</span>
            ))}
          </div>
        ) : (
          <div className="ev-fact__value">{item.value}</div>
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


  return (
    <section className={`ev-fold ${confidenceTone(section.confidence_pct)} ev-fold--${constellation} ${open ? "ev-fold--open" : ""}`}>
      <button type="button" className="ev-fold__head" onClick={() => setOpen((v) => !v)} aria-expanded={open}>
        <span className="ev-fold__spine" aria-hidden="true" />
        <span className="ev-fold__icon"><IconForKind kind={section.kind} size={18} /></span>
        <span className="ev-fold__headings">
          <span className="ev-fold__kicker">{meta.label}</span>
          <span className="ev-fold__title">{section.title}</span>
          <span className="ev-fold__read">{section.summary || section.subtitle}</span>
        </span>
        <span className="ev-fold__meta">
          <span className="ev-fold__count">{facts.length} facts</span>
          <span className="ev-fold__conf">{section.confidence_pct}%</span>
          {section.missing.length > 0 && (
            <span className="ev-fold__gap-badge">{section.missing.length} gap{section.missing.length > 1 ? "s" : ""}</span>
          )}
        </span>
        <span className="ev-fold__chevron"><ChevronIcon size={18} /></span>
      </button>

      <div className="ev-fold__wrap">
        <div className="ev-fold__inner">
          <div className="ev-fold__facts">
            {facts.map((item) => (
              <FactRow key={`${item.entity_id}-${item.label}`} item={item} />
            ))}
          </div>

          {section.missing.length > 0 && (
            <div className="ev-fold__missing">
              {section.missing.map((m) => (
                <span key={m} className="ev-fold__missing-row">
                  <GapIcon size={13} />
                  {m}
                </span>
              ))}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

export function EvidenceStack({ evidence, fallbackSections }: StackProps) {
  const sections = evidence?.sections?.length ? evidence.sections : fallbackSections ?? [];

  const ordered = [...sections].sort((a, b) => a.priority - b.priority);
  // Dynamic: a fold exists only if it carries at least one real fact.
  const folds = ordered.filter((s) =>
    s.items.some((it) => (it.values?.some(Boolean) ?? false) || (it.value && it.value.trim().length > 0)),
  );
  // Gaps from every section (including data-less ones) collected into one strip.
  const gaps = ordered.flatMap((s) =>
    s.missing.map((m) => ({ kind: s.kind, label: constellationMeta(constellationForSection(s.kind)).label, text: m })),
  );

  if (folds.length === 0 && gaps.length === 0) return null;

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

      {gaps.length > 0 && (
        <div className="evidence-stack__gaps">
          <span className="evidence-stack__gaps-label">
            <GapIcon size={14} /> Still unresolved
          </span>
          <div className="evidence-stack__gaps-list">
            {gaps.map((g) => (
              <span key={`${g.kind}-${g.text}`} className="evidence-stack__gap">
                <em>{g.label}</em> {g.text}
              </span>
            ))}
          </div>
        </div>
      )}
    </section>
  );
}
