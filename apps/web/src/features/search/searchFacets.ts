import type { SearchGroupBy } from "./api";

export function facetOperator(groupBy: SearchGroupBy): string {
  return groupBy === "from" ? "from" : groupBy === "list" ? "list" : "category";
}

export function facetValue(key: string, label: string): string {
  const email = label.match(/<([^>]+)>/)?.[1];
  return email ?? key;
}

export function shortFacet(label: string): string {
  return label.replace(/\s*<[^>]+>/, "").trim() || label;
}

export function quoteIfNeeded(value: string): string {
  return /\s/.test(value) ? `"${value}"` : value;
}
