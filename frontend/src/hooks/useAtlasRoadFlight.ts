import { useCallback, useEffect, useRef, useState } from "react";
import type { ArrivalPlaybackController } from "../lib/arrivalPlayback.ts";
import {
  advanceRoadDistance,
  dampHeading,
  roadFlightCamera,
  type AtlasCameraPose,
  type AtlasRoute,
} from "../../../experiments/home-atlas/src/journey.ts";
import policy from "../../../app/config/ui/home-atlas.json" with { type: "json" };

/** Renderer driver subordinate to the page's single playback controller. */
export function useAtlasRoadFlight({
  active,
  route,
  controller,
  elevation,
  width,
  render,
  fly,
  autoPlay,
  onProgress,
  onPhase,
}: {
  active: boolean;
  route: AtlasRoute | null;
  controller: ArrivalPlaybackController;
  elevation: number;
  width: number;
  render: (camera: AtlasCameraPose) => void;
  fly: (camera: AtlasCameraPose, durationMs: number) => void;
  autoPlay: boolean;
  onProgress?: (distanceM: number, routeLengthM: number, heading: number) => void;
  onPhase?: (phase: "context" | "descent" | "flight" | "settled") => void;
}) {
  const [rate, setRate] = useState(1);
  const [version, setVersion] = useState(0);
  const distance = useRef(0);
  const handoffHeading = useRef<number | null>(null);
  const rateRef = useRef(rate);
  const autoPlayRef = useRef(autoPlay);
  useEffect(() => {
    rateRef.current = rate;
  }, [rate]);
  useEffect(() => {
    autoPlayRef.current = autoPlay;
  }, [autoPlay]);
  useEffect(() => {
    distance.current = 0;
  }, [route]);
  useEffect(() => {
    if (!active || !route) return;
    let frame = 0;
    let previous: number | null = null;
    let flying = false;
    let introTarget: AtlasCameraPose | null = null;
    let smoothedHeading: number | null = null;
    const run = controller.begin("playing");
    const pose = (elapsedSeconds = 0) => {
      const camera = roadFlightCamera(
        route,
        distance.current,
        elevation,
        width,
        policy.road,
      );
      const targetHeading = handoffHeading.current ?? camera.heading;
      if (smoothedHeading === null) smoothedHeading = targetHeading;
      else smoothedHeading = dampHeading(
        smoothedHeading,
        targetHeading,
        policy.road.headingDamping,
        elapsedSeconds,
      );
      handoffHeading.current = null;
      return { ...camera, heading: smoothedHeading };
    };
    const stop = controller.registerStopper(() => {
      cancelAnimationFrame(frame);
      previous = null;
    });
    const tick = (now: number) => {
      if (!run.isCurrent() || controller.snapshot() !== "playing") return;
      const elapsedMs = previous === null ? 0 : now - previous;
      if (previous !== null)
        distance.current = advanceRoadDistance(
          route,
          distance.current,
          elapsedMs,
          rateRef.current,
          policy.road,
        );
      previous = now;
      const camera = pose(elapsedMs / 1_000);
      render(camera);
      onProgress?.(distance.current, route.lengthM, camera.heading);
      if (distance.current >= route.lengthM) {
        onPhase?.("settled");
        run.settle();
        return;
      }
      frame = requestAnimationFrame(tick);
    };
    const resume = controller.registerResumer(() => {
      if (!run.isCurrent()) return;
      if (flying) frame = requestAnimationFrame(tick);
      else if (introTarget) fly(introTarget, controller.remainingWaitMs());
    });
    const reducedMotion = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    if (
      reducedMotion ||
      handoffHeading.current !== null ||
      (!autoPlayRef.current && version === 0)
    ) {
      const camera = pose();
      render(camera);
      onProgress?.(distance.current, route.lengthM, camera.heading);
      onPhase?.("settled");
      run.settle();
    } else {
      run.activate();
      const roadPose = pose();
      const contextPose = {
        ...roadPose,
        range: policy.road.contextRangeM,
        tilt: policy.road.contextTilt,
      };
      introTarget = contextPose;
      onPhase?.("context");
      fly(contextPose, policy.road.contextMoveMs);
      void (async () => {
        if (!(await run.wait(policy.road.contextMoveMs)) || !run.isCurrent())
          return;
        render(contextPose);
        if (
          !(await run.wait(policy.road.orientationDwellMs)) ||
          !run.isCurrent()
        )
          return;
        introTarget = roadPose;
        onPhase?.("descent");
        fly(roadPose, policy.road.descentMs);
        if (!(await run.wait(policy.road.descentMs)) || !run.isCurrent()) return;
        render(roadPose);
        onProgress?.(distance.current, route.lengthM, roadPose.heading);
        introTarget = null;
        flying = true;
        onPhase?.("flight");
        frame = requestAnimationFrame(tick);
      })();
    }
    return () => {
      cancelAnimationFrame(frame);
      stop();
      resume();
      if (run.isCurrent()) controller.cancel("settled");
    };
  }, [active, route, controller, elevation, width, render, fly, onPhase, onProgress, version]);
  const seek = useCallback((metres: number, heading: number) => {
    distance.current = metres;
    handoffHeading.current = heading;
  }, []);
  const position = useCallback(() => distance.current, []);
  return {
    rate,
    setRate,
    seek,
    position,
    replay: () => {
      distance.current = 0;
      handoffHeading.current = null;
      setVersion((v) => v + 1);
    },
  };
}
