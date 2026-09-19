import policy from './atlasPolicy.ts';

type Point = { lat: number; lng: number };
export type SpotlightSubject = { anchor: Point; extent?: Point[]; footprints?: Point[][]; emphasis: 'home' | 'place' | 'selected' | 'road' };
type Polygon = HTMLElement & { path: Point[]; innerPaths: Point[][]; strokeColor: string };
type SpotlightMap = HTMLElement & { range: number; tilt: number; fov?: number };
type PolygonOptions = { fillColor: string; strokeColor: string; strokeWidth: number };

export function spotlightRadiusM(range: number, height: number, tilt: number,
  emphasis: SpotlightSubject['emphasis'], fieldOfViewDegrees = policy.cameraFit.fieldOfViewDegrees) {
  const p = policy.spotlight;
  const pixels = emphasis === 'selected' ? p.selectedRadiusPx
    : emphasis === 'home' ? p.homeRadiusPx : p.overviewRadiusPx;
  const metresPerPixel = 2 * range * Math.tan(fieldOfViewDegrees * Math.PI / 360) / Math.max(1, height);
  return Math.max(p.minimumRadiusM, pixels * metresPerPixel
    / Math.sqrt(Math.max(p.minimumTiltCosine, Math.cos(tilt * Math.PI / 180))));
}

export function spotlightCircle(anchor: Point, radiusM: number): Point[] {
  return Array.from({ length: 49 }, (_, i) => {
    const angle = i / 48 * Math.PI * 2;
    return { lat: anchor.lat + Math.sin(angle) * radiusM / 111_320,
      lng: anchor.lng + Math.cos(angle) * radiusM / (111_320 * Math.cos(anchor.lat * Math.PI / 180)) };
  });
}

/** An illumination envelope, never presented as a sourced site boundary. */
export function spotlightHull(points: Point[]): Point[] {
  const sorted = [...points].sort((a, b) => a.lng - b.lng || a.lat - b.lat);
  const cross = (o: Point, a: Point, b: Point) => (a.lng - o.lng) * (b.lat - o.lat) - (a.lat - o.lat) * (b.lng - o.lng);
  const half = (input: Point[]) => {
    const hull: Point[] = [];
    for (const point of input) {
      while (hull.length > 1 && cross(hull[hull.length - 2], hull[hull.length - 1], point) <= 0) hull.pop();
      hull.push(point);
    }
    return hull.slice(0, -1);
  };
  const hull = [...half(sorted), ...half([...sorted].reverse())];
  return hull.length ? [...hull, hull[0]] : points;
}

/** Merge intersecting envelopes so polygon holes cannot cancel each other out. */
export function mergeSpotlightOpenings(paths: Point[][]): Point[][] {
  const bounds = (path: Point[]) => ({ left: Math.min(...path.map(p => p.lng)), right: Math.max(...path.map(p => p.lng)),
    bottom: Math.min(...path.map(p => p.lat)), top: Math.max(...path.map(p => p.lat)) });
  const groups = paths.filter(p => p.length >= 3).map(p => [...p]);
  for (let i = 0; i < groups.length; i++) {
    for (let j = i + 1; j < groups.length; j++) {
      const a = bounds(groups[i]), b = bounds(groups[j]);
      if (a.left > b.right || b.left > a.right || a.bottom > b.top || b.bottom > a.top) continue;
      groups[i] = spotlightHull([...groups[i], ...groups[j]]);
      groups.splice(j, 1);
      i = -1;
      break;
    }
  }
  return groups;
}

/** Sourced area shapes keep their shoreline and separate parts at every range. */
export function spotlightPaths(subject: SpotlightSubject, range: number, height: number, tilt: number, scale: number,
  fieldOfViewDegrees = policy.cameraFit.fieldOfViewDegrees): Point[][] {
  if (subject.footprints?.length) return subject.footprints;
  // Quiet surroundings leaves the mapped home itself clear at every zoom.
  // Missing boundaries retain the point fallback; lake footprints stay exact.
  if (subject.emphasis === 'home' && subject.extent?.length) return [subject.extent];
  if (subject.emphasis === 'road' && subject.extent?.length) return [subject.extent.map(point => ({
    lat: subject.anchor.lat + (point.lat - subject.anchor.lat) * scale,
    lng: subject.anchor.lng + (point.lng - subject.anchor.lng) * scale,
  }))];
  const radius = spotlightRadiusM(range, height, tilt, subject.emphasis, fieldOfViewDegrees) * scale;
  const extent = (subject.extent ?? []).map(point => ({
    lat: subject.anchor.lat + (point.lat - subject.anchor.lat) * policy.spotlight.extentPaddingScale * scale,
    lng: subject.anchor.lng + (point.lng - subject.anchor.lng) * policy.spotlight.extentPaddingScale * scale,
  }));
  return [spotlightHull([...spotlightCircle(subject.anchor, radius), ...extent])];
}

/** Geographic feathered illumination follows the live lens, including manual zoom. */
export function attachAtlasSpotlight(map: SpotlightMap, subjects: SpotlightSubject[],
  createPolygon: (options: PolygonOptions) => Polygon, animate: boolean, veilFill = policy.spotlight.veilFill): () => void {
  const p = policy.spotlight;
  const veils = p.featherScales.map(() => createPolygon({ fillColor: veilFill, strokeColor: '#00000000', strokeWidth: 0 }));
  const halos = subjects.filter(s => !s.footprints?.length && (s.emphasis === 'place' || s.emphasis === 'selected')).map(subject => ({ subject,
    element: createPolygon({ fillColor: p.haloFill, strokeColor: p.haloStroke, strokeWidth: 2 }) }));
  const pulseSubject = subjects.find(s => s.emphasis === 'selected' && !s.footprints?.length);
  const pulse = pulseSubject && animate ? createPolygon({ fillColor: '#00000000', strokeColor: p.pulseStroke, strokeWidth: 2 }) : null;
  const elements = [...veils, ...halos.map(h => h.element), ...(pulse ? [pulse] : [])];
  veils.forEach(element => { element.dataset.atlasSpotlightKind = 'veil'; });
  halos.forEach(({element}) => { element.dataset.atlasSpotlightKind = 'halo'; });
  if (pulse) pulse.dataset.atlasSpotlightKind = 'pulse';
  for (const element of elements) { element.dataset.atlasSpotlight = 'true'; map.append(element); }
  const anchor = subjects[0]?.anchor;
  if (!anchor) { elements.forEach(e => e.remove()); return () => {}; }
  let frame = 0;
  let pulseStarted: number | null = null;
  let pulseFinished = !pulse;
  const pathsFor = (subject: SpotlightSubject, scale: number) =>
    spotlightPaths(subject, map.range, map.clientHeight, map.tilt, scale, map.fov);
  const draw = (time: number) => {
    frame = 0;
    const radius = Math.max(policy.quiet.radiusDegrees, map.range / 111_320 * 3,
      ...subjects.map(s => Math.max(Math.abs(s.anchor.lat - anchor.lat), Math.abs(s.anchor.lng - anchor.lng)) * 2));
    const outer = [ {lat: anchor.lat-radius, lng: anchor.lng-radius}, {lat: anchor.lat-radius, lng: anchor.lng+radius},
      {lat: anchor.lat+radius, lng: anchor.lng+radius}, {lat: anchor.lat+radius, lng: anchor.lng-radius}, {lat: anchor.lat-radius, lng: anchor.lng-radius} ];
    veils.forEach((veil, i) => {
      veil.path = outer;
      veil.innerPaths = mergeSpotlightOpenings(subjects.flatMap(s => pathsFor(s, p.featherScales[i]))).map(path => path.reverse());
    });
    halos.forEach(({subject, element}) => { element.path = pathsFor(subject, 1)[0]; });
    if (pulse && pulseSubject && !pulseFinished) {
      pulseStarted ??= time;
      const elapsed = time - pulseStarted;
      const progress = (elapsed % p.pulseDurationMs) / p.pulseDurationMs;
      pulse.path = pathsFor(pulseSubject, 1 + progress * 0.3)[0];
      pulse.strokeColor = `${p.pulseStroke}${Math.round((1-progress)*150).toString(16).padStart(2,'0')}`;
      pulseFinished = elapsed >= p.pulseDurationMs * p.pulseCount;
      if (pulseFinished) pulse.remove();
      else frame = requestAnimationFrame(draw);
    }
  };
  const schedule = () => { if (!frame) frame = requestAnimationFrame(draw); };
  const events = ['gmp-rangechange', 'gmp-tiltchange', 'gmp-fovchange'];
  events.forEach(event => map.addEventListener(event, schedule));
  const observer = new ResizeObserver(schedule);
  observer.observe(map);
  schedule();
  return () => { cancelAnimationFrame(frame); observer.disconnect(); events.forEach(event => map.removeEventListener(event, schedule)); elements.forEach(e => e.remove()); };
}
