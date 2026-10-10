import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Clock, FileText, Mail, Paperclip, Plus, Trash2 } from "lucide-react";
import type { MouseEvent } from "react";
import { toast } from "sonner";

import { deleteDraft, fetchDrafts, type DraftSummary } from "./api";
import { OrphanedDrafts } from "./OrphanedDrafts";
import { ScheduledSends } from "./ScheduledSends";
import { Page, PageSection } from "@/components/Page";
import { PageEmpty, PageError, PageSkeleton } from "@/components/PageParts";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { formatLongDate } from "@/lib/format";
import { draftIntent, newMessageIntent, useComposeUi } from "@/features/compose/composeUiStore";

export function DraftsRoute() {
  const queryClient = useQueryClient();
  const drafts = useQuery({ queryKey: ["drafts"], queryFn: fetchDrafts });
  const remove = useMutation({
    mutationFn: deleteDraft,
    onSuccess: () => {
      toast.success("Draft deleted");
      void queryClient.invalidateQueries({ queryKey: ["drafts"] });
    },
    onError: (error) => toast.error("Delete failed", { description: error.message }),
  });

  const rows = drafts.data?.drafts ?? [];
  return (
    <Page
      title="Drafts"
      description="Drafts saved in mxr. Open one to keep writing."
      actions={
        <Button
          size="sm"
          onClick={() => useComposeUi.getState().openCompose(newMessageIntent(), "overlay")}
        >
          <Plus className="size-3" />
          New draft
        </Button>
      }
    >
      <ScheduledSends />
      <OrphanedDrafts />
      <PageSection title="Drafts">
        {drafts.isPending ? (
          <PageSkeleton rows={5} label="Loading drafts" />
        ) : drafts.isError ? (
          <PageError
            title="Drafts unavailable"
            error={drafts.error}
            onRetry={() => void drafts.refetch()}
          />
        ) : rows.length === 0 ? (
          <PageEmpty
            icon={<Mail className="size-5" />}
            title="No saved drafts"
            body="Drafts persisted in mxr’s local store appear here."
          />
        ) : (
          <ul data-testid="draft-list" aria-label="Drafts">
            {rows.map((draft) => (
              <DraftRow
                key={draft.id}
                draft={draft}
                deleting={remove.isPending}
                onDelete={() => remove.mutate(draft)}
              />
            ))}
          </ul>
        )}
      </PageSection>
    </Page>
  );
}

function DraftRow({
  draft,
  deleting,
  onDelete,
}: {
  draft: DraftSummary;
  deleting: boolean;
  onDelete: () => void;
}) {
  const subject = draft.subject.trim() || "(no subject)";
  return (
    <li className="flex items-center border-b border-border/60 pr-2 hover:bg-muted/40">
      <Link
        to="/compose/$draftId"
        params={{ draftId: draft.id }}
        onClick={(event: MouseEvent<HTMLAnchorElement>) => openDraftInPlace(event, draft.id)}
        className="flex min-w-0 flex-1 items-center gap-3 px-2 py-2.5 outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <FileText className="size-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px] font-medium">{subject}</div>
          <div className="mt-0.5 truncate font-mono text-2xs text-muted-foreground">
            {draft.recipients || "No recipients"}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-3 text-2xs text-muted-foreground">
          {draft.send_at ? (
            <span
              className="inline-flex items-center gap-1 text-primary"
              title={formatLongDate(draft.send_at)}
            >
              <Clock className="size-3" aria-hidden="true" />
              scheduled
            </span>
          ) : null}
          {draft.content_kind === "html" ? (
            <span className="rounded bg-muted px-1.5 py-0.5 font-mono uppercase">HTML</span>
          ) : null}
          {draft.attachment_count > 0 ? (
            <span className="inline-flex items-center gap-1">
              <Paperclip className="size-3" />
              {draft.attachment_count}
            </span>
          ) : null}
          <time dateTime={draft.updated_at} title={draft.updated_at_full}>
            {draft.updated_at_relative}
          </time>
        </div>
      </Link>
      <AlertDialog>
        <AlertDialogTrigger asChild>
          <Button
            variant="ghost"
            size="icon-sm"
            disabled={deleting}
            aria-label={`Delete draft ${subject}`}
          >
            <Trash2 className="size-3.5" />
          </Button>
        </AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete this draft?</AlertDialogTitle>
            <AlertDialogDescription>
              “{subject}” will be permanently removed from mxr’s local draft store.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={onDelete}>
              Delete draft
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </li>
  );
}

/** A plain click opens the draft in the compose surface over this list;
 * modified clicks keep the link's deep-link behaviour (new tab, etc.). */
function openDraftInPlace(event: MouseEvent<HTMLAnchorElement>, draftId: string) {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
    return;
  }
  event.preventDefault();
  useComposeUi.getState().openCompose(draftIntent(draftId), "overlay");
}
