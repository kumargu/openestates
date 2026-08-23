import type { StoryRecordCard } from "../../lib/propertyStory.ts";
import { PropertyEvidenceCard } from "./PropertyEvidenceCard.tsx";
import "../../styles/property-fact-decks.css";

type Props = {
  cards: StoryRecordCard[];
};

export function PropertyReraTeaser({ cards }: Props) {
  const visibleCards = cards.filter((card) =>
    card.facts.some((fact) => Boolean(fact.value)));
  if (visibleCards.length === 0) return null;

  return (
    <section
      id="official-record"
      className="property-fact-deck property-rera-teaser"
      aria-labelledby="property-rera-teaser-title"
    >
      <header className="property-story-heading">
        <span>Official record</span>
        <h2 id="property-rera-teaser-title">What is filed.</h2>
      </header>

      <div className="property-evidence-grid property-evidence-grid--record">
        {visibleCards.map((card) => (
          <PropertyEvidenceCard
            key={card.id}
            to={card.href}
            eyebrow={card.label}
            title={card.title}
            facts={card.facts.flatMap((fact) =>
              fact.value
                ? [{
                    key: fact.key,
                    label: fact.label,
                    value: fact.value,
                  }]
                : [])}
            footer="View RERA report"
          />
        ))}
      </div>
    </section>
  );
}
