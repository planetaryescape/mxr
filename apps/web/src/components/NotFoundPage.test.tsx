/* @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import type { AnchorHTMLAttributes, ReactNode } from "react";
import { describe, expect, test, vi } from "vitest";

import { NotFoundPage } from "./NotFoundPage";

const router = vi.hoisted(() => ({ pathname: "/nope-route" }));

vi.mock("@tanstack/react-router", () => ({
  useRouterState: ({ select }: { select: (state: unknown) => unknown }) =>
    select({ location: { pathname: router.pathname } }),
  Link: ({
    to,
    params,
    children,
    ...rest
  }: AnchorHTMLAttributes<HTMLAnchorElement> & {
    to: string;
    params?: Record<string, string>;
    children: ReactNode;
  }) => (
    <a
      href={Object.entries(params ?? {}).reduce(
        (href, [key, value]) => href.replace(`$${key}`, value),
        to,
      )}
      {...rest}
    >
      {children}
    </a>
  ),
}));

describe("NotFoundPage", () => {
  test("offers the two places people go", () => {
    render(<NotFoundPage />);

    expect(screen.getByRole("link", { name: "Go to Now" })).toHaveAttribute("href", "/now");
    expect(screen.getByRole("link", { name: "Open Inbox" })).toHaveAttribute("href", "/m/inbox");
  });
});
