const paths = {
  plus: "M12 5v14M5 12h14",
  minus: "M5 12h14",
  rotate: "M20 7v5h-5M19.5 12a7.5 7.5 0 1 0-2 5.1",
  top: "m12 3 9 5-9 5-9-5 9-5ZM3 12l9 5 9-5M3 16l9 5 9-5",
  outline: "m4 7 11-3 5 13-13 3Z",
  close: "m6 6 12 12M6 18 18 6",
  photos: "M4 4h16v16H4V4Zm0 12 5-5 4 4 3-3 4 4M15 8h.01",
  reviews: "M4 4h16v12H9l-5 4V4Zm4 4h8M8 12h5",
  play: "m8 5 11 7-11 7V5Z",
  pause: "M9 5v14M15 5v14",
  previous: "m14 6-6 6 6 6",
  next: "m10 6 6 6-6 6",
} as const;

export function AtlasIcon({ name }: { name: keyof typeof paths }) {
  return (
    <svg className="property-atlas__icon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
      <path d={paths[name]} strokeDasharray={name === "outline" ? "3 2" : undefined} />
    </svg>
  );
}
