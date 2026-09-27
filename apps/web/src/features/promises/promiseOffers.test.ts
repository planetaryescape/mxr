import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { PromiseDetection } from "./api";

const api = vi.hoisted(() => ({
  recordPromise: vi.fn<(messageId: string, what: string, dueAt: Date) => Promise<{ id: string }>>(),
}));
vi.mock("./api", () => api);

const { acceptOffer, dismissOffer, offerPromises, usePromiseOffers } =
  await import("./promiseOffers");
const { emitSendEvent } = await import("@/features/compose/session/sendEvents");

const ids = (sendId: string) => ({ intentKey: "compose:reply:m1", sendId });
const beginSend = (sendId: string) => emitSendEvent({ kind: "queued", ...ids(sendId) });
const sendCompleted = (sendId: string, sentMessageId: string) =>
  emitSendEvent({ kind: "sent", ...ids(sendId), sentMessageId });
const sendAbandoned = (sendId: string) => emitSendEvent({ kind: "cancelled", ...ids(sendId) });

const FRIDAY_NINE = "2026-10-02T08:00:00Z";

function detection(): PromiseDetection {
  const choice = {
    at: FRIDAY_NINE,
    local: "2026-10-02T09:00:00+01:00",
    label: "09:00",
    date_label: "Friday 2 October",
    time_label: "09:00",
    relative_label: "in 3 days",
    implied: [],
  };
  return {
    status: "ready",
    promises: [
      {
        what: "send the deck",
        due_phrase: "by Friday",
        due: {
          input: "Friday",
          at: FRIDAY_NINE,
          description: "Friday 2 October, 09:00",
          spans: [],
          choices: [choice],
        },
      },
      { what: "keep you posted" },
    ],
    provenance: { model: "stub-7b", locality: "local", sources: ["your_message"] },
  } as PromiseDetection;
}

beforeEach(() => {
  usePromiseOffers.setState({ offers: [], sends: {} });
  api.recordPromise.mockResolvedValue({ id: "c-1" });
});
afterEach(() => vi.clearAllMocks());

describe("promise offers", () => {
  test("only dated promises are offered", () => {
    beginSend("s1");
    offerPromises("s1", detection());
    const offers = usePromiseOffers.getState().offers;
    expect(offers).toHaveLength(1);
    expect(offers[0]).toMatchObject({ what: "send the deck", duePhrase: "by Friday" });
  });

  test("accepting inside the undo window waits for the sent message, then keeps it", async () => {
    beginSend("s1");
    offerPromises("s1", detection());
    const [offer] = usePromiseOffers.getState().offers;
    acceptOffer(offer!.id);
    expect(api.recordPromise).not.toHaveBeenCalled();
    expect(usePromiseOffers.getState().offers[0]!.accepted).toBe(true);

    sendCompleted("s1", "sent-1");
    await vi.waitFor(() => expect(api.recordPromise).toHaveBeenCalledTimes(1));
    expect(api.recordPromise).toHaveBeenCalledWith(
      "sent-1",
      "send the deck",
      new Date(FRIDAY_NINE),
    );
    expect(usePromiseOffers.getState().offers).toHaveLength(0);
  });

  test("accepting after the send keeps it at the changed time straight away", async () => {
    beginSend("s1");
    sendCompleted("s1", "sent-1");
    offerPromises("s1", detection());
    const [offer] = usePromiseOffers.getState().offers;
    const monday = { ...offer!.choice, at: "2026-10-05T08:00:00Z" };
    acceptOffer(offer!.id, monday);
    await vi.waitFor(() =>
      expect(api.recordPromise).toHaveBeenCalledWith(
        "sent-1",
        "send the deck",
        new Date("2026-10-05T08:00:00Z"),
      ),
    );
  });

  test("an undone send drops its offers and ignores a late answer", () => {
    beginSend("s1");
    offerPromises("s1", detection());
    const [offer] = usePromiseOffers.getState().offers;
    acceptOffer(offer!.id);
    sendAbandoned("s1");
    expect(usePromiseOffers.getState().offers).toHaveLength(0);

    offerPromises("s1", detection());
    expect(usePromiseOffers.getState().offers).toHaveLength(0);
    expect(api.recordPromise).not.toHaveBeenCalled();
  });

  test("not now stores nothing", () => {
    beginSend("s1");
    offerPromises("s1", detection());
    dismissOffer(usePromiseOffers.getState().offers[0]!.id);
    sendCompleted("s1", "sent-1");
    expect(usePromiseOffers.getState().offers).toHaveLength(0);
    expect(api.recordPromise).not.toHaveBeenCalled();
  });

  test("a model that can't answer offers nothing", () => {
    beginSend("s1");
    offerPromises("s1", { status: "timed_out", promises: [] } as PromiseDetection);
    expect(usePromiseOffers.getState().offers).toHaveLength(0);
  });
});
