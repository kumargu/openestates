import { useEffect, useMemo, useState } from "react";
import {
  initialPropertySceneUrls,
  probeImageUrls,
  societyPhotoCandidates,
  societySlugFromId,
} from "../lib/propertyScene.ts";

type Input = {
  heroImage?: string | null;
  images?: string[];
  societyId?: string;
};

export function usePropertySceneImages(input: Input) {
  const seeds = useMemo(
    () =>
      initialPropertySceneUrls({
        heroImage: input.heroImage,
        images: input.images,
        societyId: input.societyId,
      }),
    [input.heroImage, input.images, input.societyId],
  );

  const [images, setImages] = useState<string[]>(seeds);
  const [loading, setLoading] = useState(seeds.length === 0 && Boolean(input.societyId));

  useEffect(() => {
    let cancelled = false;

    async function resolve() {
      if (seeds.length > 0) {
        const loaded = await probeImageUrls(seeds);
        if (!cancelled) {
          setImages(loaded.length > 0 ? loaded : seeds);
          setLoading(false);
        }
        return;
      }

      const slug = societySlugFromId(input.societyId);
      if (!slug) {
        if (!cancelled) {
          setImages([]);
          setLoading(false);
        }
        return;
      }

      setLoading(true);
      const candidates = societyPhotoCandidates(slug, 5);
      const loaded = await probeImageUrls(candidates);
      if (!cancelled) {
        setImages(loaded);
        setLoading(false);
      }
    }

    void resolve();
    return () => {
      cancelled = true;
    };
  }, [input.societyId, seeds]);

  return { images, loading, hasImages: images.length > 0 };
}
