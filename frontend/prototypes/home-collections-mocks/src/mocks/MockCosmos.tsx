import { useState } from "react";
import type { ShelfProps } from "../App.tsx";
import { PropertyMeta, SectionHead } from "../App.tsx";

/** Cosmos-inspired: warm canvas, one cluster active, asymmetric image grid. */
export function MockCosmos({ shelves }: ShelfProps) {
  const [activeId, setActiveId] = useState(shelves[0]?.id ?? "");
  const active = shelves.find((s) => s.id === activeId) ?? shelves[0];
  const [hero, ...rest] = active.cards;

  return (
    <section className="mock mock--cosmos">
      <SectionHead
        kicker="Collections"
        title="Curated by intent"
        sub="One lens at a time — images lead, proof stays on the card."
      />

      <div className="cosmos-tabs" role="tablist">
        {shelves.map((shelf) => (
          <button
            key={shelf.id}
            type="button"
            role="tab"
            aria-selected={shelf.id === active.id}
            className={`cosmos-tab${shelf.id === active.id ? " is-active" : ""}`}
            onClick={() => setActiveId(shelf.id)}
          >
            {shelf.title}
          </button>
        ))}
      </div>

      <div key={active.id} className="cosmos-stage">
        <div className="cosmos-copy">
          <span className="cosmos-copy__tag">{active.receipt}</span>
          <p className="cosmos-copy__quote">{active.quote}</p>
          <p className="cosmos-copy__desc">{active.description}</p>
          <button type="button" className="cosmos-copy__cta">
            See all in this collection →
          </button>
        </div>

        <div className="cosmos-grid">
          <a href="#/" className="cosmos-card cosmos-card--hero">
            <img src={hero.image} alt={hero.name} />
            <div className="cosmos-card__shade" />
            <div className="cosmos-card__cap">
              <strong>{hero.name}</strong>
              <PropertyMeta p={hero} />
              <span className="cosmos-card__reason">{hero.reason}</span>
            </div>
          </a>
          {rest.map((card) => (
            <a key={card.id} href="#/" className="cosmos-card">
              <img src={card.image} alt={card.name} />
              <div className="cosmos-card__shade" />
              <div className="cosmos-card__cap">
                <strong>{card.name}</strong>
                <span className="cosmos-card__reason">{card.reason}</span>
              </div>
            </a>
          ))}
        </div>
      </div>
    </section>
  );
}
