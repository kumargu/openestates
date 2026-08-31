import {
  Fragment,
  useEffect,
  useRef,
  useState,
} from "react";
import {
  formatCurrency,
  formatLakhCurrency,
  type PlanInputs,
} from "./model.ts";
import type { RepaymentDashboardModel } from "./repaymentModel.ts";
import {
  repaymentScheduleCsv,
  type RepaymentScheduleMonth,
  type RepaymentScheduleYear,
} from "./repaymentSchedule.ts";

type RepaymentScheduleSheetProps = {
  open: boolean;
  inputs: PlanInputs;
  model: RepaymentDashboardModel;
  years: RepaymentScheduleYear[];
  activeYear: number;
  onSelectYear: (year: number) => void;
  onClose: () => void;
};

function durationLabel(months: number | null): string {
  if (months == null) return "Not repaid";
  const years = Math.floor(months / 12);
  const remainder = months % 12;
  if (remainder === 0) return `${years} yr`;
  return `${years} yr ${remainder} mo`;
}

function scheduleMoney(value: number): string {
  const absolute = Math.abs(value);
  if (absolute >= 10_000_000) return formatCurrency(value, true);
  if (absolute >= 1_000_000) return `₹${(value / 100_000).toFixed(1)}L`;
  if (absolute >= 100_000) return formatLakhCurrency(value);
  return formatCurrency(value);
}

function milestoneFor(
  year: number,
  model: RepaymentDashboardModel,
): string | null {
  if (year === 1) return "Interest-heavy";
  if (year === model.markers.crossoverYear) return "Principal leads";
  if (year === model.markers.halfFirstYearImpactYear) return "Half-impact";
  return null;
}

function ValueBar({
  value,
  maximum,
  tone,
}: {
  value: number;
  maximum: number;
  tone: "interest" | "principal";
}) {
  const width = maximum <= 0 ? 0 : Math.max(0, Math.min(100, value / maximum * 100));
  return (
    <svg className={`home-plan-schedule-value-bar is-${tone}`} viewBox="0 0 100 2" aria-hidden="true">
      <rect width={width} height="2" />
    </svg>
  );
}

function AnnualRow({
  year,
  model,
  expanded,
  active,
  maxInterest,
  maxPrincipal,
  onToggle,
}: {
  year: RepaymentScheduleYear;
  model: RepaymentDashboardModel;
  expanded: boolean;
  active: boolean;
  maxInterest: number;
  maxPrincipal: number;
  onToggle: () => void;
}) {
  const milestone = milestoneFor(year.year, model);
  return (
    <tr className={`home-plan-schedule-year ${expanded ? "is-expanded" : ""} ${active ? "is-active" : ""}`}>
      <th scope="row">
        <button type="button" aria-expanded={expanded} onClick={onToggle}>
          <span aria-hidden="true">{expanded ? "⌄" : "›"}</span>
          Year {year.year}
        </button>
        {milestone ? <em>{milestone}</em> : null}
      </th>
      <td>{scheduleMoney(year.totalPaid)}</td>
      <td>
        {scheduleMoney(year.interestPaid)}
        <ValueBar value={year.interestPaid} maximum={maxInterest} tone="interest" />
      </td>
      <td>
        {scheduleMoney(year.principalPaid)}
        <ValueBar value={year.principalPaid} maximum={maxPrincipal} tone="principal" />
      </td>
      <td>{year.extraPaid > 0 ? scheduleMoney(year.extraPaid) : "—"}</td>
      <td>{scheduleMoney(year.closingBalance)}</td>
      <td className="is-avoided">
        {year.cumulativeInterestAvoided > 0
          ? `+${scheduleMoney(year.cumulativeInterestAvoided)}`
          : "—"}
      </td>
    </tr>
  );
}

function MonthlyRow({
  month,
}: {
  month: RepaymentScheduleMonth;
}) {
  const monthWithinYear = (month.paymentNumber - 1) % 12 + 1;
  return (
    <tr className="home-plan-schedule-month">
      <th scope="row">Month {monthWithinYear}</th>
      <td>{scheduleMoney(month.totalPaid)}</td>
      <td>{scheduleMoney(month.interestPaid)}</td>
      <td>{scheduleMoney(month.principalPaid)}</td>
      <td>{month.extraPaid > 0 ? scheduleMoney(month.extraPaid) : "—"}</td>
      <td>{scheduleMoney(month.closingBalance)}</td>
      <td aria-label={`Cumulative interest avoided ${scheduleMoney(month.cumulativeInterestAvoided)}`}>
        —
      </td>
    </tr>
  );
}

function MobileYearDetails({ year }: { year: RepaymentScheduleYear }) {
  return (
    <tr className="home-plan-schedule-mobile-details">
      <td colSpan={7}>
        <dl>
          <div><dt>Total paid</dt><dd>{scheduleMoney(year.totalPaid)}</dd></div>
          <div><dt>Extra principal</dt><dd>{year.extraPaid > 0 ? scheduleMoney(year.extraPaid) : "—"}</dd></div>
          <div>
            <dt>Interest avoided</dt>
            <dd>{year.cumulativeInterestAvoided > 0 ? scheduleMoney(year.cumulativeInterestAvoided) : "—"}</dd>
          </div>
        </dl>
      </td>
    </tr>
  );
}

export function RepaymentScheduleSheet({
  open,
  inputs,
  model,
  years,
  activeYear,
  onSelectYear,
  onClose,
}: RepaymentScheduleSheetProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [expandedYear, setExpandedYear] = useState<number | false | null>(null);
  const visibleExpandedYear = expandedYear === null
    ? activeYear
    : expandedYear === false
      ? null
      : expandedYear;
  const maxInterest = Math.max(1, ...years.map((year) => year.interestPaid));
  const maxPrincipal = Math.max(1, ...years.map((year) => year.principalPaid));

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) {
      dialog.showModal();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  }, [activeYear, open]);

  function toggleYear(year: number) {
    setExpandedYear((current) => {
      const currentlyExpanded = current === null ? activeYear : current;
      return currentlyExpanded === year ? false : year;
    });
    onSelectYear(year);
  }

  function closeSheet() {
    setExpandedYear(null);
    onClose();
  }

  function downloadCsv() {
    const csv = repaymentScheduleCsv(years);
    const url = URL.createObjectURL(new Blob([csv], { type: "text/csv;charset=utf-8" }));
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `repayment-schedule-${model.strategy}.csv`;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  return (
    <dialog
      ref={dialogRef}
      className="home-plan-schedule-sheet"
      aria-labelledby="repayment-schedule-title"
      onClose={closeSheet}
      onClick={(event) => {
        if (event.target === event.currentTarget) closeSheet();
      }}
    >
      <div className="home-plan-schedule-sheet__surface">
        <header className="home-plan-schedule-sheet__header">
          <span>Repayment audit</span>
          <h2 id="repayment-schedule-title">Yearly repayment schedule</h2>
          <p>Exact annual values for the selected plan. Expand a year to inspect its monthly payments.</p>
          <button type="button" aria-label="Close repayment schedule" onClick={closeSheet}>×</button>
        </header>

        <dl className="home-plan-schedule-summary">
          <div>
            <dt>Original payoff</dt>
            <dd>{durationLabel(model.baselinePayoffMonths)}</dd>
          </div>
          <span aria-hidden="true">→</span>
          <div>
            <dt>Selected payoff</dt>
            <dd>{durationLabel(model.selectedPayoffMonths)}</dd>
          </div>
          <div className="is-saved">
            <dt>Interest avoided</dt>
            <dd>{model.comparisonAvailable ? formatCurrency(model.interestSaved, true) : "Not comparable"}</dd>
          </div>
        </dl>

        <div className="home-plan-schedule-table-wrap">
          <table className="home-plan-schedule-table">
            <thead>
              <tr>
                <th>Year</th>
                <th>Total paid</th>
                <th>Interest</th>
                <th>Principal</th>
                <th>Extra</th>
                <th>Closing balance</th>
                <th>Interest avoided</th>
              </tr>
            </thead>
            <tbody>
              {years.map((year) => (
                <Fragment key={year.year}>
                  <AnnualRow
                    year={year}
                    model={model}
                    expanded={visibleExpandedYear === year.year}
                    active={activeYear === year.year}
                    maxInterest={maxInterest}
                    maxPrincipal={maxPrincipal}
                    onToggle={() => toggleYear(year.year)}
                  />
                  {visibleExpandedYear === year.year ? <MobileYearDetails year={year} /> : null}
                  {visibleExpandedYear === year.year
                    ? year.months.map((month) => (
                      <MonthlyRow key={month.paymentNumber} month={month} />
                    ))
                    : null}
                </Fragment>
              ))}
            </tbody>
          </table>
        </div>

        <footer className="home-plan-schedule-sheet__footer">
          <p>
            Indicative model for a {formatCurrency(inputs.propertyPriceLakh * 100_000, true)} home.
            Your lender’s schedule may differ due to dates, rate changes, fees or rounding.
          </p>
          <button type="button" className="is-download" onClick={downloadCsv}>⇩ Download CSV</button>
          <button type="button" onClick={closeSheet}>Done</button>
        </footer>
      </div>
    </dialog>
  );
}
