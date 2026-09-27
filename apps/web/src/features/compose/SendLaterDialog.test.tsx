/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, test, vi } from "vitest";

import { SendLaterDialog } from "./SendLaterDialog";

vi.mock("@/features/time/api", async (importOriginal) => {
  const { fakeResolvedTime } = await import("@/features/time/testing");
  return {
    ...(await importOriginal<typeof import("@/features/time/api")>()),
    resolveTime: (input: string) => Promise.resolve(fakeResolvedTime(input)),
  };
});

function renderWithQueryClient(children: ReactNode) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

describe("SendLaterDialog", () => {
  test("previews the daemon's resolution and confirms with that instant", async () => {
    const onConfirm = vi.fn<(at: Date, label: string) => void>();
    renderWithQueryClient(
      <SendLaterDialog open scheduling={false} onOpenChange={() => {}} onConfirm={onConfirm} />,
    );

    fireEvent.change(screen.getByLabelText("Or type a time"), {
      target: { value: "in 2 hours" },
    });
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/in 2 hours$/));

    fireEvent.click(screen.getByRole("button", { name: "Schedule send" }));

    await waitFor(() => expect(onConfirm).toHaveBeenCalledTimes(1));
    const [at, label] = onConfirm.mock.calls[0] ?? [];
    expect((at as Date).getTime() - Date.now()).toBeGreaterThan(7_000_000);
    expect(label).toMatch(/, \d\d:\d\d$/);
  });

  test("keeps confirm disabled for a phrase it can't read", async () => {
    renderWithQueryClient(
      <SendLaterDialog open scheduling={false} onOpenChange={() => {}} onConfirm={() => {}} />,
    );

    fireEvent.change(screen.getByLabelText("Or type a time"), {
      target: { value: "whenever you fancy" },
    });

    expect(await screen.findByText(/Didn't catch "whenever"/)).toBeVisible();
    expect(screen.getByRole("button", { name: "Schedule send" })).toBeDisabled();
  });

  test("presets show their resolved time and confirm without typing", async () => {
    const onConfirm = vi.fn<(at: Date, label: string) => void>();
    renderWithQueryClient(
      <SendLaterDialog open scheduling={false} onOpenChange={() => {}} onConfirm={onConfirm} />,
    );

    const preset = await screen.findByRole("button", { name: /tomorrow 9am.*, 09:00/i });
    fireEvent.click(preset);

    expect(onConfirm).toHaveBeenCalledTimes(1);
    const [at] = onConfirm.mock.calls[0] ?? [];
    expect((at as Date).getHours()).toBe(9);
  });
});
