import type { Image } from "@raycast/api";
import { getAvatarIcon } from "@raycast/utils";

// A sender's initials, drawn on this machine: getAvatarIcon returns an inline
// SVG, so no name or address leaves it. It writes the initials into that SVG
// unescaped, so only letters and digits reach it; a sender named "<b>" would
// otherwise break the image.

function wordsOf(text: string | null | undefined): string[] {
  const beforeAt = text?.normalize("NFC").split("@")[0] ?? "";
  return beforeAt.match(/[\p{L}\p{M}\p{N}]+/gu) ?? [];
}

/** Initials in a colour picked from them, the same every time. */
export function senderFace(name: string | null | undefined, email?: string | null): Image.Asset {
  const fromName = wordsOf(name);
  const words = fromName.length > 0 ? fromName : wordsOf(email);
  return getAvatarIcon(words.join(" ") || "?");
}
