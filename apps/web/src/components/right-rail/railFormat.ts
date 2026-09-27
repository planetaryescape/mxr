/* Display formatting shared by the right-rail panels. */

export function styleSummary(score: number): string {
  if (score >= 0.68) return `formal (${score.toFixed(2)})`;
  if (score <= 0.38) return `casual (${score.toFixed(2)})`;
  return `neutral (${score.toFixed(2)})`;
}

export function formatNumber(value: number): string {
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 }).format(value);
}

export function formatBytes(value: number): string {
  if (value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${new Intl.NumberFormat(undefined, {
    maximumFractionDigits: size >= 10 || unit === 0 ? 0 : 1,
  }).format(size)} ${units[unit]}`;
}

export function formatCadence(value?: number | null): string {
  if (value == null) return "Unknown";
  if (value < 1) return "<1d";
  return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(value)}d`;
}

export function interactionDirection(delta: number): string {
  if (delta === 0) return "Balanced";
  return delta > 0 ? "Mostly inbound" : "Mostly outbound";
}

export function formatDate(value?: string | null): string {
  if (!value) return "Never";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

export function formatShortDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
  }).format(date);
}
