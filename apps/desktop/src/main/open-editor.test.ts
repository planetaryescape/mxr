import { describe, expect, it } from "vitest";
import { buildEditorLaunch } from "./open-editor.js";

describe("buildEditorLaunch", () => {
  it("spawns an executable with args instead of a shell command string", () => {
    expect(buildEditorLaunch("code --wait", "/tmp/mxr-draft.md")).toEqual({
      executable: "code",
      args: ["--wait", "/tmp/mxr-draft.md"],
    });
  });

  it("preserves quoted editor args", () => {
    expect(buildEditorLaunch('"/Applications/Editor App.app/Contents/MacOS/editor" --reuse-window', "/tmp/mxr-draft.md")).toEqual({
      executable: "/Applications/Editor App.app/Contents/MacOS/editor",
      args: ["--reuse-window", "/tmp/mxr-draft.md"],
    });
  });

  it("adds cursor flags for terminal editors", () => {
    expect(buildEditorLaunch("nvim", "/tmp/mxr-draft.md", 12)).toEqual({
      executable: "nvim",
      args: ["+12", "/tmp/mxr-draft.md"],
    });

    expect(buildEditorLaunch("hx", "/tmp/mxr-draft.md", 12)).toEqual({
      executable: "hx",
      args: ["/tmp/mxr-draft.md:12"],
    });
  });

  it("rejects shell syntax", () => {
    expect(() => buildEditorLaunch("vim; rm -rf /", "/tmp/mxr-draft.md")).toThrow(
      "shell syntax",
    );
  });
});
