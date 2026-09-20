import { useLayoutEffect, useRef, useState } from "react";
import {
  LANDING_RAIL_GAP_PX,
  LANDING_RAIL_MIN_CARD_PX,
  RAIL_PAGE_COMPACT_MAX_WIDTH,
  fittedRailPageSize,
} from "../lib/rail-pagination.ts";

const LANDING_RAIL_MAX_WIDTH_PX = 1320;
const LANDING_RAIL_HORIZONTAL_INSET_PX = 40;

export function useFittedRailPage(
  itemCount: number,
  options: { minCardWidth?: number; gap?: number } = {},
) {
  const minCardWidth = options.minCardWidth ?? LANDING_RAIL_MIN_CARD_PX;
  const gap = options.gap ?? LANDING_RAIL_GAP_PX;
  const [viewport, setViewport] = useState<HTMLDivElement | null>(null);
  const [pageSize, setPageSize] = useState(() => typeof window === "undefined" ? 1
    : fittedRailPageSize(Math.min(LANDING_RAIL_MAX_WIDTH_PX, window.innerWidth - LANDING_RAIL_HORIZONTAL_INSET_PX), {
      compact: window.matchMedia(`(max-width: ${RAIL_PAGE_COMPACT_MAX_WIDTH}px)`).matches,
      minCardWidth, gap,
    }));
  const [position, setPosition] = useState({ leadingIndex: 0, canPrevious: false, canNext: itemCount > pageSize });
  const lastPosition = useRef(position);

  useLayoutEffect(() => {
    if (!viewport) return;
    let frame = 0;
    const syncPosition = () => {
      const left = viewport.getBoundingClientRect().left;
      const cards = [...viewport.querySelectorAll<HTMLElement>("[data-rail-item-index]")];
      const leadingIndex = Math.max(0, cards.findIndex((card) => card.getBoundingClientRect().right > left + 1));
      const next = { leadingIndex, canPrevious: viewport.scrollLeft > 1,
        canNext: viewport.scrollLeft < viewport.scrollWidth - viewport.clientWidth - 1 };
      const previous = lastPosition.current;
      if (next.leadingIndex !== previous.leadingIndex || next.canPrevious !== previous.canPrevious || next.canNext !== previous.canNext) {
        lastPosition.current = next;
        setPosition(next);
      }
    };
    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(syncPosition);
    };
    const resize = () => {
      setPageSize(fittedRailPageSize(viewport.clientWidth, {
        compact: window.matchMedia(`(max-width: ${RAIL_PAGE_COMPACT_MAX_WIDTH}px)`).matches,
        minCardWidth, gap,
      }));
      schedule();
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(viewport);
    viewport.addEventListener("scroll", schedule, { passive: true });
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      viewport.removeEventListener("scroll", schedule);
    };
  }, [gap, minCardWidth, viewport, itemCount, pageSize]);

  return { viewportRef: setViewport, pageSize,
    pageCount: Math.max(1, Math.ceil(Math.max(itemCount, 0) / pageSize)), ...position };
}
