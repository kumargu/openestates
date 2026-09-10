"use client";
import { useEffect, useRef, useState } from "react";
import type {
  Dimension,
  HomePlan,
  PreparedHome,
  TourFrame,
} from "../core/types.ts";
import { prepareHome } from "../core/tour.ts";
import { bounds } from "../core/geometry.ts";
import type { HomeViewer, ViewerState, ViewMode } from "../renderer/viewer.ts";
const initial: TourFrame = {
  position: [0, 0],
  heading: 0,
  pitch: 0,
  phase: "entrance",
  roomId: null,
  stopIndex: 0,
  viewIndex: 0,
  progress: 0,
  showDimensions: false,
};
function Plan({
  plan,
  frame,
  select,
  mini = false,
  dimensions = [],
}: {
  plan: HomePlan;
  frame: TourFrame;
  select: (id: string) => void;
  mini?: boolean;
  dimensions?: readonly Dimension[];
}) {
  const b = bounds(plan.rooms.flatMap((r) => [...r.polygon]));
  return (
    <svg
      className={mini ? "he-mini" : "he-plan"}
      viewBox={`${b.min[0] - 0.6} ${b.min[1] - 0.6} ${b.max[0] - b.min[0] + 1.2} ${b.max[1] - b.min[1] + 1.2}`}
      aria-label="Floor plan and your position"
    >
      {plan.rooms.map((r) => (
        <g
          key={r.id}
          role="button"
          tabIndex={0}
          aria-label={`Visit ${r.name}`}
          onClick={() => select(r.id)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              select(r.id);
            }
          }}
        >
          <polygon
            points={r.polygon.map((p) => p.join(",")).join(" ")}
            fill={
              frame.roomId === r.id
                ? "#a4c4b5"
                : r.kind === "balcony"
                  ? "#dde6dc"
                  : "#f4f1e9"
            }
            stroke="#c0c7c1"
            strokeWidth=".035"
          />
        </g>
      ))}
      {plan.walls.map((w) => (
        <path
          key={w.id}
          d={
            w.opening && w.opening.kind !== "window"
              ? `M${w.a.join(",")}L${[w.a[0] + (w.b[0] - w.a[0]) * w.opening.start, w.a[1] + (w.b[1] - w.a[1]) * w.opening.start].join(",")}M${[w.a[0] + (w.b[0] - w.a[0]) * w.opening.end, w.a[1] + (w.b[1] - w.a[1]) * w.opening.end].join(",")}L${w.b.join(",")}`
              : `M${w.a.join(",")}L${w.b.join(",")}`
          }
          stroke="#687b72"
          strokeWidth=".08"
          pointerEvents="none"
        />
      ))}
      {dimensions.map((d) => (
        <line
          key={d.label}
          x1={d.a[0]}
          y1={d.a[1]}
          x2={d.b[0]}
          y2={d.b[1]}
          stroke="#176b5e"
          strokeWidth=".07"
          strokeDasharray=".12 .07"
          pointerEvents="none"
        />
      ))}
      <g
        transform={`translate(${frame.position.join(" ")}) rotate(${(-frame.heading * 180) / Math.PI})`}
        pointerEvents="none"
      >
        <path d="M0 0L-.28 .6Q0 .8 .28 .6Z" fill="#268476" opacity=".25" />
        <circle r=".12" fill="#176b5e" stroke="white" strokeWidth=".05" />
      </g>
    </svg>
  );
}
/** Framework-neutral shell. Supply structured geometry; import the accompanying CSS. */
export function HomeExplorer({ plan }: { plan: HomePlan }) {
  const host = useRef<HTMLDivElement>(null),
    viewer = useRef<HomeViewer | null>(null),
    source = useRef<HTMLDialogElement>(null);
  const [home, setHome] = useState<PreparedHome | null>(null),
    [state, setState] = useState<ViewerState>({
      frame: initial,
      playing: false,
      mode: "overview",
    });
  const [error, setError] = useState(""),
    [ready, setReady] = useState(false),
    [roomsOpen, setRoomsOpen] = useState(false),
    [dimensions, setDimensions] = useState(true),
    [cutaway, setCutaway] = useState(true),
    [speed, setSpeed] = useState("1");
  useEffect(() => {
    let cancelled = false;
    setReady(false);
    setError("");
    setHome(null);
    setSpeed("1");
    setDimensions(true);
    setCutaway(true);
    const timer = setTimeout(async () => {
      try {
        const prepared = prepareHome(plan);
        if (cancelled) return;
        setHome(prepared);
        const { HomeViewer } = await import("../renderer/viewer.ts");
        if (cancelled || !host.current) return;
        viewer.current = new HomeViewer(host.current, prepared, setState);
        setReady(true);
      } catch (e) {
        if (!cancelled) {
          setError((e as Error).message);
          setState((s) => ({ ...s, mode: "plan" }));
        }
      }
    }, 30);
    return () => {
      cancelled = true;
      clearTimeout(timer);
      viewer.current?.dispose();
      viewer.current = null;
    };
  }, [plan]);
  const f = state.frame,
    room = plan.rooms.find(
      (r) => r.id === (state.mode === "walk" ? f.roomId : state.selectedRoomId),
    ),
    stop = home?.stops[f.stopIndex];
  const select = (id: string) => {
    const i = home?.stops.findIndex((s) => s.roomId === id) ?? -1;
    if (i >= 0 && viewer.current) {
      try {
        viewer.current.select(i);
        setRoomsOpen(false);
      } catch (e) {
        setError((e as Error).message);
      }
    }
  };
  const mode = (m: ViewMode) => viewer.current?.setMode(m);
  const status =
    state.mode === "overview"
      ? "Explore the layout"
      : f.phase === "complete"
        ? "You’ve explored this home"
        : state.playing
          ? f.phase === "walking"
            ? "Walking to " +
              plan.rooms.find((r) => r.id === stop?.roomId)?.name
            : f.phase === "inspecting"
              ? stop?.views[f.viewIndex]?.purpose === "connection"
                ? "Back toward the doorway"
                : stop?.views[f.viewIndex]?.purpose === "length"
                  ? "The length of the room"
                  : "Take a moment. Look around."
              : f.phase === "entrance"
                ? "Your tour begins at the front door"
                : "Arriving"
          : "Your pace. Your home tour.";
  return (
    <main className="he-root">
      <header className="he-header">
        <div className="he-brand">
          HOME <span>ATLAS</span>
          <small>INTERIORS</small>
        </div>
        <div className="he-property">
          <strong>{plan.name}</strong>
          <span>{plan.unit}</span>
        </div>
        <button
          className="he-source-button"
          onClick={() => source.current?.showModal()}
        >
          About this plan ↗
        </button>
      </header>
      <section
        className={`he-stage he-${state.mode}`}
        aria-label="Interactive apartment tour"
      >
        <div
          ref={host}
          tabIndex={0}
          className="he-canvas"
          aria-label="3D home. Drag to look. Arrow keys walk. Space pauses."
          style={{ visibility: state.mode === "plan" ? "hidden" : "visible" }}
        />
        {state.mode === "plan" && (
          <Plan plan={plan} frame={f} select={select} />
        )}
        <div className="he-topbar">
          <button
            className="he-pill"
            aria-expanded={roomsOpen}
            onClick={() => setRoomsOpen(!roomsOpen)}
          >
            ☰ <span>Rooms</span>
            <small>{plan.rooms.length}</small>
          </button>
          <div className="he-segments" aria-label="View mode">
            {(["overview", "walk", "plan"] as const).map((m) => (
              <button
                key={m}
                disabled={!ready}
                aria-pressed={state.mode === m}
                onClick={() => mode(m)}
              >
                {m === "overview"
                  ? "Dollhouse"
                  : m === "walk"
                    ? "Inside"
                    : "Floor plan"}
              </button>
            ))}
          </div>
        </div>
        {roomsOpen && (
          <nav className="he-room-list" aria-label="Tour stops">
            {(
              home?.stops.map(
                (s) => plan.rooms.find((r) => r.id === s.roomId)!,
              ) ?? plan.rooms
            ).map((r, i) => (
              <button
                key={r.id}
                disabled={!ready}
                aria-current={f.roomId === r.id ? "step" : undefined}
                onClick={() => select(r.id)}
              >
                <small>{String(i + 1).padStart(2, "0")}</small>
                {r.name}
                <span>↗</span>
              </button>
            ))}
          </nav>
        )}
        {!ready && !error && (
          <div className="he-loading" role="status">
            Preparing your home…
          </div>
        )}
        {error && (
          <div className="he-error" role="alert">
            <strong>The guided view couldn’t open.</strong>
            <p>The floor plan remains available.</p>
            <details>
              <summary>Details</summary>
              {error}
            </details>
          </div>
        )}
        <div className="he-caption">
          <p className="he-eyebrow">{status}</p>
          <h1>
            {state.mode === "walk"
              ? (room?.name ?? "Welcome home")
              : (room?.name ?? "Your home, room by room")}
          </h1>
          <p>
            {state.mode === "walk"
              ? room?.sourceDimensions
                ? `On the source plan · ${room.sourceDimensions}`
                : "Step inside, at your own pace."
              : (room?.sourceDimensions ?? "")}
          </p>
          {state.mode === "walk" &&
            room &&
            dimensions &&
            (f.showDimensions || !state.playing) && (
              <div className="he-room-dimensions">
                {home?.stops
                  .find((s) => s.roomId === room.id)
                  ?.dimensions.map((d) => (
                    <span key={d.label}>
                      {d.label} <strong>≈ {d.metres.toFixed(2)} m</strong>
                    </span>
                  ))}
                <small>Approximate model spans</small>
              </div>
            )}
        </div>
        {state.mode !== "plan" && (
          <div className="he-map">
            <Plan
              plan={plan}
              frame={
                state.mode === "walk"
                  ? f
                  : { ...f, roomId: state.selectedRoomId ?? null }
              }
              select={select}
              mini
              dimensions={
                dimensions && f.showDimensions ? stop?.dimensions : []
              }
            />
            <span>
              {state.mode === "walk" ? "YOU ARE HERE" : "THE FLOOR PLAN"}
            </span>
          </div>
        )}
        {state.mode === "walk" && (
          <div className="he-walk-pad" aria-label="Walk manually">
            <button
              aria-label="Walk forward"
              onPointerDown={(e) => {
                e.currentTarget.setPointerCapture(e.pointerId);
                viewer.current?.move(1, 0);
              }}
              onPointerUp={() => viewer.current?.move(0, 0)}
              onPointerCancel={() => viewer.current?.move(0, 0)}
              onLostPointerCapture={() => viewer.current?.move(0, 0)}
              onKeyDown={(e) => {
                if (e.key === " " || e.key === "Enter")
                  viewer.current?.move(1, 0);
              }}
              onKeyUp={() => viewer.current?.move(0, 0)}
              onBlur={() => viewer.current?.move(0, 0)}
            >
              ↑
            </button>
            <div>
              <button
                aria-label="Step left"
                onPointerDown={(e) => {
                  e.currentTarget.setPointerCapture(e.pointerId);
                  viewer.current?.move(0, -1);
                }}
                onPointerUp={() => viewer.current?.move(0, 0)}
                onPointerCancel={() => viewer.current?.move(0, 0)}
                onLostPointerCapture={() => viewer.current?.move(0, 0)}
                onKeyDown={(e) => {
                  if (e.key === " " || e.key === "Enter")
                    viewer.current?.move(0, -1);
                }}
                onKeyUp={() => viewer.current?.move(0, 0)}
                onBlur={() => viewer.current?.move(0, 0)}
              >
                ←
              </button>
              <button
                aria-label="Walk backward"
                onPointerDown={(e) => {
                  e.currentTarget.setPointerCapture(e.pointerId);
                  viewer.current?.move(-1, 0);
                }}
                onPointerUp={() => viewer.current?.move(0, 0)}
                onPointerCancel={() => viewer.current?.move(0, 0)}
                onLostPointerCapture={() => viewer.current?.move(0, 0)}
                onKeyDown={(e) => {
                  if (e.key === " " || e.key === "Enter")
                    viewer.current?.move(-1, 0);
                }}
                onKeyUp={() => viewer.current?.move(0, 0)}
                onBlur={() => viewer.current?.move(0, 0)}
              >
                ↓
              </button>
              <button
                aria-label="Step right"
                onPointerDown={(e) => {
                  e.currentTarget.setPointerCapture(e.pointerId);
                  viewer.current?.move(0, 1);
                }}
                onPointerUp={() => viewer.current?.move(0, 0)}
                onPointerCancel={() => viewer.current?.move(0, 0)}
                onLostPointerCapture={() => viewer.current?.move(0, 0)}
                onKeyDown={(e) => {
                  if (e.key === " " || e.key === "Enter")
                    viewer.current?.move(0, 1);
                }}
                onKeyUp={() => viewer.current?.move(0, 0)}
                onBlur={() => viewer.current?.move(0, 0)}
              >
                →
              </button>
            </div>
          </div>
        )}
        <div className="he-bottom">
          <div className="he-controls">
            {state.mode === "walk" && (
              <button
                className="he-icon"
                disabled={!ready || f.stopIndex === 0}
                aria-label="Previous room"
                onClick={() =>
                  viewer.current?.select(Math.max(0, f.stopIndex - 1))
                }
              >
                ‹
              </button>
            )}
            <button
              className="he-play"
              disabled={!ready}
              onClick={() =>
                state.mode === "walk"
                  ? viewer.current?.toggle()
                  : viewer.current?.start()
              }
            >
              {state.playing
                ? "Ⅱ Pause tour"
                : f.phase === "complete"
                  ? "↻ Tour again"
                  : state.mode === "walk"
                    ? "▶ Continue tour"
                    : "▶ Enter the home"}
            </button>
            {state.mode === "walk" && (
              <button
                className="he-icon"
                disabled={
                  !ready || f.stopIndex === (home?.stops.length ?? 1) - 1
                }
                aria-label="Next room"
                onClick={() => viewer.current?.select(f.stopIndex + 1)}
              >
                ›
              </button>
            )}
            <div className="he-control-divider" />
            {state.mode === "overview" && (
              <>
                <button
                  aria-pressed={cutaway}
                  onClick={() => {
                    setCutaway(!cutaway);
                    viewer.current?.setCutaway(!cutaway);
                  }}
                >
                  Cutaway
                </button>
                {state.selectedRoomId && (
                  <button onClick={() => viewer.current?.resetOverview()}>
                    Whole home
                  </button>
                )}
              </>
            )}
            <label className="he-speed">
              Pace{" "}
              <select
                value={speed}
                onChange={(e) => {
                  setSpeed(e.target.value);
                  if (viewer.current)
                    viewer.current.tour.rate = Number(e.target.value);
                }}
              >
                <option value="0.65">Unhurried</option>
                <option value="1">Natural</option>
                <option value="1.25">Brisk</option>
              </select>
            </label>
            <button
              className="he-dimensions"
              aria-pressed={dimensions}
              onClick={() => {
                setDimensions(!dimensions);
                if (viewer.current) viewer.current.dimensions = !dimensions;
              }}
            >
              ↔ <span>Measurements</span>
            </button>
          </div>
          <p className="he-hint">
            {state.mode === "walk"
              ? "Drag to look · Arrow keys to walk · Pause anywhere"
              : "Drag to orbit · Scroll to zoom"}
          </p>
        </div>
        <div
          className="he-progress"
          role="progressbar"
          aria-label="Tour progress"
          aria-valuenow={Math.round(f.progress * 100)}
          aria-valuemin={0}
          aria-valuemax={100}
        >
          <span style={{ width: `${f.progress * 100}%` }} />
        </div>
      </section>
      <footer className="he-footer">
        <span>
          {plan.source.status === "validated"
            ? "Validated floor geometry"
            : plan.source.status === "synthetic"
              ? "Illustrative floor plan"
              : "Approximate floor-plan reconstruction"}
        </span>
        <span>
          Empty interiors ·{" "}
          {plan.architecture.heightsVerified
            ? "Source heights"
            : "Illustrative heights & finishes"}
        </span>
      </footer>
      <dialog ref={source} className="he-dialog">
        <button
          className="he-close"
          aria-label="Close source plan"
          onClick={() => source.current?.close()}
        >
          ×
        </button>
        <p className="he-eyebrow">THE SOURCE OF THE SPACE</p>
        <h2>{plan.source.label}</h2>
        <p>
          This tour follows structured room boundaries and doorway connections.{" "}
          {plan.source.status === "traced"
            ? "This sample is manually traced from a brochure and has not been architecturally validated."
            : ""}{" "}
          Printed dimensions and approximate model spans are shown separately.
          For irregular rooms, the two lines measure spans through a central
          reference point. They remain fixed as you move around.
        </p>
        <p>
          Finishes, light and unverified ceiling heights are illustrative. No
          compass direction or outside view is inferred. This is a spatial
          preview, not a surveyed digital twin.
        </p>
        {plan.source.image && (
          <img src={plan.source.image} alt="Original apartment floor plan" />
        )}
        {plan.source.href && (
          <a href={plan.source.href} target="_blank" rel="noreferrer">
            Open source document ↗
          </a>
        )}
      </dialog>
    </main>
  );
}
