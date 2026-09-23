import { useEffect, useMemo, useState } from 'react';
import { pointAlongRoute, type AtlasPoint, type AtlasRoute } from '../../lib/atlas/journey.ts';

type Position = AtlasPoint & { heading?: number };

/** A fixed north-up frame; camera movement never rotates or rescales the road. */
export default function AtlasRoadOverview({ route, home, streetView, flightPosition, streetPosition }: {
  route: AtlasRoute;
  home: AtlasPoint;
  streetView: boolean;
  flightPosition: () => number;
  streetPosition: () => (AtlasPoint & { heading: number }) | null;
}) {
  const [position, setPosition] = useState<Position | null>(null);
  useEffect(() => {
    // Only this small overview updates, not the 3D scene. Read actual panorama
    // coordinates in Street View; the aerial dot denotes the tour's road position.
    const timer = window.setInterval(() => {
      const next: Position | null = streetView ? streetPosition() : pointAlongRoute(route, flightPosition());
      setPosition(previous => previous?.latitude === next?.latitude
        && previous?.longitude === next?.longitude && previous?.heading === next?.heading
        ? previous : next);
    }, 250);
    return () => window.clearInterval(timer);
  }, [route, streetView, flightPosition, streetPosition]);

  const frame = useMemo(() => {
    const longitudeScale = Math.cos(home.latitude * Math.PI / 180);
    const points = [...route.coordinates, [home.longitude, home.latitude]];
    const xs = points.map(point => point[0] * longitudeScale);
    const ys = points.map(point => -point[1]);
    const minX = Math.min(...xs), maxX = Math.max(...xs);
    const minY = Math.min(...ys), maxY = Math.max(...ys);
    const scale = Math.min(104 / Math.max(maxX - minX, 1e-8), 68 / Math.max(maxY - minY, 1e-8));
    const project = (point: AtlasPoint) => ({
      x: 76 + (point.longitude * longitudeScale - (minX + maxX) / 2) * scale,
      y: 58 + (-point.latitude - (minY + maxY) / 2) * scale,
    });
    return {
      project,
      home: project(home),
      road: route.coordinates.map(([longitude, latitude]) => {
        const point = project({ longitude, latitude });
        return `${point.x},${point.y}`;
      }).join(' '),
    };
  }, [route, home]);
  const current = position && frame.project(position);
  // Do not snap an off-road panorama onto the road or to the overview's edge.
  const visible = current && current.x >= 8 && current.x <= 144 && current.y >= 8 && current.y <= 108;

  return (
    <svg className="atlas-road-overview" viewBox="0 0 152 116" role="img"
      aria-label="Road overview, north up. Home is marked with a house; the blue marker shows the tour position.">
      <text className="atlas-road-overview__north" x="140" y="15">N</text>
      <polyline points={frame.road} fill="none" stroke="white" strokeWidth="6" strokeLinejoin="round" />
      <polyline points={frame.road} fill="none" stroke="currentColor" strokeWidth="3" strokeLinejoin="round" />
      <g transform={`translate(${frame.home.x} ${frame.home.y})`}>
        <circle r="10" fill="#f4f6f1" />
        <path d="M-6 0 0-5 6 0 M-4-1 V5 H4 V-1" fill="none" stroke="currentColor" strokeWidth="1.8" />
        <text className="atlas-road-overview__home" y="23" textAnchor="middle">Home</text>
      </g>
      {visible && <g transform={`translate(${current.x} ${current.y})`}>
        {position.heading !== undefined && <path d="M0-13 -6-4 6-4Z" fill="#2473c7"
          transform={`rotate(${position.heading})`} />}
        <circle r="5" fill="#2473c7" stroke="white" strokeWidth="2" />
      </g>}
    </svg>
  );
}
