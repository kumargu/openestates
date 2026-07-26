import { useState } from "react";
import type { ShelfProps } from "../App.tsx";
import { PropertyMeta, SectionHead } from "../App.tsx";

/** Magazine editorial — one hero home, minimal shelf switcher. */
export function MockEditorial({ shelves }: ShelfProps) {
  const [activeId, setActiveId] = useState(shelves[0]?.id ?? "");
  const active = shelves.find((s) => s.id === activeId) ?? shelves[0];
  const featured = active.cards[0];
  const alternates = active.cards.slice(1);

  return (
    <section className="mock mock--editorial">
      <SectionHead
        kicker="Editorial pick"
        title={active.title}
        sub={active.quote}
      />

      <nav className="editorial-nav" aria-label="Collections">
        {shelves.map((shelf) => (
          <button
            key={shelf.id}
            type="button"
            className={shelf.id === active.id ? "is-active" : ""}
            onClick={() => setActiveId(shelf.id)}
          >
            {shelf.title}
          </button>
        ))}
      </nav>

      <div key={active.id} className="editorial-hero">
        <a href="#/" className="editorial-hero__visual">
          <img src={featured.image} alt={featured.name} />
          <div className="editorial-hero__overlay">
            <span className="editorial-hero__reason">{featured.reason}</span>
            <h3>{featured.name}</h3>
            <PropertyMeta p={featured} />
          </div>
        </a>
        <aside className="editorial-aside">
          <p>{active.description}</p>
          <span className="editorial-aside__tag">{active.receipt}</span>
          <div className="editorial-alts">
            {alternates.map((card) => (
              <a key={card.id} href="#/" className="editorial-alt">
                <img src={card.image} alt="" />
                <div>
                  <strong>{card.name}</strong>
                  <small>{card.reason}</small>
                </div>
              </a>
            ))}
          </div>
          <button type="button" className="editorial-cta">View all in collection</button>
        </aside>
      </div>
    </section>
  );
}
