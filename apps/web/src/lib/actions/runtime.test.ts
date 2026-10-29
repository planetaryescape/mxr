import { afterEach, describe, expect, it, vi } from "vitest";

import { useMailboxPane } from "@/state/mailboxPaneStore";

import { getRuntimeNavigate, setRuntimeNavigate } from "./runtime";

afterEach(() => {
  useMailboxPane.getState().setActivePane("mailbox");
});

describe("getRuntimeNavigate", () => {
  it("hands the keys from the sidebar to the page a g place opens", () => {
    const navigate = vi.fn();
    setRuntimeNavigate({ navigate });
    // `h` gave the sidebar the keys; `g a` then goes to All Mail.
    useMailboxPane.getState().setActivePane("sidebar");

    getRuntimeNavigate().navigate("/m/archive");

    expect(navigate).toHaveBeenCalledWith("/m/archive");
    // `j` now moves All Mail's list, not the sidebar's hidden cursor.
    expect(useMailboxPane.getState().activePane).toBe("mailbox");
  });

  it("leaves the reader active when it already has the keys", () => {
    setRuntimeNavigate({ navigate: vi.fn() });
    useMailboxPane.getState().setActivePane("reader");

    getRuntimeNavigate().navigate("/todo?view=catchup");

    expect(useMailboxPane.getState().activePane).toBe("reader");
  });
});
