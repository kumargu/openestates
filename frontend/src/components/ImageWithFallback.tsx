import { useState } from "react";

export function ImageWithFallback({
  src,
  alt,
  className,
  loading = "lazy",
}: {
  src: string | null;
  alt: string;
  className?: string;
  loading?: "lazy" | "eager";
}) {
  const [failedSrc, setFailedSrc] = useState<string | null>(null);

  const isPlaceholder = !src || src.startsWith("placeholder://");
  const failed = Boolean(src && failedSrc === src);

  if (isPlaceholder || failed) {
    return (
      <div
        className={`image-placeholder${className ? ` ${className}` : ""}`}
        role={alt ? "img" : undefined}
        aria-label={alt || undefined}
        aria-hidden={alt ? undefined : true}
      >
        <span className="image-placeholder__mark" aria-hidden="true">
          <svg viewBox="0 0 48 48" fill="none">
            <path d="M11 39V18L24 9l13 9v21" />
            <path d="M18 39V25h12v14M17 19h14" />
          </svg>
        </span>
      </div>
    );
  }

  return (
    <img
      src={src}
      alt={alt}
      className={className}
      loading={loading}
      onError={() => setFailedSrc(src)}
    />
  );
}
