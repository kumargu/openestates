import { Link } from "react-router-dom";
import type { StoryRecordCard } from "../../lib/propertyStory.ts";
import "../../styles/property-fact-decks.css";

type Props = {
  card?: StoryRecordCard;
};

export function PropertyReraTeaser({ card }: Props) {
  if (!card) return null;

  return (
    <section
      id="official-record"
      className="property-fact-deck property-rera-teaser"
      aria-labelledby="property-rera-teaser-title"
    >
      <header className="property-fact-deck__intro">
        <span>Official record</span>
        <h2 id="property-rera-teaser-title">RERA facts at a glance.</h2>
      </header>

      <Link className="property-rera-teaser__card" to={card.href}>
        <div className="property-rera-teaser__lead">
          <span>{card.label}</span>
          <strong>
            {card.facts.length > 0 ? "On paper." : "Open the official record."}
          </strong>
        </div>
        {card.facts.length > 0 && (
          <dl className="property-rera-teaser__facts">
            {card.facts.map((fact) => (
              <div key={fact.key}>
                <dt>{fact.label}</dt>
                {fact.value && <dd>{fact.value}</dd>}
              </div>
            ))}
          </dl>
        )}
        <span className="property-rera-teaser__open" aria-hidden="true">↗</span>
      </Link>
    </section>
  );
}
