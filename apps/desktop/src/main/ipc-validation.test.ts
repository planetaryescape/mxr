import type { IpcMainInvokeEvent } from "electron";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { describe, expect, it } from "vitest";
import {
  assertTrustedSender,
  validateDesktopSettingsPatch,
  validateExternalUrl,
  validateKnownLocalPath,
  validateOpenDraftInEditorRequest,
} from "./ipc-validation.js";

describe("ipc validation", () => {
  it("accepts only the packaged renderer file as IPC sender", () => {
    const rendererEntry = "/tmp/mxr-renderer/index.html";
    const event = {
      senderFrame: { url: pathToFileURL(rendererEntry).toString() },
    } as unknown as IpcMainInvokeEvent;

    expect(() => assertTrustedSender(event, rendererEntry)).not.toThrow();
    expect(() =>
      assertTrustedSender(
        {
          senderFrame: { url: "https://example.com/" },
        } as unknown as IpcMainInvokeEvent,
        rendererEntry,
      ),
    ).toThrow("unexpected renderer");
  });

  it("allows only explicit external URL protocols", () => {
    expect(validateExternalUrl("https://example.com/path")).toBe("https://example.com/path");
    expect(validateExternalUrl("mailto:test@example.com")).toBe("mailto:test@example.com");
    expect(() => validateExternalUrl("file:///etc/passwd")).toThrow("protocol");
  });

  it("narrows local path opens to mxr artifacts", () => {
    expect(validateKnownLocalPath(join(tmpdir(), "mxr-report-123", "report.html"))).toContain(
      "mxr-report-123",
    );
    expect(validateKnownLocalPath("/Users/test/.local/share/mxr/logs/mxr.log")).toContain(
      "mxr.log",
    );
    expect(() => validateKnownLocalPath("/Users/test/secrets.txt")).toThrow("allowed mxr");
  });

  it("validates draft editor requests", () => {
    expect(
      validateOpenDraftInEditorRequest({
        draftPath: "/tmp/mxr-draft.md",
        editorCommand: "nvim",
        cursorLine: 4,
      }),
    ).toEqual({
      draftPath: "/tmp/mxr-draft.md",
      editorCommand: "nvim",
      cursorLine: 4,
    });
    expect(() =>
      validateOpenDraftInEditorRequest({
        draftPath: "relative.md",
        editorCommand: "nvim",
      }),
    ).toThrow("absolute");
  });

  it("validates desktop settings shape", () => {
    expect(validateDesktopSettingsPatch({ theme: "mxr-light" })).toEqual({
      theme: "mxr-light",
    });
    expect(
      validateDesktopSettingsPatch({
        telemetry: { sentryEnabled: true },
      }),
    ).toEqual({
      telemetry: { sentryEnabled: true },
    });
    expect(() => validateDesktopSettingsPatch({ theme: "unknown" })).toThrow("theme");
    expect(() => validateDesktopSettingsPatch({ telemetry: { sentryEnabled: "yes" } })).toThrow(
      "Sentry telemetry",
    );
    expect(() => validateDesktopSettingsPatch({ keymapOverrides: { nope: {} } })).toThrow(
      "Unknown keymap context",
    );
  });
});
