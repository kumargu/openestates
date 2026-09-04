import { useLayoutEffect } from "react";
import { PUBLIC_BRAND_NAME } from "../lib/brand.ts";

export function PageTitle({ title }: { title: string }) {
  useLayoutEffect(() => {
    document.title = title;
    return () => {
      if (document.title === title) document.title = PUBLIC_BRAND_NAME;
    };
  }, [title]);

  return null;
}
