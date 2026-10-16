import { useMutation, useQuery } from "@tanstack/react-query";
import { createFileRoute, useParams } from "@tanstack/react-router";
import { FileText } from "lucide-react";
import { toast } from "sonner";

import { fetchAccounts } from "@/features/accounts/api";
import {
  fetchSenderProfile,
  getRecipientBriefing,
  getRelationshipProfile,
  type ThreadBriefing,
} from "@/features/mailbox/api";
import { Page } from "@/components/Page";
import { Button } from "@/components/ui/button";

export const Route = createFileRoute("/sender/$address")({
  component: SenderProfilePage,
});

function SenderProfilePage() {
  const { address } = useParams({ from: "/sender/$address" });
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts });
  const accountId = accounts.data?.accounts[0]?.account_id ?? "";
  const profile = useQuery({
    queryKey: ["sender-profile", accountId, address],
    queryFn: () => fetchSenderProfile({ accountId, email: address }),
    enabled: Boolean(accountId && address),
  });
  const relationship = useQuery({
    queryKey: ["relationship", accountId, address],
    queryFn: () => getRelationshipProfile({ accountId, email: address }),
    enabled: Boolean(accountId && address),
  });
  const briefing = useMutation({
    mutationFn: () => getRecipientBriefing({ accountId, email: address }),
    onError: (error) => toast.error("Briefing failed", { description: error.message }),
  });

  if (!accountId)
    return (
      <Page eyebrow="Sender profile" title={address}>
        <p className="type-body text-text-2">Add an account before opening a sender profile.</p>
      </Page>
    );

  const briefingResult: ThreadBriefing | undefined = briefing.data?.briefing;

  return (
    <Page
      eyebrow="Sender profile"
      title={address}
      description="What your mail says about this sender."
      primary={
        <Button size="sm" disabled={briefing.isPending} onClick={() => briefing.mutate()}>
          <FileText className="size-3" />
          {briefing.isPending ? "Briefing…" : "Recipient briefing"}
        </Button>
      }
      className="space-y-4"
    >
      {briefingResult ? (
        <section className="space-y-2 rounded-md border border-border bg-muted/30 p-4">
          <h2 className="text-sm font-semibold">Recipient briefing</h2>
          <p className="whitespace-pre-wrap break-words text-xs leading-relaxed text-foreground">
            {briefingResult.body_markdown.trim() || "No briefing content."}
          </p>
        </section>
      ) : null}

      <section className="space-y-2">
        <h2 className="text-sm font-semibold">Relationship</h2>
        {relationship.isLoading ? (
          <div className="text-xs text-muted-foreground">Loading relationship…</div>
        ) : relationship.isError ? (
          <div className="text-xs text-destructive">{relationship.error.message}</div>
        ) : (
          <pre className="overflow-auto rounded-md border border-border bg-muted/40 p-4 font-mono text-2xs">
            {JSON.stringify(relationship.data, null, 2)}
          </pre>
        )}
      </section>

      <section className="space-y-2">
        <h2 className="text-sm font-semibold">Profile</h2>
        {profile.isLoading ? (
          <div className="text-xs text-muted-foreground">Loading…</div>
        ) : profile.isError ? (
          <div className="text-xs text-destructive">{profile.error.message}</div>
        ) : (
          <pre className="overflow-auto rounded-md border border-border bg-muted/40 p-4 font-mono text-2xs">
            {JSON.stringify(profile.data, null, 2)}
          </pre>
        )}
      </section>
    </Page>
  );
}
