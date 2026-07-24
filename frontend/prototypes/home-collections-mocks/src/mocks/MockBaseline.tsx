import type { ShelfProps } from "../App.tsx";
import { PropertyMeta, SectionHead } from "../App.tsx";

/** Current production layout — dense 4-column grid. */
export function MockBaseline({ shelves }: ShelfProps) {
  return (
    <section className="mock mock--baseline">
      <SectionHead
        kicker="Discovery shelves"
        title="Curated by intent"
      />
      <div className="baseline-grid">
        {shelves.map((shelf) => (
          <article key={shelf.id} className="baseline-shelf">
            <span className="baseline-shelf__tag">{shelf.receipt}</span>
            <h3>{shelf.title}</h3>
            <p className="baseline-shelf__quote">{shelf.quote}</p>
            <p className="baseline-shelf__desc">{shelf.description}</p>
            <div className="baseline-shelf__cards">
              {shelf.cards.map((card) => (
                <div key={card.id} className="baseline-card">
                  <strong>{card.name}</strong>
                  <PropertyMeta p={card} />
                  <em>{card.reason}</em>
                </div>
              ))}
            </div>
            <button type="button" className="baseline-shelf__cta">Search this shelf</button>
          </article>
        ))}
      </div>
    </section>
  );
}
