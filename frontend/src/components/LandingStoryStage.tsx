import { Link } from "react-router-dom";
import { LivingEvidenceTile } from "./evidence/LivingEvidenceTile.tsx";
import { propertyDetailPath } from "../lib/api.ts";
import {
  LANDING_PROOF_QUERY,
  LANDING_PROOF_QUERY_LABEL,
  landingDiscoveryHomes,
} from "../lib/landing-discovery.ts";
import type { PropertyCard, SearchResponse } from "../lib/types.ts";

const FEATURED_LIMIT = 6;

type LandingStoryStageProps = {
  properties: PropertyCard[];
  proofSearch: SearchResponse | null;
  onSearch: (query: string) => void;
};

export function LandingStoryStage({
  properties,
  proofSearch,
  onSearch,
}: LandingStoryStageProps) {
  const collection = landingDiscoveryHomes(properties, proofSearch, FEATURED_LIMIT);
  const { homes } = collection;
  const firstHome = homes[0]?.property;

  if (!firstHome) return null;

  return (
    <section className="landing-stage" aria-label="Discover homes">
      <div className="landing-featured">
        <div className="landing-featured__head">
          <div>
            <h2>Homes worth a closer look</h2>
            {collection.source === "search" ? <p>{LANDING_PROOF_QUERY_LABEL}</p> : null}
          </div>
          {collection.source === "search" ? (
            <button
              type="button"
              className="landing-featured__search"
              onClick={() => onSearch(LANDING_PROOF_QUERY)}
            >
              View matches
            </button>
          ) : null}
        </div>

        <div className="landing-stage__featured">
          {homes.map(({ property, buyerProof, proofFocus }) => (
            <div key={property.id} className="landing-stage__feature-card">
              <LivingEvidenceTile
                property={property}
                variant="browse"
                buyerProof={buyerProof}
                proofFocus={proofFocus}
                proofDensity="receipt"
                allowSave
              />
            </div>
          ))}
        </div>
      </div>

      <nav className="landing-journey" aria-label="From search to decision">
        <a href="#home-search">
          <span>1</span>
          <strong>Describe the life</strong>
          <small>Search with budget, place and tradeoffs.</small>
        </a>
        <Link to={propertyDetailPath(firstHome.id)}>
          <span>2</span>
          <strong>Check the home</strong>
          <small>Read the reasons, risks and receipts.</small>
        </Link>
        <Link to="/workspace">
          <span>3</span>
          <strong>Keep your judgment</strong>
          <small>Save notes and compare the differences.</small>
        </Link>
      </nav>
    </section>
  );
}
