import type { ReraDecisionCard, ReraDossier, ReraInfo } from "../../lib/types.ts";
import { reraFactGroups } from "../../lib/reraProjectFacts.ts";
import { LinkIcon } from "./EvidenceIcons.tsx";
import { NotebookCommentAnchor } from "../notebook/NotebookCommentAnchor.tsx";
import { NotebookPinButton } from "../notebook/NotebookPinButton.tsx";

function formatFreshness(value?: string): string | null {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return new Intl.DateTimeFormat("en-IN", {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(date);
}

function formatDate(value?: string): string | null {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("en-IN", {
    month: "short",
    year: "numeric",
  }).format(date);
}

function httpUrl(value?: string): string | null {
  if (!value) return null;
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:" ? url.toString() : null;
  } catch {
    return null;
  }
}

function toneClass(tone?: string): string {
  if (!tone || tone === "default") return "";
  return `is-${tone}`;
}

function cardLabels(card: ReraDecisionCard): string[] {
  return reraNotebookLabels(card.labels, `${card.id} ${card.title}`);
}

function reraNotebookLabels(labels: string[] | undefined, context: string): string[] {
  const next = labels?.filter(Boolean) ?? [];
  const isComplaint = /complaint/i.test(context) || next.includes("complaints");
  if (isComplaint) {
    return [
      "complaints",
      "risk",
      ...next.filter((label) => label !== "complaints" && label !== "risk"),
    ].filter((label, index, all) => all.indexOf(label) === index).slice(0, 4);
  }
  return next.length ? [...new Set(next)].slice(0, 4) : ["legal"];
}

function LegacyReraFacts({
  rera,
  propertyId,
}: {
  rera: ReraInfo;
  propertyId?: string;
}) {
  const groups = reraFactGroups(rera);
  const freshness = formatFreshness(rera.last_verified);
  const sourceUrl = httpUrl(rera.rera_portal_url);

  return (
    <div className="rera-project-facts notebook-comment-surface" aria-label="RERA project facts">
      {propertyId && (
        <NotebookCommentAnchor
          propertyId={propertyId}
          labels={["legal"]}
          detail="RERA"
          source="RERA"
        />
      )}
      <div className="rera-project-facts__grid">
        {groups.map((group) => (
          <section key={group.id} className="rera-project-facts__group">
            <h3>{group.label}</h3>
            <dl>
              {group.rows.map((row) => (
                <div key={`${group.id}-${row.label}`} className="rera-project-facts__row">
                  <dt>{row.label}</dt>
                  <dd
                    className={[
                      row.tone && row.tone !== "default" ? `is-${row.tone}` : "",
                      row.code ? "is-code" : "",
                    ].filter(Boolean).join(" ")}
                  >
                    <span>{row.value}</span>
                  </dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
      </div>

      {(freshness || sourceUrl) && (
        <footer className="rera-project-facts__footer">
          {freshness && <span>Checked {freshness}</span>}
          {sourceUrl && (
            <a href={sourceUrl} target="_blank" rel="noreferrer">
              <LinkIcon size={13} />
              RERA source
            </a>
          )}
        </footer>
      )}
    </div>
  );
}

export function ReraProjectFacts({
  rera,
  dossier,
  propertyId,
}: {
  rera?: ReraInfo | null;
  dossier?: ReraDossier | null;
  propertyId?: string;
}) {
  if (!dossier) {
    if (!rera) return null;
    return <LegacyReraFacts rera={rera} propertyId={propertyId} />;
  }

  const freshness = formatFreshness(dossier.source.last_verified);
  const sourceUrl = httpUrl(dossier.source.portal_url);
  const timelineItems = [
    { label: "Start", value: formatDate(dossier.timeline.start_date) },
    { label: "Original target", value: formatDate(dossier.timeline.original_completion_date) },
    { label: "Current target", value: formatDate(dossier.timeline.completion_date) },
    {
      label: "Movement",
      value: dossier.timeline.delay_months && dossier.timeline.delay_months > 0
        ? `${dossier.timeline.delay_months} months`
        : null,
      tone: dossier.timeline.delay_months && dossier.timeline.delay_months > 0 ? "watch" : "neutral",
    },
  ].filter((item) => item.value);

  return (
    <div className="rera-dossier notebook-comment-surface" aria-label="RERA project dossier">
      {propertyId && (
        <NotebookCommentAnchor
          propertyId={propertyId}
          labels={["legal"]}
          detail="RERA"
          source="RERA"
        />
      )}

      {dossier.summary_cards.length > 0 && (
        <div className="rera-dossier__cards">
          {dossier.summary_cards.slice(0, 6).map((card) => (
            <article key={card.id} className={`rera-dossier-card ${toneClass(card.tone)}`.trim()}>
              <div className="rera-dossier-card__head">
                <span>{card.source}</span>
                {propertyId && (
                  <NotebookPinButton
                    propertyId={propertyId}
                    catalogKey={`rera:${dossier.society_id}:card:${card.id}`}
                    title={card.title}
                    detail={card.detail}
                    source={card.source}
                    labels={cardLabels(card)}
                    className="rera-dossier__pin"
                  />
                )}
              </div>
              <h3>{card.title}</h3>
              {card.detail && <p>{card.detail}</p>}
              {card.validation_notes.length > 0 && (
                <small>{card.validation_notes[0]}</small>
              )}
            </article>
          ))}
        </div>
      )}

      {dossier.compare_items.length > 0 && propertyId && (
        <section className="rera-dossier__compare" aria-label="RERA facts for notebook compare">
          {dossier.compare_items.slice(0, 7).map((item) => (
            <div key={item.key} className={`rera-dossier-compare ${toneClass(item.tone)}`.trim()}>
              <span>{item.label}</span>
              <strong>{item.value}</strong>
              <NotebookPinButton
                propertyId={propertyId}
                catalogKey={`rera:${dossier.society_id}:compare:${item.key}`}
                title={`${item.label}: ${item.value}`}
                detail="RERA compare fact"
                source="RERA"
                labels={reraNotebookLabels(item.labels, `${item.key} ${item.label}`)}
                className="rera-dossier__pin"
              />
            </div>
          ))}
        </section>
      )}

      <div className="rera-dossier__body">
        {dossier.complaint_sections.length > 0 && (
          <section className="rera-dossier-panel rera-dossier-panel--complaints">
            <h3>Complaint read</h3>
            <div className="rera-dossier-complaints">
              {dossier.complaint_sections.map((section) => (
                <article key={section.scope} className="rera-dossier-complaint">
                  <div className="rera-dossier-complaint__head">
                    <span>{section.label}</span>
                    <strong>{section.total.toLocaleString("en-IN")}</strong>
                  </div>
                  <div className="rera-dossier-complaint__meta">
                    <span>{section.open.toLocaleString("en-IN")} open</span>
                    <span>{section.disposed.toLocaleString("en-IN")} disposed</span>
                  </div>
                  {section.top_themes.length > 0 && (
                    <div className="rera-dossier__chips">
                      {section.top_themes.map((theme) => (
                        <span key={`${section.scope}-${theme.label}`}>
                          {theme.label} · {theme.count}
                        </span>
                      ))}
                    </div>
                  )}
                  {section.sample_subjects.length > 0 && (
                    <ul>
                      {section.sample_subjects.slice(0, 2).map((subject) => (
                        <li key={subject}>{subject}</li>
                      ))}
                    </ul>
                  )}
                </article>
              ))}
            </div>
          </section>
        )}

        {dossier.document_sections.length > 0 && (
          <section className="rera-dossier-panel">
            <h3>Official files</h3>
            <div className="rera-dossier-files">
              {dossier.document_sections.slice(0, 6).map((section) => (
                <div key={section.group} className="rera-dossier-file">
                  <span>{section.label}</span>
                  <strong>{section.count}</strong>
                  {section.kinds.length > 0 && <small>{section.kinds.slice(0, 3).join(", ")}</small>}
                </div>
              ))}
            </div>
          </section>
        )}

        {timelineItems.length > 0 && (
          <section className="rera-dossier-panel">
            <h3>Timeline</h3>
            <div className="rera-dossier-timeline">
              {timelineItems.map((item) => (
                <div key={item.label} className={toneClass(item.tone)}>
                  <span>{item.label}</span>
                  <strong>{item.value}</strong>
                </div>
              ))}
            </div>
          </section>
        )}

        {dossier.legal_checks.length > 0 && (
          <section className="rera-dossier-panel">
            <h3>Legal checks</h3>
            <div className="rera-dossier-checks">
              {dossier.legal_checks.map((check) => (
                <div key={check.key} className={toneClass(check.tone)}>
                  <span>{check.label}</span>
                  <strong>{check.value}</strong>
                </div>
              ))}
            </div>
          </section>
        )}
      </div>

      {(freshness || sourceUrl) && (
        <footer className="rera-project-facts__footer">
          {freshness && <span>Checked {freshness}</span>}
          {sourceUrl && (
            <a href={sourceUrl} target="_blank" rel="noreferrer">
              <LinkIcon size={13} />
              RERA source
            </a>
          )}
        </footer>
      )}
    </div>
  );
}
