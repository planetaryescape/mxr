/* @vitest-environment jsdom */

import { render } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { usePageTitleStore, usePublishPageTitle } from "./pageTitleStore";

function Publishes({ title }: { title: string | null }) {
  usePublishPageTitle(title);
  return null;
}

describe("usePublishPageTitle", () => {
  test("publishes while mounted and clears on unmount", () => {
    const view = render(<Publishes title="Now" />);
    expect(usePageTitleStore.getState().title).toBe("Now");
    view.unmount();
    expect(usePageTitleStore.getState().title).toBeNull();
  });

  test("a null title publishes nothing", () => {
    usePageTitleStore.setState({ title: null });
    render(<Publishes title={null} />);
    expect(usePageTitleStore.getState().title).toBeNull();
  });

  test("the outgoing page does not clear the title of the page that replaced it", () => {
    const view = render(<Publishes title="Now" />);
    // Route change: the new page's effect runs after the old page's cleanup.
    view.rerender(<Publishes title="Messages" />);
    expect(usePageTitleStore.getState().title).toBe("Messages");
    view.unmount();
    expect(usePageTitleStore.getState().title).toBeNull();
  });
});
