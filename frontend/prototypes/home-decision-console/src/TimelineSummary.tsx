import { useEffect, useRef } from "react";
import { formatCurrency } from "./model.ts";

export type TimelinePoint = {
  year: number;
  value: number;
  change: number | null;
  leader: string;
  event?: string;
};

type ScenarioId = "buy" | "rent" | "smaller";

export function TimelineSummary({
  points,
  selectedYear,
  selectedScenario,
  selectedLabel,
  onSelectYear,
}: {
  points: TimelinePoint[];
  selectedYear: number;
  selectedScenario: ScenarioId;
  selectedLabel: string;
  onSelectYear: (year: number) => void;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = scrollRef.current;
    const selectedButton = container?.querySelector<HTMLButtonElement>(`[data-year="${selectedYear}"]`);
    if (!container || !selectedButton) return;
    const centeredPosition = selectedButton.offsetLeft - container.clientWidth / 2 + selectedButton.clientWidth / 2;
    container.scrollTo({ left: centeredPosition, behavior: "smooth" });
  }, [selectedYear]);

  return (
    <section className="timeline-panel">
      <div className="timeline-heading">
        <div><span>EXPERIMENTAL VIEW</span><h2>Year-by-year position</h2><p>Follow how the selected strategy compounds and when the decision changes.</p></div>
        <div className={`timeline-selected timeline-selected--${selectedScenario}`}><i /><span><small>Following</small><strong>{selectedLabel}</strong></span></div>
      </div>
      <div ref={scrollRef} className="timeline-scroll" tabIndex={0} aria-label="Year-by-year projection timeline">
        <div className="timeline-track">
          {points.map((point) => (
            <button key={point.year} data-year={point.year} className={selectedYear === point.year ? "selected" : ""} onClick={() => onSelectYear(point.year)}>
              <span className="timeline-year">{point.year === 0 ? "Now" : `Y${point.year}`}</span>
              <strong>{formatCurrency(point.value, true)}</strong>
              <small className={point.change !== null && point.change < 0 ? "negative" : ""}>{point.change === null ? "Starting point" : `${point.change >= 0 ? "+" : ""}${point.change.toFixed(1)}% YoY`}</small>
              <span className="timeline-leader">{point.leader} leads</span>
              {point.event && <em>{point.event}</em>}
            </button>
          ))}
        </div>
      </div>
      <div className="timeline-footnote"><span>Click any year to move the graph</span><span>YoY values use the selected strategy</span></div>
    </section>
  );
}
