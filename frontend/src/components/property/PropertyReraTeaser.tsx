import { Link } from "react-router-dom";
import type { StoryRecordCard } from "../../lib/propertyStory.ts";
import type { PropertyProofMatch } from "../../lib/proof-focus.ts";
import "../../styles/property-fact-decks.css";

type Props = {
  cards: StoryRecordCard[];
  focusedMatch?: PropertyProofMatch;
};

export function PropertyReraTeaser({ cards, focusedMatch }: Props) {
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
  const hasRecordFacts = Boolean(registration?.value) || documentFacts.length > 0;
  if (!focusedMatch && (!href || !hasRecordFacts)) return null;
  const facts = [
    ...(registration?.value
      ? [{ ...registration, label: "Registration" }]
      : []),
    ...documentFacts.map((fact) => ({
      ...fact,
      label: fact.label.replace(/\s+available$/i, ""),
    })),
    ...(!hasRecordFacts && focusedMatch
      ? [{ key: "search-match", label: "Matched your search", value: focusedMatch.value }]
      : []),
  ];

  return (
    <section
      id="official-record"
      className="property-fact-deck property-rera-teaser"
      aria-labelledby="property-rera-teaser-title"
      tabIndex={-1}
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

      {href ? (
        <Link className="property-rera-teaser__open" to={href}>
          Open report ↗
        </Link>
      ) : focusedMatch?.sourceUrl ? (
        <a
          className="property-rera-teaser__open"
          href={focusedMatch.sourceUrl}
          target="_blank"
          rel="noreferrer"
        >
          Source ↗
        </a>
      ) : null}
    </section>
  );
}
