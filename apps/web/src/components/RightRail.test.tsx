/* @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { RightRail } from "./RightRail";
import { useModals } from "@/state/modalStore";

vi.mock("@/features/thread/AttachmentActions", () => ({
  AttachmentActions: () => null,
}));

describe("RightRail", () => {
  afterEach(() => {
    useModals.setState({ rightRail: null });
  });

  test("renders sender profiles as relationship stats instead of raw JSON", () => {
    useModals.setState({
      rightRail: {
        kind: "sender-profile",
        payload: {
          kind: "SenderProfile",
          profile: {
            account_id: "account-1",
            email: "sender@example.com",
            display_name: "Sender Example",
            first_seen_at: "2026-05-01T10:00:00Z",
            last_seen_at: "2026-05-11T10:00:00Z",
            last_inbound_at: "2026-05-11T10:00:00Z",
            last_outbound_at: "2026-05-10T10:00:00Z",
            total_inbound: 5,
            total_outbound: 2,
            replied_count: 2,
            cadence_days_p50: 1.5,
            is_list_sender: false,
            list_id: null,
            open_thread_count: 3,
            inbound_storage_bytes: 4096,
            outbound_storage_bytes: 1024,
            attachment_count: 2,
            attachment_bytes: 2048,
          },
        },
      },
    });

    render(<RightRail />);

    expect(screen.getByRole("heading", { name: "Sender Example" })).toBeVisible();
    expect(screen.getByText("sender@example.com")).toBeVisible();
    expect(screen.getByText("Mostly inbound")).toBeVisible();
    expect(screen.getByText("5 in / 2 out")).toBeVisible();
    expect(screen.getByText(/They send \+3 KB/)).toBeVisible();
    expect(screen.queryByText(/"kind"/)).not.toBeInTheDocument();
  });
});
