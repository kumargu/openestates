import { useCallback, useEffect, useRef, useState } from "react";
import type { ArrivalPlaybackController } from "../lib/arrivalPlayback.ts";
import {
  advanceRoadDistance,
  roadFlightCamera,
  type AtlasCameraPose,
  type AtlasRoute,
} from "../../../experiments/home-atlas/src/journey.ts";
import policy from "../../../app/config/ui/home-atlas.json";

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
}: {
  active: boolean;
  route: AtlasRoute | null;
  controller: ArrivalPlaybackController;
  elevation: number;
  width: number;
  render: (camera: AtlasCameraPose) => void;
  fly: (camera: AtlasCameraPose, durationMs: number) => void;
  autoPlay: boolean;
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
    const run = controller.begin("playing");
    const pose = () => {
      const camera = roadFlightCamera(
        route,
        distance.current,
        elevation,
        width,
        policy.road,
      );
      return handoffHeading.current === null
        ? camera
        : { ...camera, heading: handoffHeading.current };
    };
    const stop = controller.registerStopper(() => {
      cancelAnimationFrame(frame);
      previous = null;
    });
    const tick = (now: number) => {
      if (!run.isCurrent() || controller.snapshot() !== "playing") return;
      if (previous !== null)
        distance.current = advanceRoadDistance(
          route,
          distance.current,
          now - previous,
          rateRef.current,
          policy.road,
        );
      previous = now;
      render(pose());
      if (distance.current >= route.lengthM) {
        run.settle();
        return;
      }
      frame = requestAnimationFrame(tick);
    };
    const resume = controller.registerResumer(() => {
      if (!run.isCurrent()) return;
      if (flying) frame = requestAnimationFrame(tick);
      else fly(pose(), controller.remainingWaitMs());
    });
    const reducedMotion = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    if (
      reducedMotion ||
      handoffHeading.current !== null ||
      (!autoPlayRef.current && version === 0)
    ) {
      render(pose());
      run.settle();
    } else {
      run.activate();
      fly(pose(), policy.road.descentMs);
      void (async () => {
        if (!(await run.wait(policy.road.descentMs)) || !run.isCurrent())
          return;
        render(pose());
        if (
          !(await run.wait(policy.road.orientationDwellMs)) ||
          !run.isCurrent()
        )
          return;
        flying = true;
        frame = requestAnimationFrame(tick);
      })();
    }
    return () => {
      cancelAnimationFrame(frame);
      stop();
      resume();
      if (run.isCurrent()) controller.cancel("settled");
    };
  }, [active, route, controller, elevation, width, render, fly, version]);
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
