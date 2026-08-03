import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import type { PropertyCard } from "../../lib/types.ts";

type SavedHomesDockProps = {
  homes: PropertyCard[];
};

function homeName(home: PropertyCard): string {
  return home.society_name?.trim() || home.title;
}

export function SavedHomesDock({ homes }: SavedHomesDockProps) {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open) return undefined;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", closeOnEscape);
    return () => document.removeEventListener("keydown", closeOnEscape);
  }, [open]);

  return (
    <div className={`saved-homes-dock${open ? " is-open" : ""}`}>
      <button
        type="button"
        className="saved-homes-dock__trigger"
        aria-expanded={open}
        aria-controls="saved-homes-panel"
        onClick={() => setOpen((current) => !current)}
      >
        Saved homes <span>{homes.length}</span>
      </button>
      {open ? (
        <section id="saved-homes-panel" className="saved-homes-dock__panel" aria-label="Saved homes">
          <header>
            <div>
              <strong>Saved homes</strong>
              <span>Continue where you left off</span>
            </div>
            <button type="button" aria-label="Close saved homes" onClick={() => setOpen(false)}>×</button>
          </header>
          <div className="saved-homes-dock__list">
            {homes.slice(0, 4).map((home) => (
              <Link key={home.id} to={`/property/${encodeURIComponent(home.id)}`}>
                <strong>{homeName(home)}</strong>
                <span>{home.area} · {home.bhk}BHK</span>
              </Link>
            ))}
          </div>
          <footer>
            <Link to="/workspace">Open workspace</Link>
          </footer>
        </section>
      ) : null}
    </div>
  );
}
