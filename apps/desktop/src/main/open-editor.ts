import { execFile, spawn } from "node:child_process";
import { basename } from "node:path";
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
  const launch = buildEditorLaunch(request.editorCommand, request.draftPath, request.cursorLine);

  if (process.platform === "darwin" && looksTerminalEditor(launch.executable)) {
    await execFileAsync("osascript", [
      "-e",
      'tell application "Terminal" to activate',
      "-e",
      `tell application "Terminal" to do script ${appleScriptString(buildTerminalCommand(launch))}`,
    ]);
    return { ok: true };
  }

  const child = spawn(launch.executable, launch.args, {
    detached: true,
    stdio: "ignore",
  });
  child.unref();

  return { ok: true };
}

export function buildEditorLaunch(
  editorCommand: string,
  draftPath: string,
  cursorLine?: number,
): { executable: string; args: string[] } {
  const [executable, ...baseArgs] = splitEditorCommand(editorCommand);
  if (!executable || executable.startsWith("-")) {
    throw new Error("$EDITOR must start with an executable");
  }

  const editorName = basename(executable).toLowerCase();
  const args = [...baseArgs];
  if (cursorLine) {
    if (["vi", "vim", "nvim"].includes(editorName)) {
      args.push(`+${cursorLine}`, draftPath);
      return { executable, args };
    }
    if (["hx", "helix"].includes(editorName)) {
      args.push(`${draftPath}:${cursorLine}`);
      return { executable, args };
    }
  }

  args.push(draftPath);
  return { executable, args };
}

function splitEditorCommand(editorCommand: string): string[] {
  const tokens: string[] = [];
  let current = "";
  let quote: "'" | '"' | null = null;
  let escaping = false;

  for (const char of editorCommand.trim()) {
    if (escaping) {
      current += char;
      escaping = false;
      continue;
    }

    if (char === "\\" && quote !== "'") {
      escaping = true;
      continue;
    }

    if (quote) {
      if (char === quote) {
        quote = null;
      } else {
        current += char;
      }
      continue;
    }

    if (char === "'" || char === '"') {
      quote = char;
      continue;
    }

    if (/\s/.test(char)) {
      if (current) {
        tokens.push(current);
        current = "";
      }
      continue;
    }

    if (/[|&;<>()`]/.test(char) || char.charCodeAt(0) < 32) {
      throw new Error("$EDITOR contains shell syntax that is not supported");
    }

    current += char;
  }

  if (escaping) {
    current += "\\";
  }
  if (quote) {
    throw new Error("$EDITOR contains an unterminated quote");
  }
  if (current) {
    tokens.push(current);
  }
  if (tokens.length === 0) {
    throw new Error("$EDITOR is empty");
  }
  return tokens;
}

function looksTerminalEditor(executable: string) {
  const editorName = basename(executable);
  return TERMINAL_EDITOR_PATTERNS.some((pattern) => pattern.test(editorName));
}

function buildTerminalCommand(launch: { executable: string; args: string[] }) {
  return [launch.executable, ...launch.args].map(shellQuote).join(" ");
}

function shellQuote(value: string) {
  return `'${value.replaceAll("'", "'\\''")}'`;
}

function appleScriptString(value: string) {
  return `"${value.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"`;
}
