import { useState } from "react";

export function ImageWithFallback({
  src,
  alt,
  style,
}: {
  src: string | null;
  alt: string;
  style?: React.CSSProperties;
}) {
  const [failed, setFailed] = useState(false);

  const isPlaceholder = !src || src.startsWith("placeholder://");

  if (isPlaceholder || failed) {
    return (
      <div className="image-placeholder" style={style}>
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
          <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
          <circle cx="8.5" cy="8.5" r="1.5" />
          <polyline points="21 15 16 10 5 21" />
        </svg>
      </div>
    );
  }

  return (
    <img
      src={src}
      alt={alt}
      style={{ objectFit: "cover", ...style }}
      onError={() => setFailed(true)}
    />
  );
}
