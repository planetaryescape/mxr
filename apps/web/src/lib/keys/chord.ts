/*
 * Key chords. A chord is one or more space-separated tokens ("g i", "?",
 * "Mod+k", "Shift+Enter"). Tokens come from `KeyboardEvent.key`, not
 * `code`, so "?" and "/" are different keys on every layout and the table
 * reads like the TUI's help. Shift is implied in printable characters
 * ("G", "#") and spelled out only for named keys ("Shift+Enter") or with
 * other modifiers ("Mod+Shift+p").
 */

export type KeyToken = string;

const NAMED_KEYS = new Set([
  "Enter",
  "Escape",
  "Tab",
  "Backspace",
  "Delete",
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "Space",
]);

const MODIFIER_KEYS = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock", "Fn", "OS"]);

export function isMacPlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  const platform =
    (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ??
    navigator.platform ??
    "";
  return /mac|iphone|ipad/i.test(platform);
}

/**
 * Normalise a keydown into a token, or null for a bare modifier press.
 * `Mod` is Cmd on macOS and Ctrl elsewhere; a literal Ctrl on macOS stays
 * "Ctrl" so TUI chords like Ctrl-d work there without stealing Cmd.
 */
export function tokenFromEvent(event: KeyboardEvent, mac = isMacPlatform()): KeyToken | null {
  const rawKey = event.key;
  if (!rawKey || MODIFIER_KEYS.has(rawKey)) return null;
  const key = rawKey === " " ? "Space" : rawKey;

  const modifiers: string[] = [];
  const mod = mac ? event.metaKey : event.ctrlKey;
  const ctrl = mac ? event.ctrlKey : false;
  if (mod) modifiers.push("Mod");
  if (ctrl) modifiers.push("Ctrl");
  if (event.altKey) modifiers.push("Alt");
  if (!mac && event.metaKey) modifiers.push("Meta");

  const named = NAMED_KEYS.has(key) || key.length > 1;
  if (named) {
    if (event.shiftKey) modifiers.push("Shift");
    return [...modifiers, key].join("+");
  }
  if (modifiers.length > 0) {
    // With Ctrl/Cmd the browser reports an uppercase letter for Shift;
    // spell Shift out so "Mod+Shift+p" is unambiguous.
    const lower = key.toLowerCase();
    if (event.shiftKey && lower !== key) modifiers.push("Shift");
    return [...modifiers, lower].join("+");
  }
  return key;
}

export function parseChord(chord: string): KeyToken[] {
  return chord.trim().split(/\s+/).filter(Boolean);
}

const DISPLAY: Record<string, string> = {
  Enter: "Enter",
  Escape: "Esc",
  Backspace: "⌫",
  Delete: "Del",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Space: "Space",
  Tab: "Tab",
};

/** Human label for a chord: "g i" → "g i", "Mod+k" → "⌘K" on macOS. */
export function formatChord(chord: string, mac = isMacPlatform()): string {
  return parseChord(chord)
    .map((token) => formatToken(token, mac))
    .join(" ");
}

function formatToken(token: KeyToken, mac: boolean): string {
  const parts = token.split("+");
  // "+" itself as a key arrives as a trailing empty part.
  const key = parts.at(-1) === "" ? "+" : (parts.at(-1) ?? "");
  const modifiers = parts.slice(0, parts.at(-1) === "" ? -2 : -1);
  const label = DISPLAY[key] ?? (modifiers.length > 0 ? key.toUpperCase() : key);
  const prefix = modifiers
    .map((modifier) => {
      if (modifier === "Mod") return mac ? "⌘" : "Ctrl+";
      if (modifier === "Ctrl") return mac ? "⌃" : "Ctrl+";
      if (modifier === "Alt") return mac ? "⌥" : "Alt+";
      if (modifier === "Shift") return mac ? "⇧" : "Shift+";
      return `${modifier}+`;
    })
    .join("");
  return `${prefix}${label}`;
}
