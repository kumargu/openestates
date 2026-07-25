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
  const hasPrepay = extraEmisPerYear > 0;
  const maxYear = points.at(-1)?.year ?? 0;
  const yearOptions = [0, 5, 10, 15, 20].filter((year) => year <= maxYear);
  const baselinePoint = baselineJourney.points.find((point) => point.year === selectedPoint.year);
  const balanceAhead = Math.max(0, (baselinePoint?.balance ?? selectedPoint.balance) - selectedPoint.balance);
  const extraPaidToDate = points
    .filter((point) => point.year <= selectedPoint.year)
    .reduce((total, point) => total + point.extraPaid, 0);

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
            ? `${formatDuration(journey.monthsSaved)} sooner · ${formatCurrency(journey.interestSaved, true)} less interest`
            : "Add extra payments below to clear the loan sooner and cut total interest."}
        </p>
        <p className="home-plan-payoff__cost">
          {formatCurrency(journey.annualPrepayment, true)} extra a year · monthly EMI stays {formatCurrency(journey.monthlyEmi, true)}
        </p>
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

      <div className="home-plan-payoff__year-focus">
        <div className="home-plan-payoff__year-picker" role="group" aria-label="View payoff progress by year">
          {yearOptions.map((year) => (
            <button
              key={year}
              type="button"
              className={selectedPoint.year === year ? "is-active" : ""}
              aria-pressed={selectedPoint.year === year}
              onClick={() => onSelectYear(year)}
            >
              {year === 0 ? "Now" : `${year}y`}
            </button>
          ))}
        </div>

        <div className="home-plan-payoff__year-readout" aria-live="polite">
          <div className="home-plan-payoff__year-heading">
            <span>{selectedPoint.year === 0 ? "Today" : `End of year ${selectedPoint.year}`}</span>
            {selectedPoint.year >= loanFreeYear && selectedPoint.balance <= 0.5 && <strong>Loan-free</strong>}
          </div>
          <dl>
            <div>
              <dt>Balance</dt>
              <dd>{formatCurrency(selectedPoint.balance, true)}</dd>
            </div>
            <div>
              <dt>Ahead vs regular EMI</dt>
              <dd>{formatCurrency(balanceAhead, true)}</dd>
            </div>
            <div>
              <dt>Extra paid so far</dt>
              <dd>{formatCurrency(extraPaidToDate, true)}</dd>
            </div>
          </dl>
        </div>
      </div>
    </section>
  );
}
