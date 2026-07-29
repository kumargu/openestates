import { useCallback, useEffect, useRef, useState } from "react";
import {
  CATALOG,
  PROPERTIES,
  RERA_COMPLAINT_BODY,
  formatCr,
  propertyById,
} from "../data.ts";
import { useNotebook } from "../store.tsx";
import { NotebookIcon } from "../components/Ui.tsx";
import { CrossLinks, PinableRow, type PageNav } from "../components/Pinable.tsx";

const SECTION_ORDER: {
  id: string;
  title: string;
  read: string;
  kinds: string[];
  tags: string[];
}[] = [
  {
    id: "schools",
    title: "Schools",
    read: "Matched your search for nearby schools.",
    kinds: ["theme"],
    tags: ["schools"],
  },
  {
    id: "water",
    title: "Water",
    read: "Operating evidence from resident reviews.",
    kinds: ["fact"],
    tags: ["water"],
  },
  {
    id: "rera",
    title: "RERA",
    read: "Official project registration and delivery record.",
    kinds: ["fact"],
    tags: ["legal"],
  },
  {
    id: "price",
    title: "Price proof",
    read: "Where asking sits vs nearby evidence.",
    kinds: ["fact"],
    tags: ["price"],
  },
  {
    id: "layout",
    title: "Layout",
    read: "Plan themes worth remembering.",
    kinds: ["theme"],
    tags: ["layout"],
  },
  {
    id: "surroundings",
    title: "Around this home",
    read: "Map layers that matter for this home.",
    kinds: ["fact", "theme"],
    tags: ["commute", "open-space"],
  },
];

export function RealisticPropertyPage({ nav }: { nav: PageNav }) {
  const {
    focusedId,
    setFocusedId,
    isPropertyInNotebook,
    toggleProperty,
    notes,
    isCatalogSaved,
    compareIds,
    addSelectionNote,
  } = useNotebook();
  const [openId, setOpenId] = useState<string>("rera");
  const property = propertyById(focusedId);
  const facts = CATALOG.filter((c) => c.propertyId === focusedId);
  const savedCount = notes.filter((n) => n.propertyId === focusedId).length;

  return (
    <div className="oe-page oe-property">
      <div className="oe-property__toolbar">
        <button type="button" className="oe-back" onClick={nav.onOpenNotebook}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <polyline points="15 18 9 12 15 6" />
          </svg>
          Notebook
        </button>
        <div className="oe-property__toolbar-right">
          <CrossLinks nav={nav} showCompare={compareIds.length >= 2} />
          <NotebookIcon
            filled={isPropertyInNotebook(property.id)}
            onClick={() => toggleProperty(property.id)}
            label="Toggle property in notebook"
          />
        </div>
      </div>

      <div className="oe-home-switch">
        {PROPERTIES.map((p) => (
          <button
            key={p.id}
            type="button"
            className={`oe-home-chip${p.id === focusedId ? " is-active" : ""}`}
            onClick={() => setFocusedId(p.id)}
          >
            {p.short}
            {isPropertyInNotebook(p.id) && <i />}
          </button>
        ))}
      </div>

      <section className="oe-brief-hero">
        <div className="oe-scene" aria-hidden>
          <div className="oe-scene__wash" />
          <span>{property.icon}</span>
          <p>{property.area}</p>
        </div>
        <div className="oe-brief-copy">
          <h1>{property.name}</h1>
          <p className="oe-brief-location">
            {property.name} · {property.area}, Bengaluru
          </p>
          <div className="oe-brief-price">
            <strong>{formatCr(property.priceCr)}</strong>
            <span>₹13,700 /sqft</span>
          </div>
          <div className="oe-proof-strip">
            <span className="oe-chip">Seller + RERA</span>
            <span className="oe-chip oe-chip--pos">RERA verified</span>
            <span className="oe-chip">Builder delivered</span>
          </div>
          <div className="oe-brief-tags">
            <span>{property.bhk}</span>
            <span>{property.sqft.toLocaleString("en-IN")} sqft carpet</span>
            <span>Ready</span>
            <span>East facing</span>
          </div>
          {savedCount > 0 && (
            <p className="oe-saved-line">{savedCount} items remembered for this home</p>
          )}
          <p className="oe-hint-line">
            Hover a fact to pin. On RERA complaints, select text → Remember. Handwritten notes live
            only in Notebook.
          </p>
        </div>
      </section>

      <aside className="oe-sticky-facts" aria-label="Property snapshot">
        <div>
          <strong>{property.short}</strong>
          <span>{property.area}</span>
        </div>
        <dl>
          <div>
            <dt>Price</dt>
            <dd>{formatCr(property.priceCr)}</dd>
          </div>
          <div>
            <dt>Carpet</dt>
            <dd>{property.sqft.toLocaleString("en-IN")} sqft</dd>
          </div>
          <div>
            <dt>Home</dt>
            <dd>{property.bhk}</dd>
          </div>
        </dl>
      </aside>

      <div className="oe-evidence-stack">
        {SECTION_ORDER.map((section) => {
          const items = facts.filter(
            (f) => section.tags.includes(f.tag) && section.kinds.includes(f.kind),
          );
          if (!items.length && !(section.id === "rera" && focusedId === "waterford")) {
            return null;
          }
          const open = openId === section.id;
          const pinned = items.filter((i) => isCatalogSaved(i.id)).length;
          return (
            <section
              key={section.id}
              className={`oe-fold oe-fold--${section.id}${open ? " is-open" : ""}`}
            >
              <button
                type="button"
                className="oe-fold__head"
                onClick={() => setOpenId(open ? "" : section.id)}
              >
                <span className="oe-fold__icon" aria-hidden>
                  {section.id === "rera"
                    ? "⚖"
                    : section.id === "schools"
                      ? "🏫"
                      : section.id === "water"
                        ? "💧"
                        : "·"}
                </span>
                <span className="oe-fold__headings">
                  <span className="oe-fold__title">{section.title}</span>
                  <span className="oe-fold__read">{section.read}</span>
                </span>
                <span className="oe-fold__meta">
                  {pinned > 0 && <em>{pinned} pinned</em>}
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <polyline points="6 9 12 15 18 9" />
                  </svg>
                </span>
              </button>
              {open && (
                <div className="oe-fold__body">
                  {section.id === "rera" && (
                    <>
                      <div className="oe-rera-grid">
                        <div>
                          <h3>Registration</h3>
                          <p>PRM/KA/RERA/1251/446/PR/171015/000456</p>
                        </div>
                        <div>
                          <h3>Promoter</h3>
                          <p>Prestige Estates Projects Ltd</p>
                        </div>
                      </div>
                      {focusedId === "waterford" && (
                        <SelectableComplaintBlock
                          propertyId={focusedId}
                          onRemember={(text) =>
                            addSelectionNote({
                              propertyId: focusedId,
                              text,
                              tag: "legal",
                              source: "RERA complaints",
                              mark: "concern",
                            })
                          }
                        />
                      )}
                    </>
                  )}
                  <div className="oe-pin-list">
                    {items.map((fact) => (
                      <PinableRow key={fact.id} fact={fact} />
                    ))}
                  </div>
                </div>
              )}
            </section>
          );
        })}
      </div>
    </div>
  );
}

function SelectableComplaintBlock({
  propertyId,
  onRemember,
}: {
  propertyId: string;
  onRemember: (text: string) => void;
}) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const [toolbar, setToolbar] = useState<{
    top: number;
    left: number;
    text: string;
  } | null>(null);

  const clearToolbar = useCallback(() => setToolbar(null), []);

  useEffect(() => {
    const onSel = () => {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed || !bodyRef.current) {
        setToolbar(null);
        return;
      }
      if (!bodyRef.current.contains(sel.anchorNode)) {
        setToolbar(null);
        return;
      }
      const text = sel.toString().trim();
      if (text.length < 8) {
        setToolbar(null);
        return;
      }
      const range = sel.getRangeAt(0);
      const rect = range.getBoundingClientRect();
      const host = bodyRef.current.getBoundingClientRect();
      setToolbar({
        top: rect.top - host.top - 44,
        left: Math.min(Math.max(8, rect.left - host.left), host.width - 140),
        text,
      });
    };
    document.addEventListener("selectionchange", onSel);
    return () => document.removeEventListener("selectionchange", onSel);
  }, [propertyId]);

  return (
    <div className="oe-complaint-block" ref={bodyRef}>
      <h3>Complaints on record</h3>
      <p className="oe-complaint-hint">Select any passage → Remember (Notion / Readwise style).</p>
      <div className="oe-complaint-body">{RERA_COMPLAINT_BODY}</div>
      {toolbar && (
        <div
          className="oe-select-toolbar"
          style={{ top: toolbar.top, left: toolbar.left }}
          role="toolbar"
        >
          <button
            type="button"
            onClick={() => {
              onRemember(toolbar.text);
              window.getSelection()?.removeAllRanges();
              clearToolbar();
            }}
          >
            Remember
          </button>
          <span>Legal</span>
        </div>
      )}
    </div>
  );
}
