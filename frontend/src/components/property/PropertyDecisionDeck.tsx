import { Link } from "react-router-dom";
import { NotebookCommentAnchor } from "../notebook/NotebookCommentAnchor.tsx";
import { SaveHeartButton } from "../SaveHeartButton.tsx";
import "../../styles/property-fact-decks.css";

type Props = {
  propertyId: string;
  title: string;
  compareHref?: string;
  reraHref?: string;
};

export function PropertyDecisionDeck({
  propertyId,
  title,
  compareHref,
  reraHref,
}: Props) {
  return (
    <section
      id="decision"
      className="property-fact-deck property-decision-deck"
      aria-labelledby="property-decision-deck-title"
    >
      <header className="property-fact-deck__intro">
        <span>Your decision</span>
        <h2 id="property-decision-deck-title">Finish the checks. Then decide.</h2>
      </header>

      <div className="property-decision-deck__actions">
        <SaveHeartButton
          propertyId={propertyId}
          className="property-decision-deck__action"
          label="Save home"
        />
        <NotebookCommentAnchor
          propertyId={propertyId}
          labels={[]}
          detail={title}
          source="Property detail"
          className="property-decision-deck__action"
        />
        {compareHref && (
          <Link className="property-decision-deck__action" to={compareHref}>
            <strong>Compare these homes</strong>
            <span aria-hidden="true">↗</span>
          </Link>
        )}
        {reraHref && (
          <Link className="property-decision-deck__action" to={reraHref}>
            <strong>Read the RERA report</strong>
            <span aria-hidden="true">↗</span>
          </Link>
        )}
      </div>
    </section>
  );
}
