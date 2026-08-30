import { formatCurrency, type PlanProjection } from "./model.ts";
import {
  ChartAnnotation,
  ChartHeading,
  ChartReadout,
  ReadoutValue,
  ScrubbableSvg,
} from "./charts/ChartPrimitives.tsx";
import {
  chartTickIndexes,
  linearScale,
  paddedExtent,
  smoothLinePath,
} from "./charts/chartGeometry.ts";

const WIDTH = 900;
const HEIGHT = 350;
const INSETS = { top: 30, right: 42, bottom: 42, left: 70 };

type PlanGraphProps = {
  projection: PlanProjection;
  activeYear: number;
  onPreviewYearChange: (year: number | null) => void;
  onPinYear: (year: number) => void;
};

function rateLabel(value: number): string {
  return `${Number(value.toFixed(1)).toLocaleString("en-IN")}%`;
}

export function PlanGraph({
  projection,
  activeYear,
  onPreviewYearChange,
  onPinYear,
}: PlanGraphProps) {
  const points = projection.points;
  const boundedYear = Math.max(0, Math.min(activeYear, points.length - 1));
  const active = points[boundedYear];
  const maximumYear = Math.max(1, points.length - 1);
  const [minimumValue, maximumValue] = paddedExtent(
    points.flatMap((point) => [point.buyNetWorth, point.rentNetWorth]),
    0.08,
    true,
  );
  const plotBottom = HEIGHT - INSETS.bottom;
  const x = linearScale([0, maximumYear], [INSETS.left, WIDTH - INSETS.right]);
  const y = linearScale([minimumValue, maximumValue], [plotBottom, INSETS.top]);
  const buyPath = points.map((point) => ({ x: x.map(point.year), y: y.map(point.buyNetWorth) }));
  const rentPath = points.map((point) => ({ x: x.map(point.year), y: y.map(point.rentNetWorth) }));
  const buyLeads = active.buyNetWorth >= active.rentNetWorth;
  const advantage = Math.abs(active.buyNetWorth - active.rentNetWorth);
  const guideValues = [minimumValue, (minimumValue + maximumValue) / 2, maximumValue];

  return (
    <section className="home-plan-story home-plan-rent-graph">
      <ChartHeading
        title={`${buyLeads ? "Buying" : "Renting"} leads by ${formatCurrency(advantage, true)} in year ${boundedYear}`}
        conclusion={`${rateLabel(projection.assumptions.homeAppreciationRate)} home appreciation · ${rateLabel(projection.assumptions.rentInflationRate)} yearly rent increase`}
      />
      <ScrubbableSvg
        width={WIDTH}
        height={HEIGHT}
        insets={INSETS}
        pointCount={points.length}
        activeIndex={boundedYear}
        label={`Projected buy and rent wealth over ${maximumYear} years`}
        className="home-plan-rent-vs-buy-chart"
        onPreviewIndex={onPreviewYearChange}
        onPinIndex={onPinYear}
      >
        {guideValues.map((value) => (
          <g key={value} className="home-plan-value-guide" aria-hidden="true">
            <line x1={INSETS.left} x2={WIDTH - INSETS.right} y1={y.map(value)} y2={y.map(value)} />
            <text x={INSETS.left - 9} y={y.map(value) + 4}>{formatCurrency(value, true)}</text>
          </g>
        ))}
        <path d={smoothLinePath(buyPath)} className="home-plan-curve is-buy" />
        <path d={smoothLinePath(rentPath)} className="home-plan-curve is-rent" />
        {projection.breakEvenYear != null && projection.breakEvenYear <= maximumYear ? (
          <ChartAnnotation
            x={x.map(projection.breakEvenYear)}
            top={INSETS.top}
            bottom={plotBottom}
            label="Crossover"
          />
        ) : null}
        {chartTickIndexes(points.length, 5).map((index) => (
          <text key={index} x={x.map(index)} y={HEIGHT - 10} className="home-plan-chart-axis">
            {index === 0 ? "Now" : `${index}y`}
          </text>
        ))}
        <g className="home-plan-chart-cursor">
          <line x1={x.map(boundedYear)} x2={x.map(boundedYear)} y1={INSETS.top} y2={plotBottom} />
          <circle cx={x.map(boundedYear)} cy={y.map(active.buyNetWorth)} r="4" className="is-buy" />
          <circle cx={x.map(boundedYear)} cy={y.map(active.rentNetWorth)} r="4" className="is-rent" />
        </g>
      </ScrubbableSvg>
      <div className="home-plan-line-legend" aria-hidden="true">
        <span className="is-buy">Buy</span>
        <span className="is-rent">Rent</span>
      </div>
      <ChartReadout columns={3}>
        <ReadoutValue label="Year" value={String(boundedYear)} />
        <ReadoutValue label="Buy wealth" value={formatCurrency(active.buyNetWorth, true)} tone="buy" />
        <ReadoutValue label="Rent wealth" value={formatCurrency(active.rentNetWorth, true)} tone="rent" />
      </ChartReadout>
    </section>
  );
}
