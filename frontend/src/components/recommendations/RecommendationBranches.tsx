import { Link } from "react-router-dom";
import type { RecommendationBranch } from "../../lib/types.ts";
import { ImageWithFallback } from "../ImageWithFallback.tsx";

function formatPrice(price: number): string {
  if (price >= 1_00_00_000) return `₹${(price / 1_00_00_000).toFixed(2)} Cr`;
  if (price >= 1_00_000) return `₹${(price / 1_00_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

const LENS_CLASS: Record<RecommendationBranch["lens"], string> = {
  proof: "rec-branch--proof",
  value: "rec-branch--value",
  trust: "rec-branch--trust",
  commute: "rec-branch--commute",
};

export function RecommendationBranches({ branches }: { branches: RecommendationBranch[] }) {
  if (branches.length === 0) return null;

  return (
    <section className="rec-branches">
      <div className="property-section-heading">
        <span>If this isn&apos;t quite right</span>
        <h2>Branches worth considering</h2>
      </div>

      <div className="rec-branches__stack">
        {branches.map((branch) => (
          <Link
            key={`${branch.lens}-${branch.property.id}`}
            to={`/property/${branch.property.id}`}
            className={`rec-branch ${LENS_CLASS[branch.lens]}`}
          >
            <div className="rec-branch__media">
              <ImageWithFallback
                src={branch.property.hero_image || ""}
                alt={branch.property.title}
                className="rec-branch__image"
                loading="lazy"
              />
            </div>

            <div className="rec-branch__body">
              <span className="rec-branch__lens">{branch.headline}</span>
              <strong className="rec-branch__title">{branch.property.title}</strong>
              <span className="rec-branch__meta">
                {branch.property.society_name} · {branch.property.area}
              </span>
              <p className="rec-branch__contrast">{branch.contrast}</p>
              {branch.tradeoff && (
                <span className="rec-branch__tradeoff">{branch.tradeoff}</span>
              )}
            </div>

            <div className="rec-branch__aside">
              <strong>{formatPrice(branch.property.price)}</strong>
              <span className="rec-branch__delta">
                {branch.evidence_delta.fact_delta >= 0 ? "+" : ""}
                {branch.evidence_delta.fact_delta} facts
              </span>
              <span className="rec-branch__chevron" aria-hidden="true">→</span>
            </div>
          </Link>
        ))}
      </div>
    </section>
  );
}
