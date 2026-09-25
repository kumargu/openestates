import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";
import { BrandMark } from "../../src/components/brand/BrandMark.tsx";
import { PropertySceneCard } from "../../src/components/property/PropertySceneCard.tsx";
import { PropertySearchStrip } from "../../src/components/property/PropertySearchRail.tsx";
import type { PropertyStoryModel } from "../../src/lib/propertyStory.ts";
import type { PropertySearchContext } from "../../src/lib/navigationContext.ts";
import { PUBLIC_BRAND_NAME } from "../../src/lib/brand.ts";
import { getCatalog, getDetail } from "./api.ts";
import type { InventoryCatalog } from "./generated/InventoryCatalog.ts";
import type { HomeSummary, InventoryDetail } from "./generated/InventoryDetail.ts";
import { InventoryEvidence } from "./InventoryEvidence.tsx";
import { askingPrice, clue, physicalFacts } from "./presentation.ts";

const QUERY = "3 BHK in Whitefield near Manipal Hospital";
const SAVE_KEY = "openestates:inventory-design-preview:saved:v1";
function readSaved(): string[] {
  try { const value: unknown = JSON.parse(localStorage.getItem(SAVE_KEY) ?? "[]"); return Array.isArray(value) ? value.filter((id): id is string => typeof id === "string") : []; }
  catch { return []; }
}

function SaveButton({ saved, onToggle }: { saved: boolean; onToggle: () => void }) {
  return <button type="button" className="inventory-save" aria-pressed={saved} onClick={onToggle}>
    <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true"><path d="M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.7l-1.1-1.1a5.5 5.5 0 0 0-7.8 7.8L12 21l8.8-8.6a5.5 5.5 0 0 0 0-7.8Z" fill={saved ? "currentColor" : "none"} stroke="currentColor" strokeWidth="1.5" /></svg>
    {saved ? "Saved" : "Save home"}
  </button>;
}

function HomeTile({ summary, href, saved, onSave }: { summary: HomeSummary; href: string; saved: boolean; onSave: () => void }) {
  const home = summary.home;
  return <article className="catalog-card catalog-card--browse inventory-tile">
    <div className="catalog-card__media"><img className="catalog-card__image" src={home.image} alt="" /><div className="catalog-card__actions"><SaveButton saved={saved} onToggle={onSave} /></div></div>
    <Link className="catalog-card__link" to={href}>
      <div className="catalog-card__caption"><h2 className="catalog-card__title">{home.title}</h2>
        <p className="catalog-card__meta">{physicalFacts(home).join(" · ")}</p>
        <strong className="catalog-card__price">{askingPrice(summary.ask)}</strong>
        <p className="inventory-tile__clue">{clue(summary)} <span aria-hidden="true">↗</span></p>
      </div>
    </Link>
  </article>;
}

function HomeView({ summary, catalog, saved, onSave, onRefreshCatalog }: { summary: HomeSummary; catalog: InventoryCatalog; saved: boolean; onSave: () => void; onRefreshCatalog: () => void }) {
  const [detail, setDetail] = useState<InventoryDetail>();
  const [error, setError] = useState<string>();
  const [retry, setRetry] = useState(0);
  const home = summary.home;
  useEffect(() => {
    const controller = new AbortController();
    getDetail(home.id, catalog.snapshot_id, controller.signal).then(setDetail).catch((cause: unknown) => {
      if (!controller.signal.aborted) setError(cause instanceof Error ? cause.message : "Asking prices couldn’t be loaded.");
    });
    return () => controller.abort();
  }, [home.id, catalog.snapshot_id, retry]);
  const image = new URL(home.image, window.location.origin).href;
  const story: PropertyStoryModel = {
    identity: { propertyId: home.id, title: home.title, location: home.location, facts: physicalFacts(home).map((value, index) => ({ key: String(index), value })) },
    media: { frames: [{ id: "society-exterior", url: image, role: "exterior", sourceType: "fixture", lifecycle: "unknown" }], galleryUrls: [image] },
    map: { available: false }, reviews: { state: "missing" }, recordCards: [],
    coverage: { level: "sparse", availableDecks: 1, totalDecks: 1 }, motionSeed: 0, motionTheme: "still", decks: [],
  };
  const context: PropertySearchContext = {
    version: 1, id: "inventory-preview", queryFingerprint: "q148", queryLabel: QUERY,
    returnUrl: `/?scenario=${home.id}`, returnScrollY: 0, createdAt: 0, selectedId: home.id,
    runtimeVersion: { scoringPolicyVersion: 1, searchEngineVersion: "design-preview", semanticContractDigest: "design-preview", servingBundleVersion: catalog.snapshot_id, snapshotIdentity: catalog.snapshot_id },
    results: [{ propertyId: home.id, title: home.title, societyName: home.title, area: home.location, bhk: home.bhk }],
  };
  return <>
    <div className="inventory-context"><Link to={`/?scenario=${home.id}`} className="inventory-back">← Results</Link><PropertySearchStrip context={context} /></div>
    <div className="inventory-home-actions"><SaveButton saved={saved} onToggle={onSave} /></div>
    <PropertySceneCard story={story} sectionId="property-cover" identityPlacement="above" cinematicMotion={false} actions={
      detail ? <InventoryEvidence detail={detail} /> : error ? <div className="inventory-error" role="alert"><p>{error}</p><button type="button" onClick={() => { setError(undefined); setRetry(value => value + 1); }}>Try again</button><Link to={`/?scenario=${home.id}`} onClick={onRefreshCatalog}>Return to results</Link></div>
        : <div className="inventory-price inventory-price--loading" role="status">Loading asking prices…</div>
    } />
    <div className="inventory-afterword"><Link to={`/?scenario=${home.id}`}>Continue your search <span aria-hidden="true">→</span></Link></div>
  </>;
}

export function Preview() {
  const location = useLocation();
  const navigate = useNavigate();
  const { id } = useParams();
  const [catalog, setCatalog] = useState<InventoryCatalog>();
  const [error, setError] = useState<string>();
  const [retry, setRetry] = useState(0);
  const [saved, setSaved] = useState(readSaved);
  const [storageMessage, setStorageMessage] = useState("");
  const searchScroll = useRef(0);
  const previousPath = useRef(location.pathname);
  const main = useRef<HTMLElement>(null);
  const scenario = id ?? new URLSearchParams(location.search).get("scenario") ?? "four-ads";
  const shortlist = location.pathname === "/saved";
  useEffect(() => {
    const controller = new AbortController();
    getCatalog(controller.signal).then(setCatalog).catch((cause: unknown) => { if (!controller.signal.aborted) setError(cause instanceof Error ? cause.message : "The preview couldn’t load."); });
    return () => controller.abort();
  }, [retry]);
  useEffect(() => { const update = () => setSaved(readSaved()); window.addEventListener("storage", update); return () => window.removeEventListener("storage", update); }, []);
  useLayoutEffect(() => {
    if (previousPath.current !== location.pathname) {
      window.scrollTo({ top: location.pathname === "/" ? searchScroll.current : 0, behavior: "instant" });
      main.current?.focus({ preventScroll: true });
      previousPath.current = location.pathname;
    }
    const capture = () => { if (location.pathname === "/") searchScroll.current = window.scrollY; };
    window.addEventListener("scroll", capture, { passive: true });
    return () => window.removeEventListener("scroll", capture);
  }, [location.pathname]);
  function toggleSave(homeId: string) {
    const next = saved.includes(homeId) ? saved.filter(value => value !== homeId) : [...saved, homeId];
    setSaved(next);
    try { localStorage.setItem(SAVE_KEY, JSON.stringify(next)); setStorageMessage(""); }
    catch { setStorageMessage("Saved for this visit. Browser storage is unavailable."); }
  }
  const summary = catalog?.homes.find(item => item.home.id === scenario);
  return <div className="property-story-page inventory-preview">
    <a className="skip-link" href="#inventory-main">Skip to content</a>
    <div className="inventory-preview-bar"><span>Design preview · example prices</span>
      {catalog && <label>Scenario<select value={scenario} onChange={event => navigate(id ? `/property/${event.target.value}?context=inventory-preview&qf=q148` : `/?scenario=${event.target.value}`)}>
        {catalog.homes.map(item => <option key={item.home.id} value={item.home.id}>{item.home.scenario}</option>)}
      </select></label>}
    </div>
    <header className="property-journey-header"><Link className="property-journey-header__brand" to={`/?scenario=${scenario}`}><BrandMark size={30} /><span>{PUBLIC_BRAND_NAME}</span></Link>
      <nav aria-label="Main navigation"><Link to={`/?scenario=${scenario}`} aria-current={!id && !shortlist ? "page" : undefined}>Explore</Link><Link to={`/saved?scenario=${scenario}`} aria-current={shortlist ? "page" : undefined}>Saved homes{saved.length > 0 && <span>{saved.length}</span>}</Link></nav>
    </header>
    <main ref={main} id="inventory-main" tabIndex={-1}>
      {error ? <div className="inventory-page-state" role="alert"><p>{error}</p><button onClick={() => { setError(undefined); setRetry(value => value + 1); }}>Try again</button></div>
        : !catalog ? <div className="inventory-page-state" role="status">Loading homes…</div>
        : id && summary ? <HomeView key={`${id}:${catalog.snapshot_id}`} summary={summary} catalog={catalog} saved={saved.includes(id)} onSave={() => toggleSave(id)} onRefreshCatalog={() => { setCatalog(undefined); setRetry(value => value + 1); }} />
        : id ? <div className="inventory-page-state"><h1>Home not found</h1><Link to="/">Return to results</Link></div>
        : <section className="inventory-results">
          <header><h1>{shortlist ? "Your saved homes" : QUERY}</h1><span>{shortlist ? saved.length : 1} {shortlist && saved.length !== 1 ? "homes" : "home"}</span></header>
          {shortlist && saved.length === 0 ? <p>Save a home to come back to it here.</p> : <div className="inventory-results__grid">
            {(shortlist ? catalog.homes.filter(item => saved.includes(item.home.id)) : summary ? [summary] : []).map(item => <HomeTile key={item.home.id} summary={item} href={`/property/${item.home.id}?context=inventory-preview&qf=q148`} saved={saved.includes(item.home.id)} onSave={() => toggleSave(item.home.id)} />)}
          </div>}
        </section>}
    </main>
    {storageMessage && <p className="inventory-storage-message" role="status">{storageMessage}</p>}
  </div>;
}
