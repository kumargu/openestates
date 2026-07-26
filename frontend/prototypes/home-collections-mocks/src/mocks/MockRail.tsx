import { useState } from "react";
import type { ShelfProps } from "../App.tsx";
import { PropertyMeta, SectionHead } from "../App.tsx";

/** Airbnb-style horizontal rail per collection. */
export function MockRail({ shelves }: ShelfProps) {
  const [activeId, setActiveId] = useState(shelves[0]?.id ?? "");
  const active = shelves.find((s) => s.id === activeId) ?? shelves[0];

  return (
    <section className="mock mock--rail">
      <SectionHead
        kicker="Browse"
        title="Start with an intent"
        sub="Swipe through homes — one proof line per card."
      />

      <div className="rail-tabs">
        {shelves.map((shelf) => (
          <button
            key={shelf.id}
            type="button"
            className={`rail-tab${shelf.id === active.id ? " is-active" : ""}`}
            onClick={() => setActiveId(shelf.id)}
          >
            {shelf.title}
          </button>
        ))}
      </div>

      <div key={active.id} className="rail-band">
        <div className="rail-band__intro">
          <h3>{active.title}</h3>
          <p>{active.quote}</p>
        </div>
        <div className="rail-scroll">
          {active.cards.map((card) => (
            <a key={card.id} href="#/" className="rail-card">
              <div className="rail-card__img">
                <img src={card.image} alt={card.name} />
              </div>
              <div className="rail-card__body">
                <strong>{card.name}</strong>
                <PropertyMeta p={card} />
                <span>{card.reason}</span>
              </div>
            </a>
          ))}
          <button type="button" className="rail-card rail-card--more">
            <span>Search collection</span>
            <small>{active.searchQuery}</small>
          </button>
        </div>
      </div>
    </section>
  );
}
