export const NOTEBOOK_SAVE_ICON_PATH =
  "M7 4.6h10a1.7 1.7 0 0 1 1.7 1.7v14.2L12 16.2l-6.7 4.3V6.3A1.7 1.7 0 0 1 7 4.6Z";

export function NotebookSaveIcon({
  filled = false,
  size = 16,
}: {
  filled?: boolean;
  size?: number;
}) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
      <path
        d={NOTEBOOK_SAVE_ICON_PATH}
        fill={filled ? "currentColor" : "none"}
        stroke="currentColor"
        strokeWidth="1.9"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
