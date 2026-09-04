import { createContext, useContext } from "react";
import type { PropertySearchContext } from "../../lib/navigationContext.ts";

export const SearchSpanContext = createContext<PropertySearchContext | null>(null);

export function useSearchSpan(): PropertySearchContext | null {
  return useContext(SearchSpanContext);
}
