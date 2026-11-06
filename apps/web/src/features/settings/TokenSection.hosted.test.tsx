import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";

import { TokenSection } from "./TokenSection";

vi.mock("@/hooks/useBridgeToken", () => ({
  useBridgeToken: () => ({
    token: "browser-session",
    hasToken: true,
    setToken: vi.fn<(token: string) => void>(),
    clearToken: vi.fn<() => void>(),
  }),
}));
afterEach(cleanup);

test("hosted sessions reconnect with a launch code without asking for a host bridge token", () => {
  render(<TokenSection />);
  expect(screen.getByRole("button", { name: "Enter connection code" })).toBeInTheDocument();
  expect(screen.queryByLabelText("Token")).not.toBeInTheDocument();
});
