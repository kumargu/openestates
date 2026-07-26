import { useEffect, useMemo, useState } from "react";
import { getPropertyEvidenceBatch } from "../lib/api.ts";
import type { PropertyEvidenceResponse } from "../lib/types.ts";

export function useEvidenceBatch(propertyIds: string[], enabled = true) {
  const [byId, setById] = useState<Map<string, PropertyEvidenceResponse>>(new Map());
  const [loading, setLoading] = useState(false);

  const key = useMemo(
    () => propertyIds.filter(Boolean).sort().join("|"),
    [propertyIds],
  );

  useEffect(() => {
    if (!enabled || propertyIds.length === 0) {
      queueMicrotask(() => {
        setById(new Map());
        setLoading(false);
      });
      return;
    }

    let cancelled = false;
    queueMicrotask(() => setLoading(true));

    getPropertyEvidenceBatch(propertyIds, 12)
      .then((response) => {
        if (cancelled) return;
        const next = new Map<string, PropertyEvidenceResponse>();
        for (const result of response.results) {
          next.set(result.property_id, result);
        }
        setById(next);
      })
      .catch(() => {
        if (!cancelled) setById(new Map());
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [enabled, key, propertyIds]);

  return { byId, loading };
}
