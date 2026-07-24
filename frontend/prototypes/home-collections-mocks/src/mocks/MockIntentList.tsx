import { useState } from "react";
import type { ShelfProps } from "../App.tsx";
import { PropertyMeta, SectionHead } from "../App.tsx";

/** Cora-style intent rows — expand on tap, no wall of columns. */
export function MockIntentList({ shelves }: ShelfProps) {
  const [openId, setOpenId] = useState<string | null>(shelves[0]?.id ?? null);

  return (
    <section className="mock mock--intent">
      <SectionHead
        kicker="How do you want to search?"
        title="Pick a lens"
        sub="Each row is a curated intent. Expand to peek at three homes."
      />

      <ul className="intent-list">
        {shelves.map((shelf) => {
          const open = openId === shelf.id;
          return (
            <li key={shelf.id} className={`intent-row${open ? " is-open" : ""}`}>
              <button
                type="button"
                className="intent-row__trigger"
                aria-expanded={open}
                onClick={() => setOpenId(open ? null : shelf.id)}
              >
                <span className="intent-row__tag">{shelf.receipt}</span>
                <span className="intent-row__title">{shelf.title}</span>
                <span className="intent-row__quote">{shelf.quote}</span>
                <span className="intent-row__chev" aria-hidden="true">{open ? "−" : "+"}</span>
              </button>
              {open && (
                <div className="intent-row__panel">
                  <p className="intent-row__desc">{shelf.description}</p>
                  <div className="intent-row__cards">
                    {shelf.cards.map((card) => (
                      <a key={card.id} href="#/" className="intent-mini">
                        <img src={card.image} alt="" />
                        <div>
                          <strong>{card.name}</strong>
                          <PropertyMeta p={card} />
                        </div>
                      </a>
                    ))}
                  </div>
                  <button type="button" className="intent-row__search">
                    Run search: {shelf.searchQuery}
                  </button>
                </div>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
