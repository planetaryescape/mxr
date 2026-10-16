/* @vitest-environment jsdom */

import { render } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { useLitReason, useLitReasonStore } from "./litReasonStore";

function Page({ reason, note }: { reason: string | null; note?: string }) {
  useLitReason(reason, note);
  return null;
}

afterEach(() => useLitReasonStore.setState({ owner: null, reason: null, note: null }));

describe("useLitReason", () => {
  test("follows the lit row, and clears when nothing is lit", () => {
    const { rerender } = render(<Page reason="Here because: A" />);
    expect(useLitReasonStore.getState().reason).toBe("Here because: A");
    rerender(<Page reason="Here because: B" note="New sender: 1 Messages" />);
    expect(useLitReasonStore.getState()).toMatchObject({
      reason: "Here because: B",
      note: "New sender: 1 Messages",
    });
    rerender(<Page reason={null} />);
    expect(useLitReasonStore.getState()).toMatchObject({ reason: null, note: null });
  });

  test("clears when the page unmounts, so it never goes stale", () => {
    const { unmount } = render(<Page reason="Here because: A" />);
    unmount();
    expect(useLitReasonStore.getState().reason).toBeNull();
  });

  test("a page unmounting never clears the reason another page has since set", () => {
    const first = render(<Page reason="Here because: Now" />);
    // The next page mounts (and claims the bar) before the old one's cleanup runs.
    render(<Page reason="Here because: Messages" />);
    first.unmount();
    expect(useLitReasonStore.getState().reason).toBe("Here because: Messages");
  });
});
