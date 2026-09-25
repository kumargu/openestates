import { useId, useLayoutEffect, useRef, useState } from "react";
import type { Advertisement, InventoryDetail } from "./generated/InventoryDetail.ts";
import { askingPrice, clue, dateLabel, labels, money, physicalFacts } from "./presentation.ts";

function AdvertisementReceipt({ ad, initiallyOpen = false }: { ad: Advertisement; initiallyOpen?: boolean }) {
  const row = ad.observation;
  const seller = labels.seller[row.seller as keyof typeof labels.seller];
  return <details className="inventory-ad" data-observation-id={row.id} open={initiallyOpen || undefined}>
    <summary>
      <span className="inventory-ad__source"><strong>{row.source_label}</strong>
        <span>{row.relation === "registration" ? "Registered sale" : [seller, labels.availability[row.availability]].filter(Boolean).join(" · ")}</span>
      </span>
      <span className="inventory-ad__ask">{row.amount_inr !== null ? money(row.amount_inr) : "Ask not given"}<span aria-hidden="true">⌄</span></span>
    </summary>
    <div className="inventory-receipt">
      {physicalFacts(row).length > 0 && <p>{physicalFacts(row).join(" · ")}</p>}
      <dl>
        {row.amount_inr !== null && <div><dt>{row.relation === "registration" ? "Registered amount" : "Exact ask"}</dt><dd>₹{row.amount_inr.toLocaleString("en-IN")}</dd></div>}
        <div><dt>Observed</dt><dd>{dateLabel(row.observed_on)}</dd></div>
        {row.first_seen && <div><dt>First seen</dt><dd>{dateLabel(row.first_seen)}</dd></div>}
        {row.last_seen && <div><dt>Last seen</dt><dd>{dateLabel(row.last_seen)}</dd></div>}
      </dl>
      {ad.history.length > 0 && <section className="inventory-history" aria-label="Price history for this advertisement">
        <h4>This advertisement’s history</h4>
        <ol>{[row, ...ad.history].map(observation => <li key={observation.id}>
          <time dateTime={observation.observed_on}>{dateLabel(observation.observed_on)}</time>
          <strong>{observation.amount_inr !== null ? money(observation.amount_inr) : "Ask not given"}</strong>
        </li>)}</ol>
      </section>}
      {ad.identity.map(signal => <p key={signal.id} className={signal.disagrees ? "inventory-conflict" : undefined}><strong>{signal.label}.</strong> {signal.value}</p>)}
      {row.source_url && <a href={row.source_url} target="_blank" rel="noreferrer">Open advertisement ↗</a>}
      <details className="inventory-receipt__reference"><summary>Receipt reference</summary>
        <dl><div><dt>Observation</dt><dd>{row.id}</dd></div><div><dt>Advertisement</dt><dd>{row.advertisement_id}</dd></div></dl>
      </details>
    </div>
  </details>;
}

export function InventoryEvidence({ detail }: { detail: InventoryDetail }) {
  const { summary } = detail;
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const layer = useRef<HTMLDivElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const originScroll = useRef(0);
  const [open, setOpen] = useState(false);
  const [question, setQuestion] = useState<"asks" | "identity">("asks");

  function positionLayer() {
    if (!trigger.current || !layer.current) return;
    const rect = trigger.current.getBoundingClientRect();
    const width = Math.min(448, window.innerWidth - 24);
    const left = Math.max(12, Math.min(rect.right - width, window.innerWidth - width - 12));
    const top = Math.max(12, Math.min(rect.bottom + 12, window.innerHeight - 240));
    layer.current.style.setProperty("--evidence-left", `${left}px`);
    layer.current.style.setProperty("--evidence-top", `${top}px`);
    layer.current.style.setProperty("--evidence-height", `${window.innerHeight - top - 16}px`);
  }

  useLayoutEffect(() => {
    const node = layer.current;
    if (!node) return;
    const toggle = (event: Event) => {
      const opened = (event as ToggleEvent).newState === "open";
      setOpen(opened);
      if (opened) { positionLayer(); closeButton.current?.focus({ preventScroll: true }); }
    };
    node.addEventListener("toggle", toggle);
    window.addEventListener("resize", positionLayer);
    return () => { node.removeEventListener("toggle", toggle); window.removeEventListener("resize", positionLayer); };
  }, []);

  function show(nextQuestion: "asks" | "identity") {
    setQuestion(nextQuestion);
    originScroll.current = window.scrollY;
    positionLayer();
    layer.current?.showPopover();
  }
  function close() {
    layer.current?.hidePopover();
    trigger.current?.focus({ preventScroll: true });
    window.scrollTo({ top: originScroll.current, behavior: "instant" });
  }

  const identity = [...new Map([...detail.advertisements, ...detail.candidates].flatMap(ad => ad.identity).map(signal => [JSON.stringify([signal.label, signal.value, signal.disagrees]), signal])).values()];
  const hasDetails = detail.advertisements.length + detail.candidates.length + detail.comparables.length + detail.registrations.length > 0;

  return <div className="inventory-price">
    <span className="inventory-price__label">{summary.ask ? "Asking price" : "No active asking price"}</span>
    {summary.ask && <strong className="inventory-price__amount">{askingPrice(summary.ask)}</strong>}
    {summary.observed_on && <span className="inventory-price__date">Observed {dateLabel(summary.observed_on)}</span>}
    {hasDetails && <button ref={trigger} type="button" className="inventory-price__clue" aria-expanded={open} aria-controls={id}
      onClick={() => open ? close() : show(summary.conflicts.length ? "identity" : "asks")}>
      {clue(summary)}<span aria-hidden="true">↗</span>
    </button>}
    {summary.conflicts.length > 0 && <p className="inventory-price__caveat">{summary.conflicts.map(signal => signal.value).join(" ")}</p>}

    <div ref={layer} id={id} popover="auto" className="inventory-evidence" role="region" aria-label="Asking price details" onKeyDown={event => {
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(); }
    }}>
      <header className="inventory-evidence__header"><h2>{question === "identity" ? "Are these the same home?" : "This home’s asking prices"}</h2>
        <button ref={closeButton} type="button" onClick={close} aria-label="Close asking price details">×</button>
      </header>
      <div className="inventory-evidence__body">
        {question === "identity" && summary.conflicts.map(signal => <p key={signal.id} className="inventory-evidence__answer">{signal.value}</p>)}
        {question === "asks" && summary.ask && summary.ask.min_inr !== summary.ask.max_inr && <p className="inventory-evidence__answer">Different advertisers, different asks. These are not price changes.</p>}
        {question === "asks" && summary.active_count === 0 && <p className="inventory-evidence__answer">Confirm availability before arranging a visit.</p>}
        <div className="inventory-evidence__ads">{detail.advertisements.map(ad => <AdvertisementReceipt key={ad.observation.id} ad={ad} initiallyOpen={summary.price_change?.observation_id === ad.observation.id} />)}</div>
        {detail.candidates.length > 0 && <section className="inventory-candidates" aria-labelledby={`${id}-candidate`}>
          <h3 id={`${id}-candidate`}>Possibly the same home</h3>
          <p>Excluded from this home’s asking price.</p>
          {detail.candidates.map(ad => <AdvertisementReceipt key={ad.observation.id} ad={ad} />)}
        </section>}
        {identity.length > 0 && <details className="inventory-more" open={question === "identity" ? true : undefined}>
          <summary>Why these advertisements belong together<span aria-hidden="true">+</span></summary>
          <ul>{identity.map(signal => <li key={signal.id}><strong>{signal.label}</strong><p>{signal.value}</p></li>)}</ul>
        </details>}
        {detail.comparables.length > 0 && <details className="inventory-more">
          <summary>Other homes in this society<span aria-hidden="true">+</span></summary>
          <p>Separate homes. Advertised asking prices.</p>
          {detail.comparables.map(ad => <div className="inventory-comparable" key={ad.observation.id}>
            <p>{physicalFacts(ad.observation).join(" · ")}</p><AdvertisementReceipt ad={ad} />
          </div>)}
        </details>}
        {detail.registrations.length > 0 && <details className="inventory-more">
          <summary>Registered sales<span aria-hidden="true">+</span></summary>
          <p>Recorded sale amounts for other homes, not asking prices.</p>
          {detail.registrations.map(ad => <div className="inventory-comparable" key={ad.observation.id}>
            <p>{dateLabel(ad.observation.observed_on)} · {physicalFacts(ad.observation).join(" · ")}</p><AdvertisementReceipt ad={ad} />
          </div>)}
        </details>}
      </div>
      <footer className="inventory-evidence__footer">Example observations · design preview</footer>
    </div>
  </div>;
}
