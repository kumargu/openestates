import { useEffect, useState } from "react";
import {
  LANDING_RAIL_GAP_PX,
  LANDING_RAIL_MIN_CARD_PX,
  RAIL_PAGE_COMPACT_MAX_WIDTH,
  fittedRailPageSize,
} from "../lib/rail-pagination.ts";

export function useFittedRailPage(
  itemCount: number,
  options: {
    minCardWidth?: number;
    gap?: number;
  } = {},
) {
  const minCardWidth = options.minCardWidth ?? LANDING_RAIL_MIN_CARD_PX;
  const gap = options.gap ?? LANDING_RAIL_GAP_PX;
  const [viewport, setViewport] = useState<HTMLDivElement | null>(null);
  const [pageSize, setPageSize] = useState(4);
  const [pageState, setPageState] = useState({
    page: 0,
    itemCount,
    pageSize,
  });

  useEffect(() => {
    if (!viewport) return;

    const sync = () => {
      const compact = window.matchMedia(`(max-width: ${RAIL_PAGE_COMPACT_MAX_WIDTH}px)`).matches;
      setPageSize(fittedRailPageSize(viewport.clientWidth, {
        compact,
        minCardWidth,
        gap,
      }));
    };

    sync();
    const observer = new ResizeObserver(sync);
    observer.observe(viewport);
    window.addEventListener("resize", sync);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", sync);
    };
  }, [gap, minCardWidth, viewport]);

  const pageCount = Math.max(1, Math.ceil(Math.max(itemCount, 0) / pageSize));
  const requestedPage = pageState.itemCount === itemCount && pageState.pageSize === pageSize
    ? pageState.page
    : 0;
  const safePage = Math.min(requestedPage, pageCount - 1);
  const setPage = (page: number) => setPageState({
    page: Math.max(0, Math.min(page, pageCount - 1)),
    itemCount,
    pageSize,
  });

  return {
    viewportRef: setViewport,
    page: safePage,
    setPage,
    pageSize,
    pageCount,
  };
}
