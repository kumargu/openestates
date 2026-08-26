import {
  normalizePublicOrigin,
  resolveBackendOwnedUrl,
  resolveSiteUrl,
} from "./publicUrls.ts";

const META_ENV = (import.meta as ImportMeta & {
  env?: Record<string, string | boolean | undefined>;
}).env ?? {};
const IS_PRODUCTION = META_ENV.PROD === true;

export const API_ORIGIN = normalizePublicOrigin(
  typeof META_ENV.VITE_API_BASE === "string" ? META_ENV.VITE_API_BASE : undefined,
  "VITE_API_BASE",
  { required: IS_PRODUCTION, httpsOnly: IS_PRODUCTION },
);

export const SITE_ORIGIN = normalizePublicOrigin(
  typeof META_ENV.VITE_SITE_URL === "string" ? META_ENV.VITE_SITE_URL : undefined,
  "VITE_SITE_URL",
  { required: IS_PRODUCTION, httpsOnly: IS_PRODUCTION },
);

export function backendUrl(value: string): string {
  return resolveBackendOwnedUrl(value, API_ORIGIN);
}

export function publicSiteUrl(path: string): string {
  return resolveSiteUrl(path, SITE_ORIGIN);
}
