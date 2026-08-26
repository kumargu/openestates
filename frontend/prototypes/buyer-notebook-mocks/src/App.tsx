import { useState } from "react";
import { PROPERTIES, propertyById, type PropertyId } from "./data.ts";
import { useNotebook } from "./store.tsx";
import { DiscoverPage } from "./pages/DiscoverProperty.tsx";
import { RealisticPropertyPage } from "./pages/RealisticPropertyPage.tsx";
import { RealisticPlanPage } from "./pages/RealisticPlanPage.tsx";
import { NotebookPage } from "./pages/NotebookPage.tsx";
import { ComparePage } from "./pages/ComparePage.tsx";
import type { PageNav } from "./components/Pinable.tsx";

export type PageId = "discover" | "property" | "plan" | "notebook" | "compare";

const PAGES: { id: PageId; label: string; hint: string }[] = [
  { id: "discover", label: "1 · Discover", hint: "Save a home" },
  { id: "property", label: "2 · Property", hint: "Hover pin / select text" },
  { id: "plan", label: "3 · Plan", hint: "Pin money rows" },
  { id: "notebook", label: "4 · Notebook", hint: "Handwritten + select" },
  { id: "compare", label: "5 · Compare", hint: "Tag rows" },
];

export function App() {
  const [page, setPage] = useState<PageId>("discover");
  const {
    propertyIds,
    compareIds,
    notes,
    pulse,
    toast,
    focusedId,
    setFocusedId,
    seedDemo,
    resetAll,
  } = useNotebook();

  const openProperty = (id: PropertyId) => {
    setFocusedId(id);
    setPage("property");
  };

  const nav: PageNav = {
    onOpenNotebook: () => setPage("notebook"),
    onOpenCompare: () => setPage("compare"),
    onOpenPlan: () => setPage("plan"),
    onOpenProperty: () => setPage("property"),
  };

  return (
    <div className="shell">
      <header className="chrome">
        <div className="chrome__brand">
          <strong>80feet</strong>
          <span>Buyer notebook · connected mocks</span>
        </div>
        <nav className="chrome__nav" aria-label="Journey pages">
          {PAGES.map((p) => (
            <button
              key={p.id}
              type="button"
              className={`chrome__nav-btn${page === p.id ? " is-active" : ""}`}
              onClick={() => setPage(p.id)}
            >
              <span>{p.label}</span>
              <small>{p.hint}</small>
            </button>
          ))}
        </nav>
        <div className="chrome__actions">
          <button type="button" className="ghost-btn" onClick={seedDemo}>
            Load demo
          </button>
          <button type="button" className="ghost-btn" onClick={resetAll}>
            Reset
          </button>
        </div>
      </header>

      <div className="workspace">
        <aside className={`sidebar${pulse ? " is-pulse" : ""}`}>
          <button
            type="button"
            className={`side-link${page === "discover" ? " is-active" : ""}`}
            onClick={() => setPage("discover")}
          >
            Discover
          </button>
          <button
            type="button"
            className={`side-link${page === "property" ? " is-active" : ""}`}
            onClick={() => setPage("property")}
          >
            Property
          </button>
          <button
            type="button"
            className={`side-link side-link--notebook${page === "notebook" ? " is-active" : ""}`}
            onClick={() => setPage("notebook")}
          >
            <span>Notebook</span>
            <em className={pulse ? "pop" : ""}>{propertyIds.length}</em>
          </button>
          <ul className="side-homes">
            {propertyIds.map((id) => {
              const p = propertyById(id);
              const count = notes.filter((n) => n.propertyId === id).length;
              return (
                <li key={id}>
                  <button
                    type="button"
                    className={`side-home${focusedId === id ? " is-focus" : ""}`}
                    onClick={() => {
                      setFocusedId(id);
                      setPage("property");
                    }}
                  >
                    <span>{p.short}</span>
                    <small>{count}</small>
                  </button>
                </li>
              );
            })}
          </ul>
          <button
            type="button"
            className={`side-link${page === "plan" ? " is-active" : ""}`}
            onClick={() => setPage("plan")}
          >
            Plan
          </button>
          <button
            type="button"
            className={`side-link${page === "compare" ? " is-active" : ""}`}
            onClick={() => setPage("compare")}
            disabled={compareIds.length < 2 && propertyIds.length < 2}
          >
            Compare
            {compareIds.length >= 2 && <em>{compareIds.length}</em>}
          </button>
          <p className="side-footnote">
            Pins auto-carry from Property/Plan. Handwritten only in Notebook. Compare joins on tags.
          </p>
        </aside>

        <main className="stage" key={page}>
          {page === "discover" && <DiscoverPage onOpenProperty={openProperty} />}
          {page === "property" && <RealisticPropertyPage nav={nav} />}
          {page === "plan" && <RealisticPlanPage nav={nav} />}
          {page === "notebook" && (
            <NotebookPage
              onOpenCompare={() => setPage("compare")}
              onOpenProperty={openProperty}
              onOpenPlan={() => setPage("plan")}
            />
          )}
          {page === "compare" && (
            <ComparePage nav={nav} onOpenProperty={openProperty} />
          )}
        </main>
      </div>

      {toast && (
        <div className="toast" key={toast.id}>
          <span>{toast.text}</span>
          {toast.undo && (
            <button type="button" onClick={toast.undo}>
              Undo
            </button>
          )}
        </div>
      )}

      <footer className="chrome-foot">
        <p>
          Locked rules: hover-pin on Property/Plan · select-text Remember on RERA · handwritten only
          in Notebook · Compare by tags. Map marker save deferred.
        </p>
        <p className="chrome-foot__homes">
          Cast: {PROPERTIES.map((p) => p.short).join(" · ")}
        </p>
      </footer>
    </div>
  );
}
