/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { ReaderSection } from "./ReaderSection";
import { useUiPrefs } from "@/state/uiPrefsStore";

const router = vi.hoisted(() => ({ section: "theme" }));

vi.mock("@tanstack/react-router", () => ({
  useParams: () => ({ section: router.section }),
  Link: ({ children, className }: { children: ReactNode; className?: string }) => (
    <a className={className}>{children}</a>
  ),
}));

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
