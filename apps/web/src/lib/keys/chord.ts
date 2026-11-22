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

export function tokenFromEvent(
  event: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">,
  mac = isMacPlatform(),
): KeyToken | null {
  const rawKey = event.key;
  if (!rawKey || MODIFIER_KEYS.has(rawKey)) return null;
  const key = rawKey === " " ? "Space" : rawKey;
  if (isAltComposed(event, key, mac)) return key;

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
    const lower = key.toLowerCase();
    if (event.shiftKey && lower !== key) modifiers.push("Shift");
    return [...modifiers, lower].join("+");
  }
  return key;
}

function isAltComposed(
  event: Pick<KeyboardEvent, "metaKey" | "ctrlKey" | "altKey">,
  key: string,
  mac: boolean,
): boolean {
  if (!event.altKey || [...key].length !== 1 || /^[a-z0-9]$/i.test(key)) return false;
  return mac ? !event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
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

export function formatChord(chord: string, mac = isMacPlatform()): string {
  return parseChord(chord)
    .map((token) => formatToken(token, mac))
    .join(" ");
}

function formatToken(token: KeyToken, mac: boolean): string {
  const parts = token.split("+");
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
