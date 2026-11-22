const TONES = [
  "avatar-tone [--tone:var(--chart-1)]",
  "avatar-tone [--tone:var(--chart-2)]",
  "avatar-tone [--tone:var(--chart-3)]",
  "avatar-tone [--tone:var(--chart-4)]",
  "avatar-tone [--tone:var(--chart-5)]",
  "avatar-tone [--tone:var(--chart-6)]",
] as const;

export function avatarTone(name: string): string {
  const hash = [...name].reduce((acc, char) => (acc * 31 + char.charCodeAt(0)) >>> 0, 7);
  return TONES[hash % TONES.length] ?? TONES[0];
}
