const EMAIL_PATTERN = /\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b/gi;
const TOKEN_PATTERN =
  /\b(?:bearer\s+)?[A-Za-z0-9_-]{24,}\.[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{16,}\b/gi;
const UNIX_HOME_PATTERN = /\/(?:Users|home)\/[^/\s]+/g;
const SENSITIVE_KEY_PATTERN =
  /authorization|cookie|password|secret|token|subject|body|sender|recipient|from|to|cc|bcc/i;

export function redactSentryEvent<T>(event: T): T {
  const redacted = scrubValue(event) as Record<string, unknown>;
  delete redacted.request;
  delete redacted.extra;
  delete redacted.breadcrumbs;
  delete redacted.user;
  return redacted as T;
}

function scrubValue(value: unknown, key = ""): unknown {
  if (value == null) {
    return value;
  }

  if (typeof value === "string") {
    if (SENSITIVE_KEY_PATTERN.test(key)) {
      return "[Filtered]";
    }
    return redactString(value);
  }

  if (Array.isArray(value)) {
    return value.map((item) => scrubValue(item, key));
  }

  if (typeof value === "object") {
    const output: Record<string, unknown> = {};
    for (const [entryKey, entryValue] of Object.entries(value)) {
      output[entryKey] = scrubValue(entryValue, entryKey);
    }
    return output;
  }

  return value;
}

function redactString(value: string): string {
  return value
    .replace(EMAIL_PATTERN, "[email]")
    .replace(TOKEN_PATTERN, "[token]")
    .replace(UNIX_HOME_PATTERN, (match) => {
      const [root] = match.split("/").filter(Boolean);
      return `/${root}/[user]`;
    });
}
