import { useEffect, useRef } from "react";
import { formatCurrency, type LoanJourney } from "./model.ts";

function formatDuration(months: number): string {
  const years = Math.floor(months / 12);
  const remainingMonths = months % 12;
  return remainingMonths === 0 ? `${years} years` : `${years}y ${remainingMonths}m`;
}

export function RepaymentJourney({
  journey,
  extraEmisPerYear,
  selectedYear,
  onExtraEmisChange,
  onSelectYear,
}: {
  journey: LoanJourney;
  extraEmisPerYear: number;
  selectedYear: number;
  onExtraEmisChange: (count: number) => void;
  onSelectYear: (year: number) => void;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const selectedPoint = journey.points.find((point) => point.year === selectedYear) ?? journey.points[0];
  const loanFreeYear = Math.ceil(journey.loanFreeMonths / 12);

  useEffect(() => {
    const container = scrollRef.current;
    const selectedButton = container?.querySelector<HTMLButtonElement>(`[data-year="${selectedYear}"]`);
    if (!container || !selectedButton) return;
    container.scrollTo({
      left: selectedButton.offsetLeft - container.clientWidth / 2 + selectedButton.clientWidth / 2,
      behavior: "smooth",
    });
  }, [selectedYear]);

  return (
    <section className="home-plan-repayment">
      <div className="home-plan-repayment-intro">
        <div>
          <span>Annual prepayment</span>
          <h2>See how extra EMIs shorten the loan.</h2>
          <p>Your regular EMI stays unchanged. Each annual prepayment goes directly toward principal.</p>
        </div>
        <div className="home-plan-repayment-options" aria-label="Extra EMIs per year">
          {[0, 1, 2, 3, 4, 6].map((count) => (
            <button type="button" key={count} className={extraEmisPerYear === count ? "is-active" : ""} onClick={() => onExtraEmisChange(count)}>
              <strong>{count}</strong>
              <small>{count === 1 ? "EMI" : "EMIs"}</small>
            </button>
          ))}
        </div>
      </div>

      <div className="home-plan-repayment-metrics">
        <div><span>Regular EMI</span><strong>{formatCurrency(journey.monthlyEmi)}</strong><small>Paid every month</small></div>
        <div><span>Extra each year</span><strong>{formatCurrency(journey.annualPrepayment, true)}</strong><small>{extraEmisPerYear} additional EMIs</small></div>
        <div><span>Loan-free in</span><strong>{formatDuration(journey.loanFreeMonths)}</strong><small>{formatDuration(journey.monthsSaved)} earlier</small></div>
        <div><span>Interest saved</span><strong>{formatCurrency(journey.interestSaved, true)}</strong><small>Compared with no prepayment</small></div>
      </div>

      <section className="home-plan-repayment-timeline">
        <header>
          <div><span>Year-by-year journey</span><h2>Watch the outstanding balance fall.</h2><p>Select a year to inspect principal, interest, and prepayments.</p></div>
          <div className="home-plan-loan-free-badge"><small>Expected loan-free point</small><strong>{formatDuration(journey.loanFreeMonths)}</strong></div>
        </header>
        <div ref={scrollRef} className="home-plan-timeline-scroll" tabIndex={0} aria-label="Year-by-year loan journey">
          <div className="home-plan-timeline-track">
            {journey.points.map((point) => {
              const isLoanFree = point.balance <= 0.5;
              const isPayoffYear = point.year === loanFreeYear;
              return (
                <button type="button" key={point.year} data-year={point.year} className={selectedYear === point.year ? "is-selected" : ""} onClick={() => onSelectYear(point.year)}>
                  <span>{point.year === 0 ? "Start" : `Y${point.year}`}</span>
                  <strong>{formatCurrency(point.balance, true)}</strong>
                  <small>{isLoanFree ? (isPayoffYear ? "No balance" : "Time saved") : "Outstanding"}</small>
                  <em>{point.interestPaid > 0 ? `${formatCurrency(point.interestPaid, true)} interest` : isPayoffYear ? "Final payment year" : "No EMI due"}</em>
                  {point.extraPaid > 0 && <i>+{extraEmisPerYear} EMIs</i>}
                  {isPayoffYear && <i>Loan-free</i>}
                </button>
              );
            })}
          </div>
        </div>
        <div className="home-plan-repayment-detail">
          <div><span>Selected point</span><strong>{selectedPoint.year === 0 ? "Loan start" : `End of loan year ${selectedPoint.year}`}</strong></div>
          <dl>
            <div><dt>Outstanding</dt><dd>{formatCurrency(selectedPoint.balance, true)}</dd></div>
            <div><dt>Principal paid</dt><dd>{formatCurrency(selectedPoint.principalPaid, true)}</dd></div>
            <div><dt>Interest paid</dt><dd>{formatCurrency(selectedPoint.interestPaid, true)}</dd></div>
            <div><dt>Extra prepaid</dt><dd>{formatCurrency(selectedPoint.extraPaid, true)}</dd></div>
          </dl>
        </div>
      </section>
    </section>
  );
}
