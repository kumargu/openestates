import { PLAN_PINS, PROPERTIES, formatCr, propertyById } from "../data.ts";
import { useNotebook } from "../store.tsx";
import { CrossLinks, PinableRow, type PageNav } from "../components/Pinable.tsx";

export function RealisticPlanPage({ nav }: { nav: PageNav }) {
  const { focusedId, setFocusedId, isPropertyInNotebook, compareIds } = useNotebook();
  const property = propertyById(focusedId);
  const buyerPins = PLAN_PINS.filter((p) => p.id.startsWith("money-"));
  const homePins = PLAN_PINS.filter((p) => p.propertyId === focusedId && !p.id.startsWith("money-"));

  return (
    <div className="oe-page oe-plan">
      <header className="oe-plan-hero">
        <div className="oe-plan-hero__top">
          <div>
            <p className="oe-eyebrow">Buy vs rent</p>
            <h1>{property.name}</h1>
            <p className="oe-plan-sub">
              {property.area} · {formatCr(property.priceCr)} · hover a money row to pin · no
              handwritten here
            </p>
          </div>
          <CrossLinks
            nav={{ ...nav, onOpenProperty: nav.onOpenProperty ?? (() => undefined) }}
            showCompare={compareIds.length >= 2}
          />
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
      </header>

      <div className="oe-plan-layout">
        <section className="oe-plan-rail">
          <h2>Household</h2>
          <p className="oe-plan-rail__hint">Entered once. Hover the notebook icon to remember.</p>
          <div className="oe-pin-list">
            {buyerPins.map((pin) => (
              <PinableRow key={pin.id} fact={pin} />
            ))}
          </div>

          <div className="oe-plan-sliders" aria-hidden>
            <label>
              <span>Monthly rent</span>
              <div>
                <b>₹</b>
                <input readOnly value={48} />
                <b>K / mo</b>
              </div>
            </label>
            <label>
              <span>Comfortable EMI</span>
              <div>
                <b>₹</b>
                <input readOnly value={135} />
                <b>K / mo</b>
              </div>
            </label>
          </div>
        </section>

        <section className="oe-plan-main">
          <div className="oe-plan-verdict">
            <p className="oe-eyebrow">At year 10</p>
            <h2>
              {focusedId === "dream-acres"
                ? "Buying stays ahead on buffer"
                : "Buying leads — watch the down-payment gap"}
            </h2>
            <p>
              {focusedId === "waterford"
                ? "Waterford needs ~₹62 L upfront vs your ₹58 L. Pin the gap if it should show in Compare."
                : focusedId === "dream-acres"
                  ? "Dream Acres leaves ~₹14 L buffer against your ₹58 L down payment."
                  : "Park Retreat sits near your comfort line — pin EMI if it matters."}
            </p>
          </div>

          <div className="oe-plan-chart" aria-hidden>
            <div className="oe-plan-chart__buy" />
            <div className="oe-plan-chart__rent" />
            <span>Buy</span>
            <span>Rent + SIP</span>
          </div>

          <h2 className="oe-plan-section-title">{property.short} · pinable outputs</h2>
          <div className="oe-pin-list">
            {homePins.length > 0 ? (
              homePins.map((pin) => <PinableRow key={pin.id} fact={pin} />)
            ) : (
              <p className="oe-plan-empty">
                No derived gap in this mock for {property.short}. Switch to Waterford or Dream Acres.
              </p>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
