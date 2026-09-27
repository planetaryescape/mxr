/* @vitest-environment jsdom */

import { render } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { ComposeRoute } from "./ComposeRoute";
import { useComposeUi } from "./composeUiStore";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
  back: vi.fn<() => void>(),
  canGoBack: false,
  location: { pathname: "/compose/new", search: {} as Record<string, unknown> },
}));

vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => router.navigate,
  useRouter: () => ({ history: { back: router.back } }),
  useCanGoBack: () => router.canGoBack,
  useRouterState: ({
    select,
  }: {
    select: (state: { location: typeof router.location }) => unknown;
  }) => select({ location: router.location }),
}));

afterEach(() => {
  vi.clearAllMocks();
  router.canGoBack = false;
  useComposeUi.setState({ intent: null, surface: "overlay" });
});

describe("/compose deep links", () => {
  test("a prefilled new-message link opens the compose surface over the inbox", () => {
    router.location = {
      pathname: "/compose/new",
      search: { to: "qa@example.com", subject: "Launch notes" },
    };
    render(<ComposeRoute />);

    expect(useComposeUi.getState().intent).toMatchObject({
      kind: "new",
      title: "New message",
      prefillTo: "qa@example.com",
      prefillSubject: "Launch notes",
    });
    expect(useComposeUi.getState().surface).toBe("overlay");
    expect(router.navigate).toHaveBeenCalledWith({
      to: "/m/$mailbox",
      params: { mailbox: "inbox" },
      replace: true,
    });
  });

  test("a stored draft link opens that draft", () => {
    router.location = { pathname: "/compose/draft-42", search: {} };
    render(<ComposeRoute />);

    expect(useComposeUi.getState().intent).toMatchObject({
      key: "draft:draft-42",
      draftId: "draft-42",
    });
  });

  test("a reply link shares the thread reply's draft", () => {
    router.location = { pathname: "/compose/new", search: { reply: "m-7", mode: "all" } };
    render(<ComposeRoute />);

    expect(useComposeUi.getState().intent).toMatchObject({
      key: "compose:reply_all:m-7",
      kind: "reply_all",
      messageId: "m-7",
    });
  });

  test("an in-app link returns to the page it came from", () => {
    router.canGoBack = true;
    router.location = { pathname: "/compose/new", search: {} };
    render(<ComposeRoute />);

    expect(useComposeUi.getState().intent?.key).toBe("compose:new:new");
    expect(router.back).toHaveBeenCalledTimes(1);
    expect(router.navigate).not.toHaveBeenCalled();
  });
});
