import type { ReactNode } from "react";
import { labelDef, type NotebookLabelId } from "../../lib/notebook.ts";
import { labelClassToken, labelVisual } from "../../lib/labelVisuals.ts";
import { LabelVisualIcon } from "../../lib/LabelVisualIcon.tsx";

export type LabelPillSurface = "notebook" | "compare" | "fact";
export type LabelPillTone = "neutral" | "positive" | "caution" | "risk" | "info";

type LabelPillProps = {
  labelId?: NotebookLabelId;
  label?: string;
  surface?: LabelPillSurface;
  tone?: LabelPillTone;
  showIcon?: boolean;
  className?: string;
  title?: string;
  onClick?: () => void;
  children?: ReactNode;
};

function pillText(labelId: NotebookLabelId | undefined, label: string | undefined): string {
  if (label) return label;
  if (labelId) return labelDef(labelId).title || labelVisual(labelId).title;
  return "";
}

function pillToken(labelId: NotebookLabelId | undefined, label: string | undefined): string {
  return labelClassToken(labelId ?? label ?? "other");
}

export function LabelPill({
  labelId,
  label,
  surface = "notebook",
  tone = "neutral",
  showIcon = false,
  className = "",
  title,
  onClick,
  children,
}: LabelPillProps) {
  const text = pillText(labelId, label);
  const token = pillToken(labelId, text);
  const classes = [
    "oe-label-pill",
    `oe-label-pill--${surface}`,
    `oe-label-pill--${token}`,
    `oe-label-pill--tone-${tone}`,
    className,
  ].filter(Boolean).join(" ");
  const content = (
    <>
      {showIcon && labelId && <LabelVisualIcon id={labelId} size={18} />}
      <span className="oe-label-pill__text">{text}</span>
      {children}
    </>
  );

  if (onClick) {
    return (
      <button type="button" className={classes} title={title} onClick={onClick}>
        {content}
      </button>
    );
  }

  return (
    <span className={classes} title={title}>
      {content}
    </span>
  );
}
