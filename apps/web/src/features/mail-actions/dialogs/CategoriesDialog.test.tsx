import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { toast } from "sonner";
import { useUndo } from "@/state/undoStore";
import type { MailTarget } from "../target";
import { CategoriesDialog } from "./CategoriesDialog";

const api = vi.hoisted(() => ({
  fetchCategories: vi.fn<typeof import("@/features/mailbox/api").fetchCategories>(),
  fetchCategoryMemberships:
    vi.fn<typeof import("@/features/mailbox/api").fetchCategoryMemberships>(),
  previewCategoryMembership:
    vi.fn<typeof import("@/features/mailbox/api").previewCategoryMembership>(),
  applyCategoryMembership: vi.fn<typeof import("@/features/mailbox/api").applyCategoryMembership>(),
  undoCategoryMembership: vi.fn<typeof import("@/features/mailbox/api").undoCategoryMembership>(),
}));
vi.mock("@/features/mailbox/api", async (original) => ({
  ...(await original<typeof import("@/features/mailbox/api")>()),
  ...api,
}));
vi.mock("sonner", () => ({
  toast: {
    success: vi.fn<(...args: unknown[]) => void>(),
    info: vi.fn<(...args: unknown[]) => void>(),
    warning: vi.fn<(...args: unknown[]) => void>(),
    error: vi.fn<(...args: unknown[]) => void>(),
    dismiss: vi.fn<(...args: unknown[]) => void>(),
  },
}));
const target: MailTarget = {
  messageIds: ["m1", "m2"],
  unstarIds: [],
  rows: [],
  conversations: 2,
  anyStarred: false,
  anyUnread: false,
  labels: [],
  source: "list",
  accountId: "a1",
};
const close = vi.fn<() => void>();
function show() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <CategoriesDialog target={target} onClose={close} />
    </QueryClientProvider>,
  );
}
beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  Element.prototype.scrollIntoView = () => undefined;
  api.fetchCategories.mockResolvedValue([
    { id: "c1", name: "Travel", total_count: 0, unread_count: 0 },
  ]);
  api.fetchCategoryMemberships.mockResolvedValue([
    { category_id: "c1", assigned_messages: 0, selected_messages: 2 },
  ]);
  api.previewCategoryMembership.mockResolvedValue({
    category_ids: ["c1"],
    message_count: 2,
    added: 2,
    removed: 0,
    preview_token: "preview-1",
  });
  api.applyCategoryMembership.mockResolvedValue({ mutation_id: "change-1", added: 2, removed: 0 });
  api.undoCategoryMembership.mockResolvedValue({ restored: 2, skipped: 0 });
  useUndo.getState().clear();
});
afterEach(() => {
  vi.clearAllMocks();
  vi.unstubAllGlobals();
});
async function apply() {
  const option = await screen.findByRole("option", { name: "Travel: not applied" });
  await waitFor(() => expect(option).toHaveAttribute("aria-disabled", "false"));
  fireEvent.click(option);
  fireEvent.click(screen.getByRole("button", { name: "Preview" }));
  fireEvent.click(await screen.findByRole("button", { name: "Apply changes" }));
  await waitFor(() => expect(close).toHaveBeenCalled());
}
test("previews the selection, closes after apply, and exposes the same mutation to keyboard Undo", async () => {
  show();
  await apply();
  expect(api.previewCategoryMembership).toHaveBeenCalledWith({
    accountId: "a1",
    messageIds: ["m1", "m2"],
    addCategoryIds: ["c1"],
    removeCategoryIds: [],
  });
  expect(api.applyCategoryMembership).toHaveBeenCalledWith("preview-1");
  await act(async () => {
    expect(await useUndo.getState().lastUndo?.()).toBe(true);
  });
  expect(api.undoCategoryMembership).toHaveBeenCalledWith("change-1");
  expect(useUndo.getState().lastUndo).toBeNull();
});
test("does not report a changed assignment as fully undone", async () => {
  api.undoCategoryMembership.mockResolvedValue({ restored: 1, skipped: 1 });
  show();
  await apply();
  await act(async () => {
    expect(await useUndo.getState().lastUndo?.()).toBe(false);
  });
  expect(toast.warning).toHaveBeenCalledWith(
    "Restored 1 category assignment",
    expect.objectContaining({ description: expect.stringContaining("1 assignment") }),
  );
});
test("shows Undo failure without reporting success", async () => {
  api.undoCategoryMembership.mockRejectedValue(new Error("Undo expired"));
  show();
  await apply();
  await act(async () => {
    expect(await useUndo.getState().lastUndo?.()).toBe(false);
  });
  expect(toast.error).toHaveBeenCalledWith("Couldn't undo category changes", {
    description: "Undo expired",
  });
  expect(toast.success).not.toHaveBeenCalled();
});
test("Manage Enter is not consumed as a category toggle", async () => {
  show();
  const button = await screen.findByRole("button", { name: "Manage Travel" });
  button.focus();
  expect(fireEvent.keyDown(button, { key: "Enter" })).toBe(true);
  expect(screen.getByRole("option", { name: "Travel: not applied" })).toBeInTheDocument();
  fireEvent.click(button);
  expect(screen.getByRole("heading", { name: "Manage category" })).toBeInTheDocument();
});
test("failed membership loading cannot turn existing assignments into guessed removals", async () => {
  api.fetchCategoryMemberships.mockRejectedValue(new Error("Unavailable"));
  show();
  expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't load current assignments");
  expect(screen.getByRole("option", { name: "Travel: not applied" })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  expect(screen.getByRole("button", { name: "Preview" })).toBeDisabled();
});

test("a preview that becomes a no-op keeps the previous Undo", async () => {
  const previous = vi.fn<() => Promise<boolean>>(async () => true);
  useUndo.getState().recordUndo(previous, "older-change");
  const previousTime = useUndo.getState().lastUndoAt;
  api.applyCategoryMembership.mockResolvedValue({ mutation_id: "", added: 0, removed: 0 });
  show();
  await apply();
  expect(useUndo.getState().lastUndo).toBe(previous);
  expect(useUndo.getState().lastUndoAt).toBe(previousTime);
  expect(useUndo.getState().lastMutationId).toBe("older-change");
});
