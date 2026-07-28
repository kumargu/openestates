import { CATALOG, PROPERTIES, formatCr, type PropertyId } from "../data.ts";
import { useNotebook } from "../store.tsx";
import { NotebookIcon } from "../components/Ui.tsx";

export function DiscoverPage({ onOpenProperty }: { onOpenProperty: (id: PropertyId) => void }) {
  const { isPropertyInNotebook, toggleProperty, notes } = useNotebook();

  return (
    <div className="page page--discover">
      <header className="page-hero">
        <p className="eyebrow">Discover</p>
        <h1>quiet 3BHK near good schools under 2.5Cr</h1>
        <p className="lede">
          Save a home here, then pin facts on Property and Plan. Handwritten notes only in Notebook.
          Compare joins on tags.
        </p>
      </header>

      <div className="result-list">
        {PROPERTIES.map((p, i) => {
          const saved = isPropertyInNotebook(p.id);
          const noteCount = notes.filter((n) => n.propertyId === p.id).length;
          const school = CATALOG.find((c) => c.propertyId === p.id && c.tag === "schools");
          return (
            <article
              key={p.id}
              className={`result-card${saved ? " is-saved" : ""}`}
              style={{ animationDelay: `${i * 60}ms` }}
            >
              <button type="button" className="result-card__main" onClick={() => onOpenProperty(p.id)}>
                <div className="result-card__rank">{i + 1}</div>
                <div>
                  <h2>{p.name}</h2>
                  <p className="result-card__meta">
                    {p.bhk} · {formatCr(p.priceCr)} · {p.area}
                  </p>
                  <p className="result-card__why">{p.whyHere.join(" · ")}</p>
                  {school && <p className="result-card__proof">{school.label}</p>}
                </div>
              </button>
              <div className="result-card__aside">
                <NotebookIcon
                  filled={saved}
                  onClick={() => toggleProperty(p.id)}
                  label={saved ? "Remove from notebook" : "Add to notebook"}
                />
                {noteCount > 0 && <span className="result-card__count">{noteCount}</span>}
              </div>
            </article>
          );
        })}
      </div>
    </div>
  );
}
