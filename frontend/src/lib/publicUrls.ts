type PublicOriginOptions = {
  required?: boolean;
  httpsOnly?: boolean;
};

export function normalizePublicOrigin(
  rawValue: string | undefined,
  variableName: string,
  options: PublicOriginOptions = {},
): string {
  const value = rawValue?.trim() ?? "";
  if (!value) {
    if (options.required) {
      throw new Error(`${variableName} must be set for production builds`);
    }
    return "";
  }

  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error(`${variableName} must be an absolute URL`);
  }

  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error(`${variableName} must use http or https`);
  }
  if (options.httpsOnly && url.protocol !== "https:") {
    throw new Error(`${variableName} must use https for production builds`);
  }
  if (url.username || url.password) {
    throw new Error(`${variableName} must not contain credentials`);
  }
  if (url.pathname !== "/" || url.search || url.hash) {
    throw new Error(`${variableName} must be an origin without a path, query, or fragment`);
  }

  return url.origin;
}

export function resolveBackendOwnedUrl(value: string, apiOrigin: string): string {
  if (!apiOrigin || !/^\/(?:api|media)(?:\/|$)/.test(value)) return value;
  return `${apiOrigin}${value}`;
}

export function resolveSiteUrl(path: string, siteOrigin: string): string {
  if (!siteOrigin) return path;
  const normalizedPath = path.startsWith("/") ? path : `/${path}`;
  return `${siteOrigin}${normalizedPath}`;
}
