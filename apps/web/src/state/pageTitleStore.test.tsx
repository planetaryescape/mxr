/* @vitest-environment jsdom */

import { render } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { usePageTitleStore, usePublishPageTitle } from "./pageTitleStore";

function Publishes({ title }: { title: string | null }) {
  usePublishPageTitle(title);
  return null;
}

describe("usePublishPageTitle", () => {
  test("the outgoing page does not clear the title of the page that replaced it", () => {
    const view = render(<Publishes title="Now" />);
    view.rerender(<Publishes title="Messages" />);
    expect(usePageTitleStore.getState().title).toBe("Messages");
    view.unmount();
    expect(usePageTitleStore.getState().title).toBeNull();
  });
});
