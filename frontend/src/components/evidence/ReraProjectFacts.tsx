import type { ReraDossier, ReraInfo } from "../../lib/types.ts";
import { reraFactGroups } from "../../lib/reraProjectFacts.ts";
import { LinkIcon } from "./EvidenceIcons.tsx";
import { NotebookCommentAnchor } from "../notebook/NotebookCommentAnchor.tsx";

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
              Source
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
              <h3>{card.title}</h3>
              {card.detail && <p>{card.detail}</p>}
            </article>
          ))}
        </div>
      )}

      {(freshness || sourceUrl) && (
        <footer className="rera-project-facts__footer">
          {freshness && <span>Checked {freshness}</span>}
          {sourceUrl && (
            <a href={sourceUrl} target="_blank" rel="noreferrer">
              <LinkIcon size={13} />
              Source
            </a>
          )}
        </footer>
      )}
    </div>
  );
}
