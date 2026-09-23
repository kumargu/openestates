import { useCallback, useEffect, useRef, useState } from "react";
import type { ArrivalPlaybackController } from "../lib/arrivalPlayback.ts";
import {
  advanceRoadDistance,
  dampHeading,
  roadFlightCamera,
  projectPointOntoRoute,
  type AtlasPoint,
  type AtlasCameraPose,
  type AtlasRoute,
} from "../lib/atlas/journey.ts";
import policy from "../lib/atlasPolicy.ts";
import { distanceMetres } from "../lib/atlas/geometry.ts";

/** Renderer driver subordinate to the page's single playback controller. */
export function useAtlasRoadFlight({
  active,
  route,
  home,
  entrance,
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
  home: AtlasPoint;
  entrance: AtlasPoint | null;
  controller: ArrivalPlaybackController;
  elevation: number;
  width: number;
  render: (camera: AtlasCameraPose) => void;
  fly: (camera: AtlasCameraPose, durationMs: number) => void;
  autoPlay: boolean;
  onProgress?: (distanceM: number, routeLengthM: number, heading: number) => void;
  onPhase?: (phase: "context" | "descent" | "flight" | "entrance" | "settled") => void;
}) {
  const [rate, setRate] = useState(policy.road.defaultRate);
  const [version, setVersion] = useState(0);
  const distance = useRef(0);
  const handoffPending = useRef(false);
  const entranceVisited = useRef(false);
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
    handoffPending.current = false;
    entranceVisited.current = false;
  }, [route]);
  useEffect(() => {
    if (!active || !route) return;
    let frame = 0;
    let previous: number | null = null;
    let flying = false;
    let introTarget: AtlasCameraPose | null = null;
    let smoothedHeading: number | null = null;
    const entranceDistance = entrance ? projectPointOntoRoute(route, entrance).distanceAlongM : null;
    if (entranceDistance !== null && distance.current > entranceDistance) entranceVisited.current = true;
    // One height throughout the pass, referenced to the resolved local terrain.
    // Increase it for distant home anchors rather than tilting toward the horizon.
    const farthestM = Math.max(...route.coordinates.map(([lng, lat]) =>
      distanceMetres({lat, lng}, {lat: home.latitude, lng: home.longitude}))) * policy.road.homeFocusWeight;
    const heightM = Math.max(
      width < policy.road.mobileBreakpointPx ? policy.road.mobileFlightHeightM : policy.road.flightHeightM,
      policy.road.altitudeOffsetM + farthestM / Math.tan(policy.road.maximumTilt * Math.PI / 180),
    );
    const run = controller.begin("playing");
    const pose = (elapsedSeconds = 0) => {
      const camera = roadFlightCamera(
        route,
        distance.current,
        elevation,
        home,
        heightM,
        policy.road,
      );
      const targetHeading = camera.heading;
      if (smoothedHeading === null) smoothedHeading = targetHeading;
      else smoothedHeading = dampHeading(
        smoothedHeading,
        targetHeading,
        policy.road.headingDamping,
        elapsedSeconds,
      );
      return { ...camera, heading: smoothedHeading };
    };
    const stop = controller.registerStopper(() => {
      cancelAnimationFrame(frame);
      previous = null;
    });
    const tick = (now: number) => {
      if (!run.isCurrent() || controller.snapshot() !== "playing") return;
      const elapsedMs = previous === null ? 0 : now - previous;
      const remainingToEntrance = entranceDistance === null ? Infinity : Math.abs(entranceDistance - distance.current);
      const approach = Math.min(1, remainingToEntrance / policy.road.entranceSlowRadiusM);
      const speedScale = policy.road.entranceSpeedScale
        + (1 - policy.road.entranceSpeedScale) * approach * approach * (3 - 2 * approach);
      if (previous !== null)
        distance.current = advanceRoadDistance(
          route,
          distance.current,
          elapsedMs,
          rateRef.current,
          {...policy.road, baseSpeedMps: policy.road.baseSpeedMps * speedScale},
        );
      const stopAtEntrance = entranceDistance !== null && !entranceVisited.current
        && distance.current >= entranceDistance;
      if (stopAtEntrance) distance.current = entranceDistance;
      previous = now;
      const camera = pose(elapsedMs / 1_000);
      render(camera);
      onProgress?.(distance.current, route.lengthM, camera.heading);
      if (stopAtEntrance) {
        entranceVisited.current = true;
        flying = false;
        onPhase?.("entrance");
        void run.wait(policy.road.entranceDwellMs).then(completed => {
          if (!completed || !run.isCurrent()) return;
          previous = null;
          flying = true;
          onPhase?.("flight");
          frame = requestAnimationFrame(tick);
        });
        return;
      }
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
    const resumingFromStreet = handoffPending.current;
    handoffPending.current = false;
    if (
      reducedMotion ||
      (!autoPlayRef.current && version === 0)
    ) {
      const camera = pose();
      render(camera);
      onProgress?.(distance.current, route.lengthM, camera.heading);
      onPhase?.("settled");
      run.settle();
    } else if (resumingFromStreet) {
      run.activate();
      const camera = pose();
      render(camera);
      onProgress?.(distance.current, route.lengthM, camera.heading);
      flying = true;
      onPhase?.("flight");
      frame = requestAnimationFrame(tick);
    } else {
      run.activate();
      const roadPose = pose();
      const contextPose = {
        ...roadPose,
        range: Math.max(policy.road.contextRangeM, roadPose.range),
        tilt: policy.road.contextTilt,
      };
      introTarget = contextPose;
      onPhase?.("context");
      fly(contextPose, policy.road.contextMoveMs);
      void (async () => {
        if (!(await run.wait(policy.road.contextMoveMs)) || !run.isCurrent())
          return;
        render(contextPose);
        introTarget = null;
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
  }, [active, route, home, entrance, controller, elevation, width, render, fly, onPhase, onProgress, version]);
  const seek = useCallback((metres: number) => {
    distance.current = metres;
    handoffPending.current = true;
  }, []);
  const position = useCallback(() => distance.current, []);
  return {
    rate,
    setRate,
    seek,
    position,
    replay: () => {
      distance.current = 0;
      handoffPending.current = false;
      entranceVisited.current = false;
      setVersion((v) => v + 1);
    },
  };
}
