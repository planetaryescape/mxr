/** Escape sender-controlled text before it is placed in Raycast markdown. */
export function escapeLine(line: string): string {
  if (/^\s*([-=])\1{2,}\s*$/.test(line)) return `\\${line.trim()}`;
  return line
    .replace(/([\\`*_[\]<>#|~])/g, "\\$1")
    .replace(/^(\s*)(\d+)\.(\s)/, "$1$2\\.$3")
    .replace(/^(\s*)([-+])(\s)/, "$1\\$2$3");
}
