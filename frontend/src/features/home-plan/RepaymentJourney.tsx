import { formatCurrency, type LoanJourney } from "./model.ts";
import { PayoffGraph } from "./PayoffGraph.tsx";

function formatDuration(months: number): string {
  const years = Math.floor(months / 12);
  const remainingMonths = months % 12;
  if (years === 0) return `${remainingMonths}m`;
  return remainingMonths === 0 ? `${years}y` : `${years}y ${remainingMonths}m`;
}

export function RepaymentJourney({
  journey,
  baselineJourney,
  extraEmisPerYear,
  selectedYear,
  onExtraEmisChange,
  onSelectYear,
}: {
  journey: LoanJourney;
  baselineJourney: LoanJourney;
  extraEmisPerYear: number;
  selectedYear: number;
  onExtraEmisChange: (count: number) => void;
  onSelectYear: (year: number) => void;
}) {
  const points = journey.points;
  const selectedPoint = points.find((point) => point.year === selectedYear) ?? points[0];
  const loanFreeYear = Math.ceil(journey.loanFreeMonths / 12);
  const originalMonths = Math.max(1, journey.loanFreeMonths + journey.monthsSaved);
  const toLoanFreePct = Math.round((journey.loanFreeMonths / originalMonths) * 100);
  const hasPrepay = extraEmisPerYear > 0;

  return (
    <section className="home-plan-payoff" aria-label="Loan payoff plan">
      <header className="home-plan-verdict">
        <p className="home-plan-verdict__year">
          {hasPrepay
            ? `With ${extraEmisPerYear} extra ${extraEmisPerYear === 1 ? "EMI" : "EMIs"} a year`
            : "On your current schedule"}
        </p>
        <h1 className="home-plan-verdict__headline">
          Loan-free in <span className="home-plan-verdict__amount">{formatDuration(journey.loanFreeMonths)}</span>
          {journey.interestSaved > 0 && (
            <>
              {" "}— saving <span className="home-plan-verdict__amount">{formatCurrency(journey.interestSaved, true)}</span>
            </>
          )}
        </h1>
        <p className="home-plan-verdict__detail">
          {journey.monthsSaved > 0
            ? `${formatDuration(journey.monthsSaved)} sooner than paying the EMI alone — same monthly outgoing, just a few extra payments a year.`
            : "Add extra payments below to clear the loan sooner and cut total interest."}
        </p>
        <div className="home-plan-payoff__bar" aria-hidden="true">
          <span className="home-plan-payoff__bar-fill" style={{ width: `${toLoanFreePct}%` }} />
        </div>

        <dl className="home-plan-verdict__breakdown home-plan-verdict__breakdown--3" aria-label="Payoff impact">
          <div className="home-plan-verdict__tile">
            <dt>Extra each year</dt>
            <dd>{formatCurrency(journey.annualPrepayment, true)}</dd>
            <small>{extraEmisPerYear} {extraEmisPerYear === 1 ? "EMI" : "EMIs"}</small>
          </div>
          <div className="home-plan-verdict__tile home-plan-verdict__tile--highlight">
            <dt>Time saved</dt>
            <dd>{journey.monthsSaved > 0 ? formatDuration(journey.monthsSaved) : "—"}</dd>
            <small>{journey.monthsSaved > 0 ? "Earlier finish" : "No change yet"}</small>
          </div>
          <div className="home-plan-verdict__tile home-plan-verdict__tile--highlight">
            <dt>Interest saved</dt>
            <dd>{formatCurrency(journey.interestSaved, true)}</dd>
            <small>{journey.interestSaved > 0 ? "vs no prepayment" : "—"}</small>
          </div>
        </dl>
      </header>

      <div className="home-plan-payoff__lever" role="group" aria-label="Extra EMIs each year">
        <span className="home-plan-payoff__lever-label">Extra EMIs a year</span>
        <div className="home-plan-view-tabs__seg home-plan-payoff__seg">
          {[0, 1, 2, 3, 4, 6].map((count) => (
            <button
              type="button"
              key={count}
              className={extraEmisPerYear === count ? "is-active" : ""}
              aria-pressed={extraEmisPerYear === count}
              onClick={() => onExtraEmisChange(count)}
            >
              <strong>{count}</strong>
              <small>{count === 1 ? "EMI" : "EMIs"}</small>
            </button>
          ))}
        </div>
      </div>

      <PayoffGraph
        journey={journey}
        baselineJourney={baselineJourney}
        extraEmisPerYear={extraEmisPerYear}
        selectedYear={selectedYear}
        onSelectYear={onSelectYear}
      />

      <div className="home-plan-payoff__readout">
        <p className="home-plan-verdict__year">
          {selectedPoint.year === 0 ? "At loan start" : `End of year ${selectedPoint.year}`}
          {selectedPoint.year >= loanFreeYear && selectedPoint.balance <= 0.5 && (
            <span className="home-plan-payoff__readout-badge"> · loan-free</span>
          )}
        </p>
        <dl className="home-plan-verdict__breakdown home-plan-verdict__breakdown--4">
          <div className="home-plan-verdict__tile"><dt>Outstanding</dt><dd>{formatCurrency(selectedPoint.balance, true)}</dd></div>
          <div className="home-plan-verdict__tile"><dt>Principal paid</dt><dd>{formatCurrency(selectedPoint.principalPaid, true)}</dd></div>
          <div className="home-plan-verdict__tile"><dt>Interest paid</dt><dd>{formatCurrency(selectedPoint.interestPaid, true)}</dd></div>
          <div className="home-plan-verdict__tile"><dt>Extra prepaid</dt><dd>{formatCurrency(selectedPoint.extraPaid, true)}</dd></div>
        </dl>
      </div>
    </section>
  );
}
