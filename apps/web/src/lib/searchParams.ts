/*
 * Tiny route search-param validators. Routes' validateSearch runs in the
 * entry chunk, so these stay dependency-free instead of pulling zod into
 * every page load.
 */

export function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

export function optionalEnum<T extends string>(value: unknown, allowed: readonly T[]): T | undefined {
  return typeof value === "string" && (allowed as readonly string[]).includes(value) ? (value as T) : undefined;
}
