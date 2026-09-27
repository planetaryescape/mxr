/*
 * Label chips for the reader: the label's own colour when it has one,
 * otherwise a stable colour from its name.
 */

import type { CSSProperties } from "react";

import { Badge } from "@/components/ui/badge";
import type { MessageLabelView } from "@/features/mailbox/types";

export function LabelBadge({ label }: { label: MessageLabelView }) {
  const style = labelBadgeStyle(labelDisplayColor(label));
  return (
    <Badge variant={style ? "outline" : "secondary"} style={style} title={label.name}>
      {style ? (
        <span className="size-1.5 rounded-full" style={{ backgroundColor: style.color }} />
      ) : null}
      {label.name}
    </Badge>
  );
}

function labelDisplayColor(label: MessageLabelView): string | null {
  return normalizeHexColor(label.color) ?? fallbackLabelColor(label.name);
}

function labelBadgeStyle(color?: string | null): CSSProperties | undefined {
  const hex = normalizeHexColor(color);
  if (!hex) return undefined;
  return {
    backgroundColor: hexToRgba(hex, 0.16),
    borderColor: hexToRgba(hex, 0.65),
    // The raw label colour is often too light or dark for text on its own
    // tint; pulling it toward the theme's text colour keeps it readable.
    color: `color-mix(in oklab, ${hex} 55%, var(--foreground))`,
  };
}

function normalizeHexColor(value?: string | null): string | null {
  const trimmed = value?.trim();
  if (!trimmed) return null;
  const short = trimmed.match(/^#([0-9a-f]{3})$/i);
  if (short) {
    return `#${short[1]!
      .split("")
      .map((part) => part + part)
      .join("")}`;
  }
  const long = trimmed.match(/^#([0-9a-f]{6})$/i);
  return long ? `#${long[1]}` : null;
}

function fallbackLabelColor(name: string): string {
  switch (name.toUpperCase()) {
    case "INBOX":
      return "#60a5fa";
    case "STARRED":
    case "IMPORTANT":
      return "#facc15";
    case "SENT":
      return "#9ca3af";
    case "DRAFT":
      return "#d946ef";
    case "TRASH":
      return "#f87171";
    case "SPAM":
      return "#fb923c";
    case "ARCHIVE":
    case "ALL MAIL":
      return "#6b7280";
    default: {
      const colors = [
        "#60a5fa",
        "#34d399",
        "#fb923c",
        "#a78bfa",
        "#fb7185",
        "#38bdf8",
        "#fdba74",
        "#86efac",
      ];
      const hash = [...name].reduce((acc, char) => (acc + char.charCodeAt(0)) % 256, 0);
      return colors[hash % colors.length]!;
    }
  }
}

export function hexToRgba(hex: string, alpha: number): string {
  const value = hex.slice(1);
  const red = Number.parseInt(value.slice(0, 2), 16);
  const green = Number.parseInt(value.slice(2, 4), 16);
  const blue = Number.parseInt(value.slice(4, 6), 16);
  return `rgba(${red}, ${green}, ${blue}, ${alpha})`;
}
