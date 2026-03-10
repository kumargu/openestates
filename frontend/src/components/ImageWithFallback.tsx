import { useState } from "react";

// Generate a consistent color from a string
function stringToColor(str: string): string {
  const colors = [
    "#1a365d", "#2d3748", "#22543d", "#5b2c6f", "#7b341e",
    "#1a202c", "#2c5282", "#276749", "#6b46c1", "#9c4221",
    "#2b6cb0", "#38a169", "#805ad5", "#dd6b20", "#e53e3e",
  ];
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = str.charCodeAt(i) + ((hash << 5) - hash);
  }
  return colors[Math.abs(hash) % colors.length];
}

function getInitials(name: string): string {
  const words = name.replace(/\d+\s*BHK\s+in\s+/i, "").split(/\s+/);
  if (words.length >= 2) {
    return (words[0][0] + words[1][0]).toUpperCase();
  }
  return (words[0]?.[0] || "?").toUpperCase();
}

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
    const bg = stringToColor(alt);
    const initials = getInitials(alt);
    return (
      <div
        className="image-placeholder"
        style={{
          ...style,
          background: `linear-gradient(135deg, ${bg}, ${bg}dd)`,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          gap: "0.3rem",
        }}
      >
        <span style={{ fontSize: "1.8rem", fontWeight: 700, color: "rgba(255,255,255,0.9)", letterSpacing: "0.05em" }}>
          {initials}
        </span>
        <span style={{ fontSize: "0.65rem", color: "rgba(255,255,255,0.5)", textTransform: "uppercase", letterSpacing: "0.08em" }}>
          Photo coming soon
        </span>
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
