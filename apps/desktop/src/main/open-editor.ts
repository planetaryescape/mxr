import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const TERMINAL_EDITOR_PATTERNS = [
  /\b(?:vi|vim|nvim)\b/i,
  /\b(?:hx|helix)\b/i,
  /\b(?:nano|pico)\b/i,
  /\b(?:emacs|kak|micro)\b/i,
];

export async function openDraftInEditor(request: {
  draftPath: string;
  editorCommand: string;
  cursorLine?: number;
}): Promise<{ ok: true }> {
  const command = buildEditorCommand(request.editorCommand, request.draftPath, request.cursorLine);

  if (process.platform === "darwin" && looksTerminalEditor(request.editorCommand)) {
    await execFileAsync("osascript", [
      "-e",
      'tell application "Terminal" to activate',
      "-e",
      `tell application "Terminal" to do script ${appleScriptString(command)}`,
    ]);
    return { ok: true };
  }

  const child = spawn(command, {
    detached: true,
    shell: true,
    stdio: "ignore",
  });
  child.unref();

  return { ok: true };
}

function buildEditorCommand(editorCommand: string, draftPath: string, cursorLine?: number) {
  const lower = editorCommand.toLowerCase();
  if (cursorLine) {
    if (lower.includes("vim") || lower === "vi" || lower.includes("nvim")) {
      return `${editorCommand} +${cursorLine} ${shellQuote(draftPath)}`;
    }
    if (lower.includes("hx") || lower.includes("helix")) {
      return `${editorCommand} ${shellQuote(`${draftPath}:${cursorLine}`)}`;
    }
  }

  return `${editorCommand} ${shellQuote(draftPath)}`;
}

function looksTerminalEditor(editorCommand: string) {
  return TERMINAL_EDITOR_PATTERNS.some((pattern) => pattern.test(editorCommand));
}

function shellQuote(value: string) {
  return `'${value.replaceAll("'", "'\\''")}'`;
}

function appleScriptString(value: string) {
  return `"${value.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"`;
}
