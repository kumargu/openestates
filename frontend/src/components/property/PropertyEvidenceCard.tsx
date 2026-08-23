import { Link } from "react-router-dom";
import { ImageWithFallback } from "../ImageWithFallback.tsx";

export type PropertyEvidenceFact = {
  key: string;
  label: string;
  value: string;
};

type Props = {
  to: string;
  eyebrow: string;
  title: string;
  facts: PropertyEvidenceFact[];
  footer: string;
  imageUrl?: string;
  imageAlt?: string;
  current?: boolean;
};

export function PropertyEvidenceCard({
  to,
  eyebrow,
  title,
  facts,
  footer,
  imageUrl,
  imageAlt = "",
  current = false,
}: Props) {
  return (
    <article
      className={`property-evidence-card${current ? " is-current" : ""}`}
    >
      <Link to={to}>
        {imageUrl && (
          <div className="property-evidence-card__media">
            <ImageWithFallback
              src={imageUrl}
              alt={imageAlt}
              loading="lazy"
              fetchPriority="low"
            />
          </div>
        )}
        <div className="property-evidence-card__body">
          <header>
            <span>{eyebrow}</span>
            {current && <strong>Current home</strong>}
          </header>
          <h3>{title}</h3>
          <dl>
            {facts.map((fact) => (
              <div key={fact.key}>
                <dt>{fact.label}</dt>
                <dd>{fact.value}</dd>
              </div>
            ))}
          </dl>
          <footer>{footer} ↗</footer>
        </div>
      </Link>
    </article>
  );
}
