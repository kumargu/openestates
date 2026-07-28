import { TAGS, formatCr, propertyById, tagById, type PropertyId, type TagId } from "../data.ts";
import { useNotebook, type NotebookNote } from "../store.tsx";
import { CrossLinks, type PageNav } from "../components/Pinable.tsx";

type TagRow = {
  tag: TagId;
  label: string;
  values: Record<PropertyId, string>;
  sources: Record<PropertyId, string>;
};

export function ComparePage({
  nav,
  onOpenProperty,
}: {
  nav: PageNav;
  onOpenProperty: (id: PropertyId) => void;
}) {
  const { compareIds, notes, compareStyle, setCompareStyle, toggleCompare, propertyIds } =
    useNotebook();

  const ids = compareIds.length >= 2 ? compareIds : [];
  const ready = ids.length >= 2;
  const rows = ready ? buildTagRows(ids, notes) : [];

  return (
    <div className={`page page--compare compare--${compareStyle}`}>
      <header className="page-hero">
        <div className="page-hero__row">
          <div>
            <p className="eyebrow">Compare</p>
            <h1>Compare by tags</h1>
            <p className="lede">
              Rows come from shared notebook tags. Click a home name to open Property. Handwritten
              notes only exist in Notebook — pins auto-carry from Property and Plan.
            </p>
          </div>
          <CrossLinks nav={nav} showCompare={false} />
        </div>
      </header>

      <div className="style-row">
        <span>Layout</span>
        {(
          [
            ["columns", "Editorial columns"],
            ["matrix", "Calm matrix"],
            ["mobile", "Topic stack"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            className={`chip${compareStyle === id ? " is-active" : ""}`}
            onClick={() => setCompareStyle(id)}
          >
            {label}
          </button>
        ))}
      </div>

      <div className="compare-select-bar">
        {propertyIds.map((id) => {
          const p = propertyById(id);
          const on = compareIds.includes(id);
          return (
            <button
              key={id}
              type="button"
              className={`select-pill${on ? " is-on" : ""}`}
              onClick={() => toggleCompare(id)}
            >
              {on ? "●" : "○"} {p.short}
            </button>
          );
        })}
        {propertyIds.length < 2 && (
          <p className="compare-hint">Add at least two homes to the notebook first.</p>
        )}
      </div>

      {!ready ? (
        <div className="empty-notebook">
          <h2>Select two homes in Notebook</h2>
          <p>Pin the same tags on both homes (Schools, Down payment, Legal…), then select them.</p>
        </div>
      ) : rows.length === 0 ? (
        <div className="empty-notebook">
          <h2>No overlapping tags yet</h2>
          <p>
            Pin facts on Property/Plan or add a handwritten note in Notebook with a shared tag.
          </p>
        </div>
      ) : compareStyle === "mobile" ? (
        <MobileStack ids={ids} rows={rows} onOpenProperty={onOpenProperty} />
      ) : compareStyle === "matrix" ? (
        <Matrix ids={ids} rows={rows} onOpenProperty={onOpenProperty} />
      ) : (
        <Columns ids={ids} rows={rows} onOpenProperty={onOpenProperty} />
      )}
    </div>
  );
}

function Columns({
  ids,
  rows,
  onOpenProperty,
}: {
  ids: PropertyId[];
  rows: TagRow[];
  onOpenProperty: (id: PropertyId) => void;
}) {
  return (
    <div className="compare-columns" style={{ ["--cols" as string]: ids.length }}>
      <div className="compare-columns__head">
        <div className="compare-columns__label" />
        {ids.map((id, i) => {
          const p = propertyById(id);
          return (
            <button
              key={id}
              type="button"
              className="compare-columns__home compare-columns__home--btn"
              style={{ animationDelay: `${i * 80}ms` }}
              onClick={() => onOpenProperty(id)}
            >
              <h2>
                {p.icon} {p.short}
              </h2>
              <p>
                {formatCr(p.priceCr)} · {p.bhk} · open
              </p>
            </button>
          );
        })}
      </div>

      <div className="compare-group">
        <h3>From notebook tags</h3>
        {rows.map((r, i) => {
          const tag = tagById(r.tag);
          return (
            <div
              key={r.tag}
              className="compare-row is-notebook"
              style={{ animationDelay: `${120 + i * 40}ms` }}
            >
              <div className="compare-row__label">
                <span className="tag-pill" style={{ background: tag.color, color: tag.ink }}>
                  {tag.label}
                </span>
              </div>
              {ids.map((id) => (
                <div key={id} className="compare-row__cell">
                  <strong className="compare-cell-main">{r.values[id] || "—"}</strong>
                  {r.sources[id] && <span className="compare-cell-src">{r.sources[id]}</span>}
                </div>
              ))}
            </div>
          );
        })}
      </div>

      <div className="compare-group">
        <h3>Baseline</h3>
        <div className="compare-row">
          <div className="compare-row__label">Asking</div>
          {ids.map((id) => (
            <div key={id} className="compare-row__cell">
              {formatCr(propertyById(id).priceCr)}
            </div>
          ))}
        </div>
        <div className="compare-row">
          <div className="compare-row__label">Sale area</div>
          {ids.map((id) => (
            <div key={id} className="compare-row__cell">
              {propertyById(id).sqft.toLocaleString("en-IN")} sqft
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function Matrix({
  ids,
  rows,
  onOpenProperty,
}: {
  ids: PropertyId[];
  rows: TagRow[];
  onOpenProperty: (id: PropertyId) => void;
}) {
  return (
    <div className="compare-matrix-wrap">
      <table className="compare-matrix">
        <thead>
          <tr>
            <th>Tag</th>
            {ids.map((id) => (
              <th key={id}>
                <button type="button" className="compare-th-btn" onClick={() => onOpenProperty(id)}>
                  {propertyById(id).short}
                </button>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => {
            const tag = tagById(r.tag);
            return (
              <tr key={r.tag} className="is-notebook">
                <th>
                  <span className="tag-pill" style={{ background: tag.color, color: tag.ink }}>
                    {tag.label}
                  </span>
                </th>
                {ids.map((id) => (
                  <td key={id}>{r.values[id] || "—"}</td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function MobileStack({
  ids,
  rows,
  onOpenProperty,
}: {
  ids: PropertyId[];
  rows: TagRow[];
  onOpenProperty: (id: PropertyId) => void;
}) {
  return (
    <div className="compare-stack">
      {rows.map((r) => {
        const tag = tagById(r.tag);
        return (
          <section key={r.tag} className="stack-topic">
            <h3>
              <span className="tag-pill" style={{ background: tag.color, color: tag.ink }}>
                {tag.label}
              </span>
            </h3>
            <div className="stack-scroll">
              {ids.map((id) => {
                const p = propertyById(id);
                return (
                  <button
                    key={id}
                    type="button"
                    className="stack-card stack-card--btn"
                    onClick={() => onOpenProperty(id)}
                  >
                    <h4>
                      {p.icon} {p.short}
                    </h4>
                    <p>
                      <strong>{r.values[id] || "—"}</strong>
                    </p>
                  </button>
                );
              })}
            </div>
          </section>
        );
      })}
    </div>
  );
}

function buildTagRows(ids: PropertyId[], notes: NotebookNote[]): TagRow[] {
  const relevant = notes.filter((n) => ids.includes(n.propertyId));
  const tagIds = TAGS.map((t) => t.id).filter((tag) => relevant.some((n) => n.tag === tag));

  return tagIds.map((tag) => {
    const values = {} as Record<PropertyId, string>;
    const sources = {} as Record<PropertyId, string>;
    for (const id of ids) {
      const candidates = relevant.filter((n) => n.propertyId === id && n.tag === tag);
      if (!candidates.length) {
        values[id] = "";
        sources[id] = "";
        continue;
      }
      const preferred =
        candidates.find((n) => n.kind === "plan") ||
        candidates.find((n) => n.kind === "fact" || n.kind === "theme") ||
        candidates[0];
      values[id] = preferred.label;
      sources[id] =
        preferred.kind === "handwritten"
          ? "Your note"
          : preferred.selectionText
            ? "Selected text"
            : preferred.kind === "plan"
              ? "Plan pin"
              : preferred.kind === "theme"
                ? "Theme"
                : "Saved fact";
    }
    return { tag, label: tagById(tag).label, values, sources };
  });
}
