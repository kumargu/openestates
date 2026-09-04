import type { ReactNode } from "react";
import type { PropertySearchContext } from "../../lib/navigationContext.ts";
import { SearchSpanContext } from "./SearchSpanContext.ts";

export function SearchSpanProvider({
  value,
  children,
}: {
  value: PropertySearchContext | null;
  children: ReactNode;
}) {
  return (
    <SearchSpanContext.Provider value={value}>
      {children}
    </SearchSpanContext.Provider>
  );
}
