import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { ReactNode } from "react";

import type { UpdatesDigest } from "./api";
import { digestFixture } from "./testing";
import { UpdatesRoute } from "./UpdatesRoute";

interface DigestState {
  data?: UpdatesDigest;
}
const state = vi.hoisted((): DigestState => ({}));
vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  useDigest: () => ({ data: state.data, isLoading: !state.data, isError: false }),
  useLatestCode: () => ({ data: null }),
}));
vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: ReactNode }) => children,
  useNavigate: () => vi.fn<() => void>(),
  useParams: () => ({}),
}));
vi.mock("@/features/places/PlaceLayout", () => ({
  PlaceLayout: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("@/features/modes/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/modes/api")>()),
  useModeGuide: () => ({ data: undefined }),
}));
vi.mock("@/features/hints/useHint", () => ({
  useHint: () => ({ hint: undefined, dismiss: () => undefined }),
  useActiveHintDismiss: () => undefined,
}));
vi.mock("@/hooks/useSearchParam", () => ({
  useSearchParam: () => [null, vi.fn<() => void>()],
}));

test("a digest subtitle waits for the held skeleton when the guide is unavailable", async () => {
  state.data = undefined;
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const ui = () => (
    <QueryClientProvider client={client}>
      <UpdatesRoute />
    </QueryClientProvider>
  );
  const view = render(ui());
  await screen.findByTestId("page-skeleton");
  state.data = digestFixture({
    header: "A freshly loaded digest subtitle",
    needs_a_look: [],
    changed: [],
    routine: [],
  });
  view.rerender(ui());
  expect(screen.getByTestId("page-skeleton")).toBeInTheDocument();
  expect(screen.queryByText("A freshly loaded digest subtitle")).toBeNull();
  await waitFor(() => expect(screen.queryByTestId("page-skeleton")).toBeNull());
  expect(screen.getByTestId("mode-header")).toHaveTextContent("A freshly loaded digest subtitle");
});
