import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/")({
  beforeLoad: () => {
    throw redirect({ to: "/m/$mailbox", params: { mailbox: "inbox" } });
  },
});
