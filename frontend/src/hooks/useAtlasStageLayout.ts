import {
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from "react";
import {
  chooseAtlasPanelPlacement,
  type AtlasNearbyMode,
  type AtlasPanelPlacement,
} from "../lib/atlasNearbyUi.ts";

type AtlasStageLayoutPolicy = Readonly<{
  minimumMapWidthPx: number;
  minimumMapHeightPx: number;
  browsePanelWidthPx: number;
  compactPanelWidthPx: number;
  placementHysteresisPx: number;
  transitionMs: number;
  stableFrames: number;
}>;

type AtlasStageLayout = Readonly<{
  bodyRef: RefObject<HTMLDivElement | null>;
  mapFrameRef: RefObject<HTMLDivElement | null>;
  placement: AtlasPanelPlacement;
  layoutSettledVersion: number;
  layoutSettling: boolean;
}>;

export function useAtlasStageLayout(
  mode: AtlasNearbyMode,
  policy: AtlasStageLayoutPolicy,
): AtlasStageLayout {
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const mapFrameRef = useRef<HTMLDivElement | null>(null);
  const placementRef = useRef<AtlasPanelPlacement>("none");
  const bodyBoundsRef = useRef({ width: 0, height: 0 });
  const settleEpochRef = useRef(0);
  const [placement, setPlacement] = useState<AtlasPanelPlacement>("none");
  const [geometryVersion, setGeometryVersion] = useState(0);
  const [layoutSettledVersion, setLayoutSettledVersion] = useState(0);
  const [settledKey, setSettledKey] = useState("");

  useLayoutEffect(() => {
    const body = bodyRef.current;
    if (!body) return undefined;
    const measure = () => {
      const bounds = body.getBoundingClientRect();
      if (
        bounds.width !== bodyBoundsRef.current.width
        || bounds.height !== bodyBoundsRef.current.height
      ) {
        bodyBoundsRef.current = { width: bounds.width, height: bounds.height };
        setGeometryVersion((current) => current + 1);
      }
      const next = chooseAtlasPanelPlacement({
        width: bounds.width,
        height: bounds.height,
        mode,
        previousPlacement: placementRef.current,
        minimumMapWidthPx: policy.minimumMapWidthPx,
        minimumMapHeightPx: policy.minimumMapHeightPx,
        browsePanelWidthPx: policy.browsePanelWidthPx,
        compactPanelWidthPx: policy.compactPanelWidthPx,
        placementHysteresisPx: policy.placementHysteresisPx,
      });
      placementRef.current = next;
      setPlacement((current) => current === next ? current : next);
    };
    const observer = new ResizeObserver(measure);
    observer.observe(body);
    return () => observer.disconnect();
  }, [
    mode,
    policy.browsePanelWidthPx,
    policy.compactPanelWidthPx,
    policy.minimumMapHeightPx,
    policy.minimumMapWidthPx,
    policy.placementHysteresisPx,
  ]);

  const layoutKey = `${mode}:${placement}:${geometryVersion}`;
  const layoutSettling = settledKey !== layoutKey;

  useLayoutEffect(() => {
    const frame = mapFrameRef.current;
    if (!frame) return undefined;
    const epoch = settleEpochRef.current + 1;
    settleEpochRef.current = epoch;
    const reducedMotion = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    let timeout = 0;
    let animationFrame = 0;
    let latest = frame.getBoundingClientRect();
    let previous: DOMRect | null = null;
    let unchangedFrames = 0;
    frame.dataset.layoutSettled = "false";

    const observer = new ResizeObserver(() => {
      latest = frame.getBoundingClientRect();
      unchangedFrames = 0;
    });
    observer.observe(frame);

    const publish = () => {
      if (settleEpochRef.current !== epoch) return;
      frame.dataset.layoutSettled = "true";
      setSettledKey(layoutKey);
      setLayoutSettledVersion((current) => current + 1);
      frame.dispatchEvent(new CustomEvent("openestates:atlas-layout-settled", {
        bubbles: true,
        detail: {
          width: latest.width,
          height: latest.height,
          placement,
        },
      }));
    };

    const check = () => {
      if (settleEpochRef.current !== epoch) return;
      latest = frame.getBoundingClientRect();
      if (
        previous
        && previous.width === latest.width
        && previous.height === latest.height
      ) {
        unchangedFrames += 1;
      } else {
        unchangedFrames = 0;
      }
      previous = latest;
      if (unchangedFrames >= Math.max(1, policy.stableFrames)) {
        publish();
        return;
      }
      animationFrame = window.requestAnimationFrame(check);
    };

    timeout = window.setTimeout(
      () => {
        animationFrame = window.requestAnimationFrame(check);
      },
      reducedMotion ? 0 : policy.transitionMs,
    );

    return () => {
      observer.disconnect();
      window.clearTimeout(timeout);
      window.cancelAnimationFrame(animationFrame);
    };
  }, [
    layoutKey,
    mode,
    placement,
    policy.stableFrames,
    policy.transitionMs,
  ]);

  return {
    bodyRef,
    mapFrameRef,
    placement,
    layoutSettledVersion,
    layoutSettling,
  };
}
