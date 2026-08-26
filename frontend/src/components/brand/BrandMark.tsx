import { PUBLIC_BRAND_NAME } from "../../lib/brand.ts";

type BrandMarkProps = {
  size?: number;
  className?: string;
  title?: string;
};

/** Clay door mark used in buyer-facing product chrome. */
export function BrandMark({
  size = 28,
  className,
  title = PUBLIC_BRAND_NAME,
}: BrandMarkProps) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 64 64"
      role="img"
      aria-label={title}
    >
      <rect width="64" height="64" rx="14" fill="currentColor" className="brand-mark__tile" />
      <g
        className="brand-mark__door"
        fill="none"
        stroke="#fff"
        strokeWidth="2.6"
        strokeLinecap="round"
        strokeLinejoin="round"
        transform="translate(16 12)"
      >
        <path d="M2 40V6a3 3 0 0 1 3-3h14a3 3 0 0 1 3 3v34" />
        <path d="M22 16h5a2.5 2.5 0 0 1 2.5 2.5V40" />
        <path d="M11 40v-10h6v10" />
      </g>
    </svg>
  );
}
