import type { PlanMilestone } from "./planFields.ts";

type TimeRailProps = {
  horizon: number;
  maxYear: number;
  milestones: PlanMilestone[];
  hintedMilestoneYear: number | null;
  onChange: (year: number) => void;
  onMilestonePress: (milestone: PlanMilestone) => void;
};

const QUICK_YEARS = [5, 10, 15, 20];

export function TimeRail({
  horizon,
  maxYear,
  milestones,
  hintedMilestoneYear,
  onChange,
  onMilestonePress,
}: TimeRailProps) {
  return (
    <div className="home-plan-time-rail" aria-label="Projection timeline">
      <div className="home-plan-time-rail__labels">
        <span>Now</span>
        <span>{maxYear} years</span>
      </div>
      <div className="home-plan-time-rail__track">
        <div
          className="home-plan-time-rail__fill"
          style={{ width: `${(horizon / maxYear) * 100}%` }}
        />
        <input
          type="range"
          className="home-plan-time-rail__input"
          min={0}
          max={maxYear}
          step={1}
          value={horizon}
          onChange={(event) => onChange(Number(event.target.value))}
          aria-label="Projection year"
        />
        {milestones.map((milestone) => (
          <button
            type="button"
            key={`${milestone.year}-${milestone.label}`}
            className={`home-plan-time-rail__milestone ${horizon === milestone.year ? "is-active" : ""} ${hintedMilestoneYear === milestone.year ? "is-hinted" : ""}`}
            style={{ left: `${(milestone.year / maxYear) * 100}%` }}
            onClick={() => onMilestonePress(milestone)}
            aria-label={`${milestone.label}, year ${milestone.year}`}
          >
            <i />
            <span>{milestone.label}</span>
          </button>
        ))}
      </div>
      <div className="home-plan-time-rail__quick">
        {QUICK_YEARS.filter((year) => year <= maxYear).map((year) => (
          <button
            type="button"
            key={year}
            className={horizon === year ? "is-active" : ""}
            onClick={() => onChange(year)}
          >
            {year}y
          </button>
        ))}
      </div>
    </div>
  );
}
