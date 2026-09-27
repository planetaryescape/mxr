/* @vitest-environment jsdom */

import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

const runCommand = vi.hoisted(() => vi.fn<(scope: string, command: string) => boolean>());
vi.mock("@/lib/keys/controllers", () => ({ runCommand }));

import { replyIntent, useComposeUi } from "@/features/compose/composeUiStore";

import { ReplyField } from "./ReplyField";

afterEach(() => {
  vi.clearAllMocks();
  act(() => useComposeUi.getState().closeCompose());
});

describe("ReplyField", () => {
  test("clicking the field replies, the same command as r", () => {
    render(<ReplyField name="Maya" canReplyAll />);
    fireEvent.click(screen.getByRole("button", { name: "Reply to Maya…" }));
    expect(runCommand).toHaveBeenCalledWith("reader", "reply");
    fireEvent.click(screen.getByRole("button", { name: /Reply all/ }));
    expect(runCommand).toHaveBeenCalledWith("reader", "replyAll");
    fireEvent.click(screen.getByRole("button", { name: /Forward/ }));
    expect(runCommand).toHaveBeenCalledWith("reader", "forward");
    expect(screen.queryByRole("button", { name: /Draft in your voice/ })).toBeNull();
  });

  test("offers the draft assist when a model is configured", () => {
    const draft = vi.fn<() => void>();
    render(<ReplyField name={null} canReplyAll={false} onDraftInVoice={draft} />);
    expect(screen.getByRole("button", { name: "Reply…" })).toBeVisible();
    expect(screen.queryByRole("button", { name: /Reply all/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Draft in your voice/ }));
    expect(draft).toHaveBeenCalledOnce();
  });

  test("steps aside while the inline composer is open", () => {
    render(<ReplyField name="Maya" canReplyAll />);
    act(() => useComposeUi.getState().openCompose(replyIntent("m1", "single"), "inline"));
    expect(screen.queryByTestId("reply-field")).toBeNull();
  });
});
