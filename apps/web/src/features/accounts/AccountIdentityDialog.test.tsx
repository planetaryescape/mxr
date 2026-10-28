/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { RuntimeAccount } from "@/features/compose/api";

import { AccountIdentityDialog } from "./AccountIdentityDialog";
import type { AccountIdentities } from "./useAccountIdentities";

const accountsApi = vi.hoisted(() => ({
  setAccountIdentity: vi.fn<(key: string, identity: unknown) => Promise<unknown>>(),
}));
vi.mock("./api", () => ({ setAccountIdentity: accountsApi.setAccountIdentity }));

const home: RuntimeAccount = {
  account_id: "home",
  key: "home",
  name: "Home",
  email: "bk@home.example",
  provider_kind: "imap",
  enabled: true,
  is_default: false,
};
const work: RuntimeAccount = {
  ...home,
  account_id: "work",
  key: "work",
  name: "Work",
  email: "bk@work.example",
};

const identities: AccountIdentities = {
  byId: new Map([
    ["home", { accountId: "home", name: "Home", email: home.email, tag: "H", slot: "amber" }],
    ["work", { accountId: "work", name: "Work", email: work.email, tag: "W", slot: "sky" }],
  ]),
  accountsById: new Map([
    ["home", home],
    ["work", work],
  ]),
  several: true,
  marksShown: true,
};

function renderDialog() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const onClose = vi.fn<() => void>();
  render(
    <QueryClientProvider client={client}>
      <AccountIdentityDialog account={home} identities={identities} onClose={onClose} />
    </QueryClientProvider>,
  );
  return { onClose };
}

describe("AccountIdentityDialog", () => {
  test("shows the daemon's refusal inline and keeps the chosen colour", async () => {
    accountsApi.setAccountIdentity.mockRejectedValue(new Error("tag W is already held by Work"));
    renderDialog();
    fireEvent.click(screen.getByRole("radio", { name: /^sky/ }));
    fireEvent.change(screen.getByLabelText("Tag"), { target: { value: "w" } });
    expect(screen.getByLabelText("Tag")).toHaveValue("W");
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("tag W is already held by Work");
    expect(accountsApi.setAccountIdentity).toHaveBeenCalledWith("home", {
      color: "sky",
      tag: "W",
    });
    expect(screen.getByRole("radio", { name: /^sky/ })).toHaveAttribute("aria-checked", "true");
  });

  test("names the other accounts that already use the chosen colour", () => {
    renderDialog();
    fireEvent.click(screen.getByRole("radio", { name: /^sky/ }));
    expect(screen.getByText(/sky is also used by Work/)).toBeInTheDocument();
  });

  test("a tag outside one to three letters or digits can't be saved", () => {
    renderDialog();
    fireEvent.change(screen.getByLabelText("Tag"), { target: { value: "" } });
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  test("a saved identity closes the dialog", async () => {
    accountsApi.setAccountIdentity.mockResolvedValue({});
    const { onClose } = renderDialog();
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });
});
