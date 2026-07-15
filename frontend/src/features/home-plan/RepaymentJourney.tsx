import { useMemo } from "react";
import { formatCurrency, type LoanJourney } from "./model.ts";

function formatDuration(months: number): string {
  const years = Math.floor(months / 12);
  const remainingMonths = months % 12;
  if (years === 0) return `${remainingMonths}m`;
  return remainingMonths === 0 ? `${years}y` : `${years}y ${remainingMonths}m`;
}

const CHART_W = 760;
const CHART_H = 200;
const PAD = { top: 22, right: 10, bottom: 10, left: 10 };

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
  const points = journey.points;
  const selectedPoint = points.find((point) => point.year === selectedYear) ?? points[0];
  const loanFreeYear = Math.ceil(journey.loanFreeMonths / 12);
  const originalMonths = Math.max(1, journey.loanFreeMonths + journey.monthsSaved);
  const toLoanFreePct = Math.round((journey.loanFreeMonths / originalMonths) * 100);

  const geometry = useMemo(() => {
    const maxYear = Math.max(1, points[points.length - 1]?.year ?? 1);
    const maxBalance = Math.max(1, ...points.map((point) => point.balance));
    const innerW = CHART_W - PAD.left - PAD.right;
    const innerH = CHART_H - PAD.top - PAD.bottom;
    const x = (year: number) => PAD.left + (year / maxYear) * innerW;
    const y = (balance: number) => CHART_H - PAD.bottom - (balance / maxBalance) * innerH;

    const coords = points.map((point) => ({ ...point, cx: x(point.year), cy: y(point.balance) }));
    const line = coords.map((c, i) => `${i === 0 ? "M" : "L"} ${c.cx.toFixed(1)} ${c.cy.toFixed(1)}`).join(" ");
    const area = `${line} L ${x(maxYear).toFixed(1)} ${(CHART_H - PAD.bottom).toFixed(1)} L ${x(0).toFixed(1)} ${(CHART_H - PAD.bottom).toFixed(1)} Z`;
    const step = innerW / maxYear;
    return { coords, line, area, step };
  }, [points]);

  const selectedCoord = geometry.coords.find((c) => c.year === selectedYear) ?? geometry.coords[0];

  return (
    <section className="home-plan-payoff" aria-label="Loan payoff plan">
      <header className="home-plan-payoff__head">
        <div className="home-plan-payoff__title">
          <span className="home-plan-payoff__badge" aria-hidden="true">₹</span>
          <div>
            <h2>Pay off the loan faster</h2>
            <p>Same monthly EMI. Add a few extra payments each year and watch the finish line move.</p>
          </div>
        </div>
        <div className="home-plan-payoff__progress" aria-label={`Loan-free in ${formatDuration(journey.loanFreeMonths)}`}>
          <div className="home-plan-payoff__progress-head">
            <strong>{formatDuration(journey.loanFreeMonths)}</strong>
            <small>{journey.monthsSaved > 0 ? `${formatDuration(journey.monthsSaved)} sooner` : "on original schedule"}</small>
          </div>
          <div className="home-plan-payoff__bar">
            <span className="home-plan-payoff__bar-fill" style={{ width: `${toLoanFreePct}%` }} />
          </div>
        </div>
      </header>

      <div className="home-plan-payoff__lever" role="group" aria-label="Extra EMIs each year">
        <span className="home-plan-payoff__lever-label">Extra EMIs a year</span>
        <div className="home-plan-payoff__seg">
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

      <ul className="home-plan-payoff__outcomes">
        <li>
          <span className="home-plan-payoff__outcome-label">Extra you pay each year</span>
          <span className="home-plan-payoff__outcome-value">{formatCurrency(journey.annualPrepayment, true)}</span>
          <span className="home-plan-payoff__chip">{extraEmisPerYear} {extraEmisPerYear === 1 ? "EMI" : "EMIs"}</span>
        </li>
        <li>
          <span className="home-plan-payoff__outcome-label">Time you save</span>
          <span className="home-plan-payoff__outcome-value">{journey.monthsSaved > 0 ? formatDuration(journey.monthsSaved) : "—"}</span>
          <span className="home-plan-payoff__chip home-plan-payoff__chip--good">{journey.monthsSaved > 0 ? "earlier" : "no change"}</span>
        </li>
        <li>
          <span className="home-plan-payoff__outcome-label">Interest you save</span>
          <span className="home-plan-payoff__outcome-value">{formatCurrency(journey.interestSaved, true)}</span>
          <span className="home-plan-payoff__chip home-plan-payoff__chip--good">{journey.interestSaved > 0 ? "saved" : "—"}</span>
        </li>
      </ul>

      <div className="home-plan-payoff__chart-wrap">
        <div className="home-plan-payoff__chart-head">
          <span>Loan balance falling to zero</span>
          <small>Hover the curve to read any year</small>
        </div>
        <svg
          className="home-plan-payoff__chart"
          viewBox={`0 0 ${CHART_W} ${CHART_H}`}
          preserveAspectRatio="none"
          role="img"
          aria-label="Outstanding loan balance by year"
        >
          <defs>
            <linearGradient id="payoffFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--plan-accent)" stopOpacity="0.22" />
              <stop offset="100%" stopColor="var(--plan-accent)" stopOpacity="0" />
            </linearGradient>
          </defs>
          <path d={geometry.area} fill="url(#payoffFill)" />
          <path d={geometry.line} fill="none" stroke="var(--plan-accent)" strokeWidth="2.5" strokeLinejoin="round" strokeLinecap="round" />

          {selectedCoord && (
            <g className="home-plan-payoff__cursor">
              <line x1={selectedCoord.cx} y1={PAD.top - 8} x2={selectedCoord.cx} y2={CHART_H - PAD.bottom} />
              <circle cx={selectedCoord.cx} cy={selectedCoord.cy} r="5" />
            </g>
          )}

          {geometry.coords.map((c) => (
            <rect
              key={c.year}
              x={c.cx - geometry.step / 2}
              y={0}
              width={geometry.step}
              height={CHART_H}
              fill="transparent"
              style={{ cursor: "pointer" }}
              onMouseEnter={() => onSelectYear(c.year)}
              onClick={() => onSelectYear(c.year)}
            />
          ))}
        </svg>
      </div>

      <div className="home-plan-payoff__readout">
        <span className="home-plan-payoff__readout-year">
          {selectedPoint.year === 0 ? "At loan start" : `End of year ${selectedPoint.year}`}
          {selectedPoint.year >= loanFreeYear && selectedPoint.balance <= 0.5 && <em>· loan-free</em>}
        </span>
        <dl>
          <div><dt>Outstanding</dt><dd>{formatCurrency(selectedPoint.balance, true)}</dd></div>
          <div><dt>Principal paid</dt><dd>{formatCurrency(selectedPoint.principalPaid, true)}</dd></div>
          <div><dt>Interest paid</dt><dd>{formatCurrency(selectedPoint.interestPaid, true)}</dd></div>
          <div><dt>Extra prepaid</dt><dd>{formatCurrency(selectedPoint.extraPaid, true)}</dd></div>
        </dl>
      </div>
    </section>
  );
}
