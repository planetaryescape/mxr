import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

describe("App", () => {
  beforeEach(() => {
    Object.defineProperty(window, "mxrDesktop", {
      value: {
        getBridgeState: vi.fn().mockResolvedValue({
          kind: "mismatch",
          binaryPath: "/usr/local/bin/mxr",
          usingBundled: false,
          daemonVersion: "0.4.2",
          actualProtocol: 0,
          requiredProtocol: 1,
          updateSteps: [
            "Homebrew: brew upgrade mxr",
            "Release install: rerun ./install.sh",
            "Source install: git pull && cargo install --path crates/daemon --locked",
          ],
          detail: "mxr Desktop needs a compatible version of mxr before it can connect.",
        }),
        retryBridge: vi.fn(),
        useBundledMxr: vi.fn(),
        setExternalBinaryPath: vi.fn(),
      },
      configurable: true,
    });
  });

  it("renders mismatch guidance with update steps", async () => {
    render(<App />);

    expect(
      await screen.findByText("mxr Desktop needs a compatible version of mxr"),
    ).toBeInTheDocument();
    expect(screen.getByText("Homebrew: brew upgrade mxr")).toBeInTheDocument();
    expect(screen.getByText("Use bundled mxr")).toBeInTheDocument();
  });
});
