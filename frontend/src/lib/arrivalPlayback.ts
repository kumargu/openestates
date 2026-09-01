import { useCallback, useEffect, useState, useSyncExternalStore } from "react";

export type ArrivalPlaybackState =
  | "idle"
  | "preparing"
  | "revealing"
  | "playing"
  | "paused"
  | "settled"
  | "unavailable";

export type ArrivalActiveState = "revealing" | "playing";

export type ArrivalPlaybackClock = {
  now: () => number;
  setTimeout: (callback: () => void, milliseconds: number) => number;
  clearTimeout: (id: number) => void;
};

type PendingWait = {
  id: number | null;
  remainingMs: number;
  startedAt: number;
  resolve: (completed: boolean) => void;
};

const browserClock: ArrivalPlaybackClock = {
  now: () => performance.now(),
  setTimeout: (callback, milliseconds) => window.setTimeout(callback, milliseconds),
  clearTimeout: (id) => window.clearTimeout(id),
};

export class ArrivalPlaybackController {
  private readonly clock: ArrivalPlaybackClock;
  private state: ArrivalPlaybackState = "idle";
  private activeState: ArrivalActiveState = "revealing";
  private runId = 0;
  private readonly listeners = new Set<() => void>();
  private readonly stoppers = new Set<() => void>();
  private readonly resumers = new Set<() => void>();
  private readonly waits = new Set<PendingWait>();

  constructor(clock: ArrivalPlaybackClock = browserClock) {
    this.clock = clock;
  }

  snapshot = (): ArrivalPlaybackState => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  begin(activeState: ArrivalActiveState): ArrivalPlaybackRun {
    this.cancelPending("idle");
    this.runId += 1;
    this.activeState = activeState;
    this.setState("preparing");
    return new ArrivalPlaybackRun(this, this.runId, activeState);
  }

  pause(): void {
    if (!matchesRunningState(this.state)) return;
    this.stopRenderers();
    for (const wait of this.waits) {
      if (wait.id === null) continue;
      this.clock.clearTimeout(wait.id);
      wait.id = null;
      wait.remainingMs = Math.max(0, wait.remainingMs - (this.clock.now() - wait.startedAt));
    }
    this.setState("paused");
  }

  resume(): void {
    if (this.state !== "paused") return;
    this.setState(this.activeState);
    for (const resume of this.resumers) resume();
    for (const wait of this.waits) this.schedule(wait);
  }

  cancel(nextState: "idle" | "settled" | "unavailable" = "settled"): void {
    this.cancelPending(nextState);
    this.runId += 1;
  }

  registerStopper(stopper: () => void): () => void {
    this.stoppers.add(stopper);
    return () => this.stoppers.delete(stopper);
  }

  registerResumer(resumer: () => void): () => void {
    this.resumers.add(resumer);
    return () => this.resumers.delete(resumer);
  }

  isCurrent(runId: number): boolean {
    return runId === this.runId;
  }

  remainingWaitMs(): number {
    let remainingMs = 0;
    for (const wait of this.waits) {
      const elapsedMs = wait.id === null ? 0 : this.clock.now() - wait.startedAt;
      remainingMs = Math.max(remainingMs, Math.max(0, wait.remainingMs - elapsedMs));
    }
    return remainingMs;
  }

  activate(runId: number, state: ArrivalActiveState): boolean {
    if (!this.isCurrent(runId)) return false;
    this.activeState = state;
    if (this.state !== "paused") this.setState(state);
    return true;
  }

  finish(runId: number, state: "settled" | "unavailable"): void {
    if (!this.isCurrent(runId)) return;
    this.cancelPending(state);
  }

  wait(runId: number, milliseconds: number): Promise<boolean> {
    if (!this.isCurrent(runId)) return Promise.resolve(false);
    return new Promise((resolve) => {
      const wait: PendingWait = {
        id: null,
        remainingMs: Math.max(0, milliseconds),
        startedAt: this.clock.now(),
        resolve,
      };
      this.waits.add(wait);
      if (this.state !== "paused") this.schedule(wait);
    });
  }

  private schedule(wait: PendingWait): void {
    if (!this.waits.has(wait) || wait.id !== null) return;
    wait.startedAt = this.clock.now();
    wait.id = this.clock.setTimeout(() => {
      wait.id = null;
      this.waits.delete(wait);
      wait.resolve(true);
    }, wait.remainingMs);
  }

  private cancelPending(nextState: ArrivalPlaybackState): void {
    this.stopRenderers();
    for (const wait of this.waits) {
      if (wait.id !== null) this.clock.clearTimeout(wait.id);
      wait.resolve(false);
    }
    this.waits.clear();
    this.setState(nextState);
  }

  private stopRenderers(): void {
    for (const stop of this.stoppers) stop();
  }

  private setState(state: ArrivalPlaybackState): void {
    if (this.state === state) return;
    this.state = state;
    for (const listener of this.listeners) listener();
  }
}

export class ArrivalPlaybackRun {
  private readonly controller: ArrivalPlaybackController;
  private readonly runId: number;
  private readonly activeState: ArrivalActiveState;

  constructor(
    controller: ArrivalPlaybackController,
    runId: number,
    activeState: ArrivalActiveState,
  ) {
    this.controller = controller;
    this.runId = runId;
    this.activeState = activeState;
  }

  activate(): boolean {
    return this.controller.activate(this.runId, this.activeState);
  }

  wait(milliseconds: number): Promise<boolean> {
    return this.controller.wait(this.runId, milliseconds);
  }

  isCurrent(): boolean {
    return this.controller.isCurrent(this.runId);
  }

  settle(): void {
    this.controller.finish(this.runId, "settled");
  }

  unavailable(): void {
    this.controller.finish(this.runId, "unavailable");
  }
}

function matchesRunningState(state: ArrivalPlaybackState): boolean {
  return state === "preparing" || state === "revealing" || state === "playing";
}

export function useArrivalPlaybackController(): {
  controller: ArrivalPlaybackController;
  state: ArrivalPlaybackState;
  pause: () => void;
  resume: () => void;
} {
  const [controller] = useState(() => new ArrivalPlaybackController());
  const state = useSyncExternalStore(controller.subscribe, controller.snapshot, controller.snapshot);
  const pause = useCallback(() => controller.pause(), [controller]);
  const resume = useCallback(() => controller.resume(), [controller]);
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") controller.pause();
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [controller]);
  useEffect(() => () => controller.cancel("idle"), [controller]);
  return { controller, state, pause, resume };
}
