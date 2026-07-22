/* @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { RuntimeAccount } from "./api";
import { ComposeTopBar } from "./ComposeTopBar";

function account(overrides: Partial<RuntimeAccount> = {}): RuntimeAccount {
  return {
    account_id: "acct-1",
    name: "Bhekani.com",
    email: "hello@planetaryescape.xyz",
    provider_kind: "imap",
    enabled: true,
    is_default: true,
    ...overrides,
  };
}

function renderTopBar(props: Partial<React.ComponentProps<typeof ComposeTopBar>> = {}) {
  return render(
    <ComposeTopBar
      title="New message"
      busy={false}
      canServerSave={false}
      onRefresh={vi.fn<() => void>()}
      onServerSave={vi.fn<() => void>()}
      onDiscard={vi.fn<() => void>()}
      accounts={[account()]}
      accountId="acct-1"
      onAccountChange={vi.fn<(id: string) => void>()}
      addresses={["hello@planetaryescape.xyz"]}
      fromAddress="hello@planetaryescape.xyz"
      onFromChange={vi.fn<(email: string) => void>()}
      {...props}
    />,
  );
}

describe("ComposeTopBar send-as picker", () => {
  test("single account, single address: shows the address plainly, no picker", () => {
    renderTopBar();

    expect(screen.getByText("hello@planetaryescape.xyz")).toBeVisible();
    expect(screen.queryByRole("combobox", { name: "Send as address" })).not.toBeInTheDocument();
  });

  test("multiple aliases on one account: renders the send-as picker with each alias", () => {
    renderTopBar({
      addresses: [
        "hello@planetaryescape.xyz",
        "admin@planetaryescape.xyz",
        "legal@planetaryescape.xyz",
      ],
      fromAddress: "admin@planetaryescape.xyz",
    });

    const picker = screen.getByRole("combobox", { name: "Send as address" });
    expect(picker).toBeVisible();
    // The trigger reflects the currently selected alias.
    expect(picker).toHaveTextContent("admin@planetaryescape.xyz");
  });

  test("a from-address not in the alias list falls back to the first alias for display", () => {
    renderTopBar({
      addresses: ["hello@planetaryescape.xyz", "admin@planetaryescape.xyz"],
      fromAddress: "stale@example.com",
    });

    const picker = screen.getByRole("combobox", { name: "Send as address" });
    expect(picker).toHaveTextContent("hello@planetaryescape.xyz");
  });
});
