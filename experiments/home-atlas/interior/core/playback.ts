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
import { doorwayCenter, roundRoute } from "./director.ts";

/** One clock owns translation, orientation, arrival and each authored viewing moment. */
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
  private speed = 0;
  private route: readonly Vec2[];
  private length = 0;
  private resumeNeedsSettle = false;
  private readonly doors: Vec2[];
  constructor(home: PreparedHome) {
    this.home = home;
    this.navigator = new Navigator(home.plan, home.policy);
    this.doors = home.plan.walls
      .filter((w) => w.opening && w.opening.kind !== "window")
      .map(doorwayCenter);
    this.route = home.routes[0];
    this.length = pathLength(this.route);
    this.frame = this.initial();
  }
  private initial(): TourFrame {
    return {
      position: this.home.entrance,
      heading: this.home.entranceHeading,
      pitch: 0,
      phase: "entrance",
      roomId: null,
      stopIndex: 0,
      viewIndex: 0,
      progress: 0,
      showDimensions: false,
    };
  }
  reset() {
    this.playing = false;
    this.stage = "entrance";
    this.seconds = this.travelled = this.speed = 0;
    this.resumeNeedsSettle = false;
    this.route = this.home.routes[0];
    this.length = pathLength(this.route);
    this.frame = this.initial();
  }
  play() {
    if (this.stage === "complete") this.reset();
    if (this.resumeNeedsSettle && this.stage === "inspecting") {
      this.stage = "settling";
      this.seconds = 0;
    }
    this.resumeNeedsSettle = false;
    this.playing = true;
    this.frame.phase = this.stage;
  }
  pause() {
    this.playing = false;
    this.speed = 0;
    this.frame.phase = "paused";
  }
  private setRoute(end: Vec2) {
    // Route construction precedes mutation: failure leaves the current camera state intact.
    const path = roundRoute(
      this.navigator.route(this.frame.position, end),
      this.navigator,
      this.home.policy.cornerRadiusM,
    );
    this.route = path;
    this.length = pathLength(path);
    this.travelled = this.seconds = this.speed = 0;
    this.stage = "walking";
    this.frame.showDimensions = false;
  }
  select(index: number) {
    if (!Number.isFinite(index)) throw new Error("Invalid room selection.");
    const next = clamp(Math.trunc(index), 0, this.home.stops.length - 1);
    this.setRoute(this.home.stops[next].views[0].point);
    this.frame.stopIndex = next;
    this.frame.viewIndex = 0;
    this.playing = true;
  }
  look(delta: number, pitchDelta = 0) {
    this.pause();
    this.frame.heading += delta;
    this.frame.pitch = clamp(this.frame.pitch + pitchDelta, -0.85, 0.85);
    this.resumeNeedsSettle = true;
  }
  move(forward: number, right: number, dt: number) {
    if (!forward && !right) return;
    this.pause();
    this.frame.phase = "exploring";
    this.frame.showDimensions = false;
    const h = this.frame.heading,
      n = Math.max(1, Math.hypot(forward, right)),
      s = (this.home.policy.walkMps * clamp(dt, 0, 0.05)) / n,
      p = this.frame.position;
    const next: Vec2 = [
      p[0] + (Math.sin(h) * forward + Math.cos(h) * right) * s,
      p[1] + (Math.cos(h) * forward - Math.sin(h) * right) * s,
    ];
    if (this.navigator.clear(p, next)) this.frame.position = next;
    this.updateRoom();
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
    if (!Number.isFinite(elapsedSeconds) || elapsedSeconds < 0)
      throw new Error("Invalid elapsed time.");
    if (!this.playing) return { ...this.frame };
    // Pace controls walking only. A faster journey never steals the time to inspect a room.
    const dt = Math.min(elapsedSeconds, 0.1),
      p = this.home.policy,
      stop = this.home.stops[this.frame.stopIndex],
      view = stop.views[this.frame.viewIndex];
    if (this.stage === "entrance") {
      this.seconds += dt;
      if (this.seconds >= p.entranceSeconds) {
        this.seconds = 0;
        this.stage = "walking";
      }
    } else if (this.stage === "walking") {
      if (this.route.length === 1) this.setRoute(view.point);
      const remaining = this.length - this.travelled;
      if (remaining < 1e-8 || this.reducedMotion) {
        this.frame.position = view.point;
        this.speed = 0;
        this.stage = "settling";
        this.seconds = 0;
      } else {
        // Face the actual next path segment, not a point beyond a wall around the corner.
        let cursor = 0,
          next = this.route.at(-1)!;
        for (let i = 1; i < this.route.length; i++) {
          cursor += distance(this.route[i - 1], this.route[i]);
          if (cursor > this.travelled + 1e-8) {
            next = this.route[i];
            break;
          }
        }
        const desired = heading(this.frame.position, next);
        this.frame.heading = turnToward(
          this.frame.heading,
          desired,
          p.turnRadiansPerSecond * dt,
        );
        const aligned =
          Math.abs(angleDelta(this.frame.heading, desired)) <=
          p.maxMovingYawError;
        const nearDoor = this.doors.some(
          (d) => distance(d, this.frame.position) < 0.8,
        );
        const desiredSpeed = Math.min(
          (nearDoor ? p.doorwayMps : p.walkMps) * clamp(this.rate, 0.5, 1.5),
          Math.sqrt(2 * p.accelerationMps2 * remaining),
        );
        this.speed = aligned
          ? Math.min(desiredSpeed, this.speed + p.accelerationMps2 * dt)
          : 0;
        // Never cross a sharp corner in the same tick: rotate at the corner before the next leg.
        this.travelled = Math.min(
          cursor,
          this.length,
          this.travelled + this.speed * dt,
        );
        this.frame.position = alongPath(this.route, this.travelled);
        this.frame.pitch = turnToward(this.frame.pitch, -0.06, 0.3 * dt);
      }
    } else if (this.stage === "settling") {
      this.frame.heading = this.reducedMotion
        ? view.heading
        : turnToward(
            this.frame.heading,
            view.heading,
            p.turnRadiansPerSecond * dt,
          );
      this.frame.pitch = this.reducedMotion
        ? view.pitch
        : turnToward(this.frame.pitch, view.pitch, 0.25 * dt);
      const aligned =
        Math.abs(angleDelta(this.frame.heading, view.heading)) < 0.005 &&
        Math.abs(this.frame.pitch - view.pitch) < 0.005;
      if (aligned) this.seconds += dt;
      if (this.seconds >= p.settleSeconds) {
        this.stage = "inspecting";
        this.seconds = 0;
      }
    } else if (this.stage === "inspecting") {
      // Hold completely still. The viewer should have time to read the space.
      this.seconds += dt;
      this.frame.showDimensions = true;
      if (this.seconds >= view.holdSeconds) {
        if (this.frame.viewIndex < stop.views.length - 1) {
          const next = stop.views[this.frame.viewIndex + 1];
          this.setRoute(next.point);
          this.frame.viewIndex++;
        } else if (this.frame.stopIndex < this.home.stops.length - 1) {
          this.setRoute(
            this.home.stops[this.frame.stopIndex + 1].views[0].point,
          );
          this.frame.stopIndex++;
          this.frame.viewIndex = 0;
        } else {
          this.stage = "complete";
          this.playing = false;
        }
      }
    }
    this.updateRoom();
    this.frame.phase = this.stage;
    if (this.stage !== "inspecting" && this.stage !== "complete")
      this.frame.showDimensions = false;
    this.frame.progress =
      this.stage === "complete"
        ? 1
        : (this.frame.stopIndex +
            (this.frame.viewIndex +
              (this.stage === "inspecting"
                ? clamp(this.seconds / view.holdSeconds, 0, 1)
                : 0)) /
              stop.views.length) /
          this.home.stops.length;
    return { ...this.frame };
  }
}
