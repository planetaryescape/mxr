/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { ReaderSection } from "./ReaderSection";
import { SettingsRoute } from "./SettingsRoute";
import { useUiPrefs } from "@/state/uiPrefsStore";

const router = vi.hoisted(() => ({ section: "theme" }));

vi.mock("@tanstack/react-router", () => ({
  useParams: () => ({ section: router.section }),
  Link: ({ children, className }: { children: ReactNode; className?: string }) => (
    <a className={className}>{children}</a>
  ),
}));

describe("SettingsRoute", () => {
  test("an unknown section renders a not-found state, not a blank page", () => {
    router.section = "nonsense";
    render(<SettingsRoute />);

    expect(screen.getByText(/there is no "nonsense" settings page/i)).toBeVisible();
  });

  test("the old density id still opens appearance", () => {
    router.section = "density";
    render(<SettingsRoute />);

    expect(screen.getByRole("heading", { level: 1, name: "Appearance" })).toBeVisible();
  });
});

describe("ReaderSection", () => {
  beforeEach(() => {
    useUiPrefs.setState({ remoteImageSenders: ["news@example.com", "ada@example.com"] });
  });

  test("lists senders whose images always load and removes one", () => {
    render(<ReaderSection />);

    expect(screen.getByText("news@example.com")).toBeVisible();
    fireEvent.click(
      screen.getByRole("button", { name: /stop loading images from news@example.com/i }),
    );

    expect(useUiPrefs.getState().remoteImageSenders).toEqual(["ada@example.com"]);
    expect(screen.queryByText("news@example.com")).not.toBeInTheDocument();
  });
});
