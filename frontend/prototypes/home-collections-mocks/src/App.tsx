import { useState } from "react";
import { formatPrice, SHELVES, type MockProperty, type MockShelf } from "./data.ts";
import { MockBaseline } from "./mocks/MockBaseline.tsx";
import { MockCosmos } from "./mocks/MockCosmos.tsx";
import { MockRail } from "./mocks/MockRail.tsx";
import { MockIntentList } from "./mocks/MockIntentList.tsx";
import { MockEditorial } from "./mocks/MockEditorial.tsx";

export type MockId = "baseline" | "cosmos" | "rail" | "intent" | "editorial";

const DIRECTIONS: { id: MockId; label: string; note: string }[] = [
  { id: "baseline", label: "A · Current", note: "Four columns, dense text — hard to scan" },
  { id: "cosmos", label: "B · Cosmos clusters", note: "One collection in focus, image-led, tabs" },
  { id: "rail", label: "C · Horizontal rail", note: "Airbnb-style scroll, minimal copy on cards" },
  { id: "intent", label: "D · Intent list", note: "Cora-like rows — tap to reveal homes" },
  { id: "editorial", label: "E · Editorial hero", note: "One featured home per shelf, magazine feel" },
];

export function App() {
  const [active, setActive] = useState<MockId>("cosmos");
  const current = DIRECTIONS.find((d) => d.id === active)!;

  return (
    <div className="gallery">
      <header className="gallery__chrome">
        <div className="gallery__brand">
          <strong>80feet</strong>
          <span>Collections UI mocks</span>
        </div>
        <nav className="gallery__nav" aria-label="Mock directions">
          {DIRECTIONS.map((d) => (
            <button
              key={d.id}
              type="button"
              className={`gallery__nav-btn${active === d.id ? " is-active" : ""}`}
              onClick={() => setActive(d.id)}
            >
              {d.label}
            </button>
          ))}
        </nav>
        <p className="gallery__note">{current.note}</p>
      </header>

      <main className="gallery__stage">
        {active === "baseline" && <MockBaseline shelves={SHELVES} />}
        {active === "cosmos" && <MockCosmos shelves={SHELVES} />}
        {active === "rail" && <MockRail shelves={SHELVES} />}
        {active === "intent" && <MockIntentList shelves={SHELVES} />}
        {active === "editorial" && <MockEditorial shelves={SHELVES} />}
      </main>

      <footer className="gallery__footer">
        <p>
          Inspired by{" "}
          <a href="https://cosmos.so" target="_blank" rel="noreferrer">Cosmos</a>
          {" "}(clusters, image-first), Airbnb rails, Cora intent rows.
          Pick a direction before shipping to HomePage.
        </p>
      </footer>
    </div>
  );
}

export function PropertyMeta({ p }: { p: MockProperty }) {
  return (
    <span className="property-meta">
      {p.area} · {p.bhk} BHK · {formatPrice(p.priceL)}
    </span>
  );
}

export function SectionHead({ kicker, title, sub }: { kicker: string; title: string; sub?: string }) {
  return (
    <div className="section-head">
      <span className="section-head__kicker">{kicker}</span>
      <h2 className="section-head__title">{title}</h2>
      {sub && <p className="section-head__sub">{sub}</p>}
    </div>
  );
}

export type ShelfProps = {
  shelves: MockShelf[];
  onSearch?: (q: string) => void;
};
