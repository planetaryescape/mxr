import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { TokenSection } from "./TokenSection";

const fetchBridge = vi.fn<typeof fetch>();
let submit: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  window.history.replaceState({}, "", "/settings/token?reason=expired");
  vi.stubGlobal("fetch", fetchBridge);
  fetchBridge.mockReset();
  submit = vi.spyOn(HTMLFormElement.prototype, "submit").mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function connect(token: string) {
  fireEvent.change(screen.getByLabelText("Token"), { target: { value: token } });
  fireEvent.click(screen.getByRole("button", { name: /Save and (reload|connect)/ }));
}

test("checks the pasted token before storing it and opens mail without the expired URL", async () => {
  fetchBridge.mockResolvedValue(new Response("{}", { status: 200 }));
  render(<TokenSection />);
  connect("  valid-token  ");

  await waitFor(() => expect(submit).toHaveBeenCalledOnce());
  expect(fetchBridge).toHaveBeenCalledWith(
    `${window.location.origin}/api/v1/admin/status`,
    expect.objectContaining({ headers: { Authorization: "Bearer valid-token" } }),
  );
  expect(window.localStorage.getItem("mxr.bridgeToken")).toBe("valid-token");
  const form = submit.mock.instances[0] as HTMLFormElement;
  expect(new URL(form.action).pathname).toBe("/now");
  expect(new URL(form.action).search).toBe("");
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("a rejected token stays on the form and does not replace the stored token", async () => {
  window.localStorage.setItem("mxr.bridgeToken", "existing-token");
  fetchBridge.mockResolvedValue(new Response("{}", { status: 401 }));
  render(<TokenSection />);
  connect("wrong-token");

  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(/token was rejected/i));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: /Save and (reload|connect)/ })).toBeEnabled(),
  );
  expect(window.localStorage.getItem("mxr.bridgeToken")).toBe("existing-token");
  expect(submit).not.toHaveBeenCalled();
});

test("connection failures do not report that the token is invalid", async () => {
  fetchBridge.mockRejectedValue(new TypeError("Failed to fetch"));
  render(<TokenSection />);
  connect("candidate-token");

  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(/couldn't reach/i));
  expect(window.localStorage.getItem("mxr.bridgeToken")).toBeNull();
  expect(submit).not.toHaveBeenCalled();
});

test("a reachable but failed bridge preserves the token and reports its status", async () => {
  fetchBridge.mockResolvedValue(new Response("{}", { status: 502 }));
  render(<TokenSection />);
  connect("candidate-token");

  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(/502/));
  expect(window.localStorage.getItem("mxr.bridgeToken")).toBeNull();
  expect(submit).not.toHaveBeenCalled();
});

test("browser storage failures do not claim the server rejected the token", async () => {
  fetchBridge.mockResolvedValue(new Response("{}", { status: 200 }));
  vi.spyOn(window, "localStorage", "get").mockImplementation(() => {
    throw new DOMException("Site data blocked", "SecurityError");
  });
  render(<TokenSection />);
  connect("valid-token");

  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(/couldn't save/i));
  expect(submit).not.toHaveBeenCalled();
});
