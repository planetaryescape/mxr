import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { OneTimeCode } from "./api";
import { LatestCode } from "./LatestCode";

function code(overrides: Partial<OneTimeCode> = {}): OneTimeCode {
  return {
    code: "482913",
    current: true,
    expires_at: "2026-10-22T14:42:00Z",
    received_at: "2026-10-22T14:32:00Z",
    sender_name: "Google",
    sender_host: "attacker.example",
    sender_email: "otp@attacker.example",
    subject: "Your verification code",
    account_id: "acct",
    message_id: "msg",
    thread_id: "thread",
    ...overrides,
  };
}

describe("LatestCode", () => {
  test("names the host beside a borrowed brand and copies on click", async () => {
    const writeText = vi.fn<(text: string) => Promise<void>>().mockResolvedValue();
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(<LatestCode code={code()} />);
    const strip = screen.getByTestId("updates-latest-code");
    expect(strip).toHaveTextContent("Google (attacker.example)");
    fireEvent.click(screen.getByRole("button", { name: /copy code 482913/i }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("482913"));
  });

  test("a sender named as its host is named once", () => {
    render(
      <LatestCode code={code({ sender_name: "acme.example", sender_host: "acme.example" })} />,
    );
    expect(screen.getByTestId("updates-latest-code")).not.toHaveTextContent("(acme.example)");
  });
});
