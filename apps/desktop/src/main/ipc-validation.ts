import type { IpcMainInvokeEvent } from "electron";
import { tmpdir } from "node:os";
import {
  basename,
  dirname,
  isAbsolute,
  normalize,
  relative,
  sep,
} from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import type {
  DesktopKeymapContext,
  DesktopSettingsPatch,
  DesktopThemeId,
  OpenBrowserDocumentRequest,
  OpenDraftInEditorRequest,
} from "../shared/types.js";

const DESKTOP_THEMES = new Set<DesktopThemeId>([
  "mxr-dark",
  "mxr-light",
  "catppuccin-mocha",
  "gruvbox-dark",
  "nightfox",
  "kanagawa-wave",
  "one-dark",
]);

const KEYMAP_CONTEXTS = new Set<DesktopKeymapContext>([
  "mailList",
  "threadView",
  "messageView",
  "rules",
  "accounts",
  "diagnostics",
]);

export function assertTrustedSender(
  event: IpcMainInvokeEvent,
  rendererEntry: string,
): void {
  const senderUrl = event.senderFrame?.url;
  if (!senderUrl) {
    throw new Error("Rejected IPC from unknown sender");
  }

  const expected = pathToFileURL(rendererEntry).toString();
  if (stripUrlNoise(senderUrl) !== stripUrlNoise(expected)) {
    throw new Error("Rejected IPC from unexpected renderer");
  }
}

export function validateExternalBinaryPath(value: unknown): string {
  const path = asString(value, "binary path");
  rejectNullByte(path, "binary path");
  return path;
}

export function validateDesktopSettingsPatch(value: unknown): DesktopSettingsPatch {
  if (!isPlainObject(value)) {
    throw new Error("Desktop settings patch must be an object");
  }

  const patch: DesktopSettingsPatch = {};
  if ("theme" in value) {
    if (typeof value.theme !== "string" || !DESKTOP_THEMES.has(value.theme as DesktopThemeId)) {
      throw new Error("Desktop settings theme is invalid");
    }
    patch.theme = value.theme as DesktopThemeId;
  }

  if ("keymapOverrides" in value) {
    if (!isPlainObject(value.keymapOverrides)) {
      throw new Error("Desktop keymap overrides must be an object");
    }
    const overrides: NonNullable<DesktopSettingsPatch["keymapOverrides"]> = {};
    for (const [context, bindings] of Object.entries(value.keymapOverrides)) {
      if (!KEYMAP_CONTEXTS.has(context as DesktopKeymapContext)) {
        throw new Error(`Unknown keymap context: ${context}`);
      }
      if (!isPlainObject(bindings)) {
        throw new Error(`Keymap bindings for ${context} must be an object`);
      }
      overrides[context as DesktopKeymapContext] = {};
      for (const [key, action] of Object.entries(bindings)) {
        if (typeof key !== "string" || typeof action !== "string") {
          throw new Error(`Invalid keymap binding in ${context}`);
        }
        overrides[context as DesktopKeymapContext]![key] = action;
      }
    }
    patch.keymapOverrides = overrides;
  }

  if ("telemetry" in value) {
    if (!isPlainObject(value.telemetry)) {
      throw new Error("Desktop telemetry settings must be an object");
    }
    const telemetry: NonNullable<DesktopSettingsPatch["telemetry"]> = {};
    if ("sentryEnabled" in value.telemetry) {
      if (typeof value.telemetry.sentryEnabled !== "boolean") {
        throw new Error("Sentry telemetry setting must be boolean");
      }
      telemetry.sentryEnabled = value.telemetry.sentryEnabled;
    }
    patch.telemetry = telemetry;
  }

  return patch;
}

export function validateOpenDraftInEditorRequest(
  value: unknown,
): OpenDraftInEditorRequest {
  if (!isPlainObject(value)) {
    throw new Error("Editor request must be an object");
  }
  const draftPath = asString(value.draftPath, "draft path");
  const editorCommand = asString(value.editorCommand, "editor command");
  rejectNullByte(draftPath, "draft path");
  rejectNullByte(editorCommand, "editor command");
  if (!isAbsolute(draftPath)) {
    throw new Error("Draft path must be absolute");
  }
  if (!editorCommand.trim()) {
    throw new Error("Editor command is empty");
  }

  const request: OpenDraftInEditorRequest = { draftPath, editorCommand };
  if (value.cursorLine !== undefined) {
    const cursorLine = value.cursorLine;
    if (typeof cursorLine !== "number" || !Number.isInteger(cursorLine) || cursorLine < 1) {
      throw new Error("Cursor line must be a positive integer");
    }
    request.cursorLine = cursorLine;
  }
  return request;
}

export function validateOpenBrowserDocumentRequest(
  value: unknown,
): OpenBrowserDocumentRequest {
  if (!isPlainObject(value)) {
    throw new Error("Browser document request must be an object");
  }
  const title = boundedString(value.title, "document title", 1_000);
  const html = boundedString(value.html, "document html", 20 * 1024 * 1024);
  const request: OpenBrowserDocumentRequest = { title, html };

  if (value.suggestedFilename !== undefined) {
    request.suggestedFilename = boundedString(
      value.suggestedFilename,
      "document filename",
      255,
    );
  }
  return request;
}

export function validateExternalUrl(value: unknown): string {
  const raw = asString(value, "external URL");
  const url = new URL(raw);
  if (!["https:", "http:", "mailto:"].includes(url.protocol)) {
    throw new Error(`External URL protocol is not allowed: ${url.protocol}`);
  }
  return url.toString();
}

export function validateKnownLocalPath(value: unknown): string {
  const path = asString(value, "local path");
  rejectNullByte(path, "local path");
  if (!isAbsolute(path)) {
    throw new Error("Local path must be absolute");
  }

  const normalized = normalize(path);
  if (isMxrGeneratedTempArtifact(normalized) || isMxrLogPath(normalized)) {
    return normalized;
  }

  throw new Error("Local path is not an allowed mxr artifact");
}

function stripUrlNoise(value: string): string {
  const url = new URL(value);
  url.hash = "";
  url.search = "";
  if (url.protocol === "file:") {
    return pathToFileURL(fileURLToPath(url)).toString();
  }
  return url.toString();
}

function isMxrGeneratedTempArtifact(path: string): boolean {
  if (!isWithin(tmpdir(), path)) {
    return false;
  }
  return basename(path).startsWith("mxr-") || basename(dirname(path)).startsWith("mxr-");
}

function isMxrLogPath(path: string): boolean {
  const parts = path.split(sep);
  return basename(path) === "mxr.log" && parts.includes("logs");
}

function isWithin(parent: string, child: string): boolean {
  const rel = relative(normalize(parent), normalize(child));
  return rel === "" || (!rel.startsWith("..") && !isAbsolute(rel));
}

function boundedString(value: unknown, label: string, maxLength: number): string {
  const result = asString(value, label);
  if (result.length > maxLength) {
    throw new Error(`${label} is too large`);
  }
  return result;
}

function asString(value: unknown, label: string): string {
  if (typeof value !== "string") {
    throw new Error(`${label} must be a string`);
  }
  return value;
}

function rejectNullByte(value: string, label: string): void {
  if (value.includes("\0")) {
    throw new Error(`${label} contains a null byte`);
  }
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
