export interface SearchToken {
  raw: string;
  kind: "operator" | "term";
  label: string;
}

const operatorPattern =
  /^(from|to|cc|subject|label|has|is|older_than|newer_than|before|after):(.+)$/i;

export function parseSearchTokens(query: string): SearchToken[] {
  return query
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .map((raw) => {
      const match = raw.match(operatorPattern);
      if (!match) return { raw, kind: "term", label: raw };
      return { raw, kind: "operator", label: `${match[1]}: ${match[2]}` };
    });
}

export function removeSearchToken(query: string, token: SearchToken): string {
  const parts = query.trim().split(/\s+/).filter(Boolean);
  const index = parts.indexOf(token.raw);
  if (index >= 0) parts.splice(index, 1);
  return parts.join(" ");
}

export const searchSyntaxRows = [
  ["from:alice@example.com", "Sender"],
  ["to:me@example.com", "Recipient"],
  ["subject:invoice", "Subject text"],
  ["has:attachment", "Messages with attachments"],
  ["is:unread", "Unread messages"],
  ["is:starred", "Starred messages"],
  ["older_than:7d", "Older than 7 days"],
  ["newer_than:1d", "Newer than 1 day"],
  ["before:2026-01-01", "Before date"],
  ["after:2026-01-01", "After date"],
] as const;
