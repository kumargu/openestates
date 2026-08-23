import { Link } from "react-router-dom";
import type { StoryRecordCard } from "../../lib/propertyStory.ts";
import "../../styles/property-fact-decks.css";

type Props = {
  cards: StoryRecordCard[];
};

export function PropertyReraTeaser({ cards }: Props) {
  const visibleCards = cards.filter((card) =>
    card.facts.some((fact) => Boolean(fact.value)));
  const registration = visibleCards
    .flatMap((card) => card.facts)
    .find((fact) => fact.key === "registration" && fact.value);
  const documentFacts = visibleCards
    .flatMap((card) => card.facts)
    .filter((fact) => fact.key !== "registration" && fact.value)
    .slice(0, 3);
  const href = visibleCards[0]?.href;
  if (!registration?.value || !href) return null;
  const facts = [
    { ...registration, label: "Registration" },
    ...documentFacts.map((fact) => ({
      ...fact,
      label: fact.label.replace(/\s+available$/i, ""),
    })),
  ];

  return (
    <section
      id="official-record"
      className="property-fact-deck property-rera-teaser"
      aria-labelledby="property-rera-teaser-title"
    >
      <header className="property-rera-teaser__intro">
        <h2 id="property-rera-teaser-title">RERA record</h2>
      </header>

      <dl className="property-rera-teaser__facts">
        {facts.map((fact) => (
          <div key={fact.key}>
            <dt>{fact.label}</dt>
            <dd>{fact.value}</dd>
          </div>
        ))}
      </dl>

      <Link className="property-rera-teaser__open" to={href}>
        Open report ↗
      </Link>
    </section>
  );
}
