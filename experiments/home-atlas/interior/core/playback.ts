import type { PreparedHome, TourFrame, Vec2 } from "./types.ts";
import {
  angleDelta,
  alongPath,
  clamp,
  contains,
  distance,
  heading,
  pathLength,
  turnToward,
} from "./geometry.ts";
import { Navigator } from "./navigation.ts";

/** One clock owns travel, arrival, inspection and pause. No DOM, timers or renderer imports. */
export class HomeTour {
  readonly home: PreparedHome;
  readonly navigator: Navigator;
  frame: TourFrame;
  playing = false;
  rate = 1;
  reducedMotion = false;
  private stage:
    | "entrance"
    | "walking"
    | "settling"
    | "inspecting"
    | "complete" = "entrance";
  private seconds = 0;
  private travelled = 0;
  private route: readonly Vec2[];
  constructor(home: PreparedHome) {
    this.home = home;
    this.navigator = new Navigator(home.plan, home.policy);
    this.route = home.routes[0];
    this.frame = {
      position: home.entrance,
      heading: home.entranceHeading,
      pitch: 0,
      phase: "entrance",
      roomId: null,
      stopIndex: 0,
      progress: 0,
      showDimensions: false,
    };
  }
  reset() {
    this.playing = false;
    this.stage = "entrance";
    this.seconds = this.travelled = 0;
    this.route = this.home.routes[0];
    this.frame = {
      position: this.home.entrance,
      heading: this.home.entranceHeading,
      pitch: 0,
      phase: "entrance",
      roomId: null,
      stopIndex: 0,
      progress: 0,
      showDimensions: false,
    };
  }
  play() {
    if (this.stage === "complete") this.reset();
    this.playing = true;
    this.frame.phase = this.stage;
  }
  pause() {
    this.playing = false;
    this.frame.phase = "paused";
  }
  select(index: number) {
    const target = clamp(index, 0, this.home.stops.length - 1);
    const route = this.navigator.route(
      this.frame.position,
      this.home.stops[target].point,
    );
    this.frame.stopIndex = target;
    this.frame.showDimensions = false;
    this.route = route;
    this.seconds = this.travelled = 0;
    this.stage = "walking";
    this.playing = true;
  }
  look(delta: number, pitchDelta = 0) {
    this.pause();
    this.frame.heading += delta;
    this.frame.pitch = clamp(this.frame.pitch + pitchDelta, -0.85, 0.85);
  }
  move(forward: number, right: number, dt: number) {
    if (!forward && !right) return;
    this.pause();
    this.frame.phase = "exploring";
    this.frame.showDimensions = false;
    const h = this.frame.heading,
      n = Math.max(1, Math.hypot(forward, right)),
      speed = (this.home.policy.walkMps * clamp(dt, 0, 0.05)) / n;
    const p = this.frame.position;
    const next: Vec2 = [
      p[0] + (Math.sin(h) * forward + Math.cos(h) * right) * speed,
      p[1] + (Math.cos(h) * forward - Math.sin(h) * right) * speed,
    ];
    if (this.navigator.clear(p, next)) this.frame.position = next;
    this.updateRoom();
    // Resume starts a fresh path from this actual position, never an old path cursor.
    this.route = [this.frame.position];
    this.travelled = 0;
    this.stage = "walking";
  }
  private updateRoom() {
    this.frame.roomId =
      this.home.plan.rooms.find((r) => contains(this.frame.position, r.polygon))
        ?.id ?? null;
  }
  tick(elapsedSeconds: number): TourFrame {
    if (!this.playing) return { ...this.frame };
    const dt = clamp(elapsedSeconds, 0, 0.1) * clamp(this.rate, 0.5, 1.5),
      policy = this.home.policy;
    const stop = this.home.stops[this.frame.stopIndex];
    this.frame.phase = this.stage;
    if (this.stage === "entrance") {
      this.seconds += dt;
      if (this.seconds >= policy.entranceSeconds) {
        this.seconds = 0;
        this.stage = "walking";
      }
    } else if (this.stage === "walking") {
      if (this.route.length === 1)
        this.route = this.navigator.route(this.frame.position, stop.point);
      const length = pathLength(this.route),
        look = alongPath(this.route, Math.min(length, this.travelled + 0.55));
      const desired =
        distance(this.frame.position, look) > 0.03
          ? heading(this.frame.position, look)
          : this.frame.heading;
      this.frame.heading = turnToward(
        this.frame.heading,
        desired,
        policy.turnRadiansPerSecond * dt,
      );
      const alignment = Math.max(
        0,
        Math.cos(angleDelta(this.frame.heading, desired)),
      );
      const remaining = length - this.travelled;
      const speed =
        policy.walkMps * clamp(remaining / 0.6, 0.25, 1) * alignment;
      this.travelled = Math.min(length, this.travelled + speed * dt);
      this.frame.position = alongPath(this.route, this.travelled);
      this.frame.pitch *= Math.exp(-dt * 3);
      this.frame.showDimensions = false;
      if (remaining < 0.015 || this.reducedMotion) {
        this.frame.position = stop.point;
        this.stage = "settling";
        this.seconds = 0;
      }
    } else if (this.stage === "settling") {
      this.frame.heading = this.reducedMotion
        ? stop.heading
        : turnToward(
            this.frame.heading,
            stop.heading,
            policy.turnRadiansPerSecond * dt,
          );
      if (Math.abs(angleDelta(this.frame.heading, stop.heading)) < 0.02)
        this.seconds += dt;
      if (this.seconds >= policy.settleSeconds) {
        this.stage = "inspecting";
        this.seconds = 0;
      }
    } else if (this.stage === "inspecting") {
      this.seconds += dt;
      this.frame.showDimensions = true;
      const fraction = clamp(this.seconds / policy.inspectSeconds, 0, 1);
      // A gentle quarter-turn reveals breadth after length, while staying stationary.
      const turn =
        fraction < 0.3 ? 0 : fraction < 0.65 ? (fraction - 0.3) / 0.35 : 1;
      const smooth = turn * turn * (3 - 2 * turn);
      if (!this.reducedMotion)
        this.frame.heading = stop.heading + (smooth * Math.PI) / 2;
      this.frame.pitch = -0.08;
      if (this.seconds >= policy.inspectSeconds) {
        if (this.frame.stopIndex === this.home.stops.length - 1) {
          this.stage = "complete";
          this.frame.phase = "complete";
          this.playing = false;
        } else {
          this.frame.stopIndex++;
          this.route = this.home.routes[this.frame.stopIndex];
          this.travelled = this.seconds = 0;
          this.stage = "walking";
          this.frame.showDimensions = false;
        }
      }
    }
    this.updateRoom();
    this.frame.progress =
      (this.frame.stopIndex +
        (this.stage === "inspecting"
          ? clamp(this.seconds / policy.inspectSeconds, 0, 1)
          : 0)) /
      this.home.stops.length;
    if (this.stage === "complete") this.frame.progress = 1;
    return { ...this.frame };
  }
}
