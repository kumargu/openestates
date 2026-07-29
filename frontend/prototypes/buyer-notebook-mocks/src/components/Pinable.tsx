import type { CatalogFact } from "../data.ts";
import { tagById } from "../data.ts";
import { useNotebook } from "../store.tsx";
import { NotebookIcon } from "./Ui.tsx";

/** Quiet pin row — hover reveals notebook icon (AllTrails-style). No handwritten. */
export function PinableRow({ fact }: { fact: CatalogFact }) {
  const { isCatalogSaved, toggleCatalog } = useNotebook();
  const saved = isCatalogSaved(fact.id);
  const tag = tagById(fact.tag);

  return (
    <div className={`pin-row${saved ? " is-saved" : ""}`}>
      <div className="pin-row__body">
        <strong>{fact.label}</strong>
        <span>
          {fact.detail}
          {fact.source ? ` · ${fact.source}` : ""}
        </span>
      </div>
      <span className="pin-row__tag" style={{ background: tag.color, color: tag.ink }}>
        {tag.label}
      </span>
      <div className={`pin-row__action${saved ? " is-visible" : ""}`}>
        <NotebookIcon
          filled={saved}
          size="sm"
          label={saved ? "Remove from notebook" : "Add to notebook"}
          onClick={() => toggleCatalog(fact)}
        />
      </div>
    </div>
  );
}

export type PageNav = {
  onOpenNotebook: () => void;
  onOpenCompare: () => void;
  onOpenPlan?: () => void;
  onOpenProperty?: () => void;
};

export function CrossLinks({
  nav,
  showCompare,
}: {
  nav: PageNav;
  showCompare: boolean;
}) {
  return (
    <div className="oe-cross-links">
      <button type="button" className="oe-cross-link" onClick={nav.onOpenNotebook}>
        Notebook
      </button>
      {showCompare && (
        <button type="button" className="oe-cross-link oe-cross-link--accent" onClick={nav.onOpenCompare}>
          Compare
        </button>
      )}
      {nav.onOpenPlan && (
        <button type="button" className="oe-cross-link" onClick={nav.onOpenPlan}>
          Plan
        </button>
      )}
      {nav.onOpenProperty && (
        <button type="button" className="oe-cross-link" onClick={nav.onOpenProperty}>
          Property
        </button>
      )}
    </div>
  );
}
