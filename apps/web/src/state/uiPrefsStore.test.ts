/* @vitest-environment jsdom */

import { describe, expect, test } from "vitest";

import {
  emailHtmlThemeForSender,
  applyContrastAttribute,
  migrateUiPrefs,
  resolveContrast,
  watchSystemContrast,
  readEmailHtmlSenderOverrides,
  useUiPrefs,
} from "./uiPrefsStore";

describe("compose editor preference", () => {
  test("someone with no saved prefs gets the CodeMirror + vim editor", () => {
    expect(useUiPrefs.getInitialState().composeEditor).toBe("codemirror-vim");
  });

  test("a saved Rich text choice is kept, not migrated to the new default", async () => {
    // Written through the store's own storage so the test holds whether the
    // runtime exposes a working localStorage or the in-memory fallback.
    await useUiPrefs.persist
      .getOptions()
      .storage?.setItem("mxr.uiPrefs", { state: { composeEditor: "tiptap" }, version: 2 });
    await useUiPrefs.persist.rehydrate();
    expect(useUiPrefs.getState().composeEditor).toBe("tiptap");
  });
});

describe("theme preference", () => {
  test("someone with no saved prefs follows the OS appearance", () => {
    expect(useUiPrefs.getInitialState().theme).toBe("system");
  });

  test("new installs use Auto and saved explicit canvas choices survive migration", () => {
    expect(useUiPrefs.getInitialState().emailHtmlTheme).toBe("auto");
    expect(migrateUiPrefs({ emailHtmlTheme: "dark" }, 4)).toEqual({
      emailHtmlTheme: "dark",
      contrast: "system",
    });
    expect(migrateUiPrefs({ emailHtmlTheme: "original" }, 4)).toEqual({
      emailHtmlTheme: "original",
      contrast: "system",
    });
    expect(migrateUiPrefs({ emailHtmlTheme: "unexpected" }, 4)).toEqual({
      contrast: "system",
    });
  });

  test("sender overrides normalize identity and reject malformed map entries", () => {
    const overrides = readEmailHtmlSenderOverrides([
      { sender: "  News@Example.COM ", theme: "original" },
      { sender: "news@example.com", theme: "dark" },
      { sender: "constructor", theme: "original" },
      { sender: "bad@example.com", theme: "auto" },
      { theme: "dark" },
    ]);
    expect(overrides).toEqual([
      { sender: "news@example.com", theme: "dark" },
      { sender: "constructor", theme: "original" },
    ]);
    expect(emailHtmlThemeForSender(overrides, "NEWS@example.com")).toBe("dark");
    expect(emailHtmlThemeForSender(overrides, "other@example.com")).toBeUndefined();
    expect(emailHtmlThemeForSender({ constructor: "dark" }, "constructor")).toBeUndefined();
  });
});

describe("contrast preference", () => {
  test("defaults to system and migrates existing v5 preferences without changing saved choices", () => {
    expect(useUiPrefs.getInitialState().contrast).toBe("system");
    expect(migrateUiPrefs({ theme: "paper" }, 5)).toEqual({
      theme: "paper",
      contrast: "system",
    });
    expect(migrateUiPrefs({ contrast: "increased" }, 5)).toEqual({
      contrast: "system",
    });
    expect(
      migrateUiPrefs(
        {
          emailHtmlTheme: "original",
          emailHtmlSenderOverrides: [{ sender: "news@example.com", theme: "dark" }],
        },
        5,
      ),
    ).toEqual({
      emailHtmlTheme: "original",
      emailHtmlSenderOverrides: [{ sender: "news@example.com", theme: "dark" }],
      contrast: "system",
    });
  });

  test("system resolves only increased contrast; explicit choices ignore the OS", () => {
    const original = window.matchMedia;
    // The test only exercises the MediaQueryList members the resolver reads.
    window.matchMedia = ((query: string) => ({
      matches: query === "(prefers-contrast: more)",
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
    })) as unknown as typeof window.matchMedia;
    try {
      expect(resolveContrast("system")).toBe("increased");
      expect(resolveContrast("standard")).toBe("standard");
      expect(resolveContrast("increased")).toBe("increased");
      applyContrastAttribute("standard");
      expect(document.documentElement.dataset.contrast).toBe("standard");
    } finally {
      window.matchMedia = original;
    }
  });

  test("a system change is live, but explicit preferences stay in force", () => {
    let listener: (() => void) | undefined;
    let matches = false;
    const original = window.matchMedia;
    // Capture change listeners to exercise subscribe, explicit override, and cleanup.
    window.matchMedia = ((query: string) => ({
      matches,
      media: query,
      addEventListener: (_type: string, callback: () => void) => {
        listener = callback;
      },
      removeEventListener: (_type: string, callback: () => void) => {
        if (listener === callback) listener = undefined;
      },
    })) as unknown as typeof window.matchMedia;
    const previous = useUiPrefs.getState().contrast;
    try {
      useUiPrefs.getState().setContrast("system");
      const stop = watchSystemContrast();
      matches = true;
      listener?.();
      expect(document.documentElement.dataset.contrast).toBe("increased");
      useUiPrefs.getState().setContrast("standard");
      applyContrastAttribute("standard");
      matches = false;
      listener?.();
      expect(document.documentElement.dataset.contrast).toBe("standard");
      stop();
      expect(listener).toBeUndefined();
    } finally {
      useUiPrefs.getState().setContrast(previous);
      window.matchMedia = original;
    }
  });

  test("persists the selected contrast value", async () => {
    const previous = useUiPrefs.getState().contrast;
    try {
      useUiPrefs.getState().setContrast("increased");
      const saved = await useUiPrefs.persist.getOptions().storage?.getItem("mxr.uiPrefs");
      expect(saved).toEqual(
        expect.objectContaining({
          state: expect.objectContaining({ contrast: "increased" }),
        }),
      );
    } finally {
      useUiPrefs.getState().setContrast(previous);
    }
  });
});

describe("home and sidebar prefs", () => {
  test("new installs land on Now with only places unfolded", () => {
    const initial = useUiPrefs.getInitialState();
    expect(initial.home).toBe("now");
    expect(initial.collapsedSections).toEqual(["more", "labels", "tools"]);
  });

  test("saved v2 prefs keep their choices, fold More and Labels and land on Now", () => {
    expect(migrateUiPrefs({ theme: "paper", collapsedSections: ["tools", "saved"] }, 2)).toEqual({
      theme: "paper",
      collapsedSections: ["tools", "saved", "more", "labels"],
      home: "now",
      contrast: "system",
    });
    expect(migrateUiPrefs(undefined, 1)).toEqual({
      collapsedSections: ["tools", "more", "labels"],
      home: "now",
      contrast: "system",
    });
  });

  test("a saved desk home becomes Now and an inbox home stays", () => {
    expect(migrateUiPrefs({ home: "desk", collapsedSections: [] }, 3)).toEqual({
      home: "now",
      collapsedSections: [],
      contrast: "system",
    });
    const inbox = { home: "inbox", collapsedSections: [] };
    expect(migrateUiPrefs(inbox, 3)).toEqual({ ...inbox, contrast: "system" });
  });

  test("v6 prefs pass through untouched", () => {
    const saved = { home: "now", collapsedSections: [] };
    expect(migrateUiPrefs(saved, 6)).toBe(saved);
  });
});
