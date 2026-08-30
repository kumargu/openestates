import { type KeyboardEvent, type ReactNode } from "react";
import {
  nearestIndex,
  type ChartInsets,
} from "./chartGeometry.ts";

type ScrubbableSvgProps = {
  width: number;
  height: number;
  insets: ChartInsets;
  pointCount: number;
  activeIndex: number;
  label: string;
  className?: string;
  columns?: number;
  indexFromPoint?: (
    clientX: number,
    clientY: number,
    bounds: DOMRect,
  ) => number;
  children: ReactNode;
  onPreviewIndex: (index: number | null) => void;
  onPinIndex: (index: number) => void;
};

export function ScrubbableSvg({
  width,
  height,
  insets,
  pointCount,
  activeIndex,
  label,
  className,
  columns = 1,
  indexFromPoint,
  children,
  onPreviewIndex,
  onPinIndex,
}: ScrubbableSvgProps) {
  const indexFromPointer = (event: {
    clientX: number;
    clientY: number;
    currentTarget: SVGSVGElement;
  }) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    const index = indexFromPoint
      ? indexFromPoint(event.clientX, event.clientY, bounds)
      : nearestIndex(event.clientX, bounds, width, insets, pointCount);
    return Math.max(0, Math.min(pointCount - 1, index));
  };
  const moveByKeyboard = (event: KeyboardEvent<SVGSVGElement>) => {
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const next = event.key === "Home"
      ? 0
      : event.key === "End"
        ? Math.max(0, pointCount - 1)
        : Math.max(
          0,
          Math.min(
            pointCount - 1,
              activeIndex + (
                event.key === "ArrowLeft"
                  ? -1
                  : event.key === "ArrowRight"
                    ? 1
                    : event.key === "ArrowUp"
                      ? -columns
                      : columns
              ),
          ),
        );
    onPreviewIndex(null);
    onPinIndex(next);
  };

  return (
    <svg
      className={`home-plan-chart ${className ?? ""}`}
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label={label}
      tabIndex={0}
      onKeyDown={moveByKeyboard}
      onPointerMove={(event) => onPreviewIndex(indexFromPointer(event))}
      onPointerLeave={() => onPreviewIndex(null)}
      onClick={(event) => onPinIndex(indexFromPointer(event))}
    >
      {children}
    </svg>
  );
}

export function ChartHeading({
  title,
  conclusion,
}: {
  title: string;
  conclusion?: string;
}) {
  return (
    <header className="home-plan-chart-heading">
      <h2>{title}</h2>
      {conclusion ? <p>{conclusion}</p> : null}
    </header>
  );
}

export function ChartReadout({
  children,
  columns = 3,
}: {
  children: ReactNode;
  columns?: number;
}) {
  return (
    <dl
      className={`home-plan-chart-readout home-plan-chart-readout--${Math.max(2, Math.min(5, columns))}`}
      aria-live="polite"
      aria-atomic="true"
    >
      {children}
    </dl>
  );
}

export function ReadoutValue({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone?: "buy" | "rent" | "interest" | "principal";
}) {
  return (
    <div className={tone ? `is-${tone}` : undefined}>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

export function ChartAnnotation({
  x,
  top,
  bottom,
  label,
  align = "middle",
}: {
  x: number;
  top: number;
  bottom: number;
  label: string;
  align?: "start" | "middle" | "end";
}) {
  return (
    <g className="home-plan-chart-annotation" aria-hidden="true">
      <line x1={x} x2={x} y1={top} y2={bottom} />
      <text x={x} y={top - 8} textAnchor={align}>{label}</text>
    </g>
  );
}
