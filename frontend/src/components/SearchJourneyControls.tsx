import { useState } from "react";
import { journeyCopy, journeyEditTargets, journeyHistory, type SavedJourney } from "../lib/search-journey.ts";
import type { SearchResponse, SearchRevisionTarget } from "../lib/types.ts";

export function SearchJourneyControls({ response, busy, canUndo, onUndo, onRestore, onRefine }: {
  response: SearchResponse;
  busy: boolean;
  canUndo: boolean;
  onUndo: () => void;
  onRestore: (saved: SavedJourney) => void;
  onRefine: (utterance: string, target: SearchRevisionTarget) => void;
}) {
  const [targetIndex, setTargetIndex] = useState("");
  const journey = response.journey;
  if (!journey) return null;
  const history = journeyHistory(response).slice(1);
  const ambiguous = journey.attempt.outcome === "clarificationRequired";
  const targets = ambiguous ? journeyEditTargets(journey.active.intent) : [];
  const consequence = journey.attempt.selectedPropertyConsequence;
  // A retained home needs no extra reassurance unless the attempted edit failed.
  const consequenceMessage = consequence && (consequence.outcome === "excluded"
    || Boolean(consequence.failedPredicateIds?.length)) ? consequence.explanation : undefined;
  const message = consequenceMessage ?? journey.attempt.clarification?.message;
  if (!message && !canUndo && !history.length) return null;

  return <div className="home-journey" aria-busy={busy}>
    {message && !ambiguous && <p role="status">{message}</p>}
    {ambiguous && <form className="home-journey__clarification" onSubmit={(event) => {
      event.preventDefault();
      const choice = targetIndex === "" ? undefined : targets[Number(targetIndex)];
      if (choice) onRefine(journey.active.latestUtterance, choice.target);
    }}>
      <label htmlFor="journey-target">{journeyCopy.chooseTarget}</label>
      <select id="journey-target" value={targetIndex} disabled={busy}
        onChange={(event) => setTargetIndex(event.target.value)}>
        <option value="">{journeyCopy.choosePlaceholder}</option>
        {targets.map((choice, index) => <option key={index} value={index}>{choice.label}</option>)}
      </select>
      <button type="submit" disabled={busy || targetIndex === ""}>{journeyCopy.applyTarget}</button>
    </form>}
    <div className="home-journey__actions">
      {canUndo && <button type="button" disabled={busy} onClick={onUndo}>Undo</button>}
      {history.length > 0 && <details className="home-journey__history" onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.currentTarget.open = false;
          event.currentTarget.querySelector("summary")?.focus();
        }
      }}>
        <summary>{journeyCopy.history}</summary>
        <ol>
          {history.map((saved) => <li key={saved.response.journey!.active.revision.id}>
            <button type="button" disabled={busy} onClick={(event) => {
              const disclosure = event.currentTarget.closest("details");
              if (disclosure) {
                disclosure.open = false;
                disclosure.querySelector("summary")?.focus();
              }
              onRestore(saved);
            }}
              title={journeyCopy.restore}>
              {saved.response.journey!.active.buyerBrief}
            </button>
          </li>)}
        </ol>
      </details>}
    </div>
  </div>;
}
