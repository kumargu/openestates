/**
 * Resolve only the image URLs explicitly promoted in the serving payload.
 *
 * This hook must remain a synchronous selector, not an image loader. Browsers
 * already fetch `<img>` resources independently from the property API response,
 * and each rendering surface owns the correct priority: the visible hero uses
 * eager/high priority while galleries and recommendations use lazy/low priority.
 * `ImageWithFallback` handles an individual failed request at render time.
 *
 * Do not add `new Image()`, `Promise.all()`, HEAD requests, or society-ID path
 * guessing here. A preflight probe starts every gallery download on page reload,
 * defeats native lazy loading, and multiplies requests when several cards use
 * this hook. A missing URL is a serving-bundle/data issue; the frontend must not
 * search local folders for an alternative source of truth.
 */
import { useMemo } from "react";
import type { PropertyMedia } from "../lib/types.ts";
import { trustedPropertyMedia } from "../lib/propertyScene.ts";

type Input = {
  media?: PropertyMedia[];
};

export function usePropertySceneImages(input: Input) {
  const media = useMemo(() => trustedPropertyMedia(input.media), [input.media]);

  return { media, loading: false, hasImages: media.length > 0 };
}
