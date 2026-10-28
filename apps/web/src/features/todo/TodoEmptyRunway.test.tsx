import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { TodoEmptyRunway } from "./TodoEmptyRunway";
import { runwayFixture } from "./testing";
import { openCount } from "./todoRows";

test("only uncertain asks keep the empty runway and a collapsed source disclosure", () => {
  const runway = runwayFixture({
    now: [],
    might_need_you: [
      {
        thread_id: "uncertain-thread",
        subject: "The proposed review",
        asks: {
          message_id: "uncertain-message",
          input_fingerprint: "fingerprint",
          policy_version: "asks-r2c-3",
          pending_body: false,
          more_units: false,
          units: [
            {
              unit: 0,
              zone: 0,
              span: { start: 0, end: 23 },
              answer: "not_sure",
              quote: "You might review this.",
            },
          ],
        },
      },
    ],
  });
  const openSource = vi.fn<(threadId: string) => void>();
  render(
    <TodoEmptyRunway
      runway={runway}
      body="No confirmed tasks."
      onOpenSource={openSource}
    />,
  );
  expect(screen.getByText("Nothing here yet")).toBeVisible();
  expect(openCount(runway)).toBe(0);
  expect(screen.queryByRole("list", { name: "Now" })).not.toBeInTheDocument();
  const disclosure = screen.getByTestId("might-need-you");
  expect(disclosure).not.toHaveAttribute("open");
  fireEvent.click(screen.getByText("Might need you · 1"));
  expect(disclosure).toHaveAttribute("open");
  expect(screen.getByText("You might review this.")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "The proposed review" }));
  expect(openSource).toHaveBeenCalledWith("uncertain-thread");
});
